//! Import straight from Microsoft To Do: sign in with Microsoft, read every list and task
//! through Microsoft Graph, and hand them to the usual import (import.rs) as if they came
//! from an export file.
//!
//! RustDo asks only for `Tasks.Read`: it reads To Do and never changes anything there. The
//! sign-in token is used once and not kept.
//!
//! * Desktop and Android: the device code flow. RustDo shows a short code; the person
//!   enters it at microsoft.com/devicelogin in any browser and signs in there.
//! * Web app: the authorization code flow with PKCE. The page goes to Microsoft's sign-in
//!   and comes back to rustdo.app with a code, which [`resume_web`] finishes.
//!
//! The app registration ("RustDo" in KoebleEffect's Microsoft Entra tenant) allows personal
//! and work or school accounts, public client flows, and the SPA redirect https://rustdo.app/.

use crate::i18n::{tr, trf};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

/// RustDo's app registration (public: it identifies the app, it isn't a secret).
pub const CLIENT_ID: &str = "cf3c5938-2328-4a94-95bb-dabfa657f064";
const AUTHORITY: &str = "https://login.microsoftonline.com/common/oauth2/v2.0";
const SCOPE: &str = "https://graph.microsoft.com/Tasks.Read";
const GRAPH: &str = "https://graph.microsoft.com/v1.0";
#[cfg(target_arch = "wasm32")]
const REDIRECT: &str = "https://rustdo.app/";

/// What the sign-in and download report back to the app.
pub enum Event {
    /// Desktop and Android: show this code and where to enter it.
    Code { user_code: String, url: String },
    /// Progress, e.g. "Reading “Groceries”…".
    Status(String),
    /// Everything read, as Graph JSON (`{"lists": [...]}`) for `import::parse_graph_json`,
    /// or what went wrong.
    Done(Result<String, String>),
}

/// A sign-in in progress.
pub struct Session {
    pub rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

impl Session {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn form(fields: &[(&str, &str)]) -> Vec<u8> {
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn post_form(url: &str, fields: &[(&str, &str)]) -> ehttp::Request {
    let mut r = ehttp::Request::post(url, form(fields));
    r.headers.insert("Content-Type", "application/x-www-form-urlencoded");
    r
}

fn graph_get(url: &str, token: &str) -> ehttp::Request {
    let mut r = ehttp::Request::get(url);
    r.headers.insert("Authorization", format!("Bearer {token}"));
    r
}

fn json_of(r: &ehttp::Response) -> Value {
    serde_json::from_slice(&r.bytes).unwrap_or(Value::Null)
}

/// A short reason from a Microsoft error answer.
fn error_text(v: &Value, status: u16) -> String {
    v.get("error_description")
        .and_then(Value::as_str)
        .map(|d| d.lines().next().unwrap_or(d).to_string())
        .or_else(|| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| trf("Microsoft answered with an error ({status}).", &[("status", &status)]))
}

/// Puts the lists and their tasks together the way tools/Export-MicrosoftToDo.ps1 does.
fn assemble(lists: Vec<(Value, Vec<Value>)>) -> String {
    let lists: Vec<Value> = lists
        .into_iter()
        .map(|(mut list, tasks)| {
            if let Some(obj) = list.as_object_mut() {
                obj.insert("tasks".into(), Value::Array(tasks));
            }
            list
        })
        .collect();
    serde_json::json!({ "lists": lists }).to_string()
}

fn lists_url() -> String {
    format!("{GRAPH}/me/todo/lists")
}

fn tasks_url(list: &Value) -> Option<String> {
    let id = list.get("id")?.as_str()?;
    Some(format!("{GRAPH}/me/todo/lists/{}/tasks?$expand=checklistItems&$top=100", urlencode(id)))
}

fn list_name(list: &Value) -> String {
    list.get("displayName").and_then(Value::as_str).unwrap_or_default().to_string()
}

// ---------------------------------------------------------------- desktop and Android

/// Starts signing in (device code) and reading To Do, on its own thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn start(ctx: &eframe::egui::Context) -> Session {
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let (stop, ctx) = (cancel.clone(), ctx.clone());
    std::thread::spawn(move || {
        let send = |e: Event| {
            let _ = tx.send(e);
            ctx.request_repaint();
        };
        let result = device_code_import(&send, &stop);
        if !stop.load(Ordering::Relaxed) {
            send(Event::Done(result));
        }
    });
    Session { rx, cancel }
}

#[cfg(not(target_arch = "wasm32"))]
fn device_code_import(send: &dyn Fn(Event), stop: &AtomicBool) -> Result<String, String> {
    let fetch = |r: &ehttp::Request| ehttp::fetch_blocking(r).map_err(|_| tr("Couldn't reach Microsoft. Check your internet connection.").to_string());

    let r = fetch(&post_form(&format!("{AUTHORITY}/devicecode"), &[("client_id", CLIENT_ID), ("scope", SCOPE)]))?;
    let v = json_of(&r);
    if !r.ok {
        return Err(error_text(&v, r.status));
    }
    let (Some(user_code), Some(device_code)) = (v["user_code"].as_str(), v["device_code"].as_str()) else {
        return Err(error_text(&v, r.status));
    };
    let url = v["verification_uri"].as_str().unwrap_or("https://microsoft.com/devicelogin").to_string();
    send(Event::Code { user_code: user_code.into(), url });
    let mut interval = v["interval"].as_u64().unwrap_or(5).max(1);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(v["expires_in"].as_u64().unwrap_or(900));

    // Wait for the person to sign in.
    let token = loop {
        for _ in 0..interval * 4 {
            if stop.load(Ordering::Relaxed) {
                return Err(String::new());
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        if std::time::Instant::now() > deadline {
            return Err(tr("The sign-in code expired. Start again.").into());
        }
        let r = fetch(&post_form(
            &format!("{AUTHORITY}/token"),
            &[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", CLIENT_ID), ("device_code", device_code)],
        ))?;
        let v = json_of(&r);
        if let Some(t) = v["access_token"].as_str() {
            break t.to_string();
        }
        match v["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("authorization_declined") => return Err(tr("Sign-in was cancelled.").into()),
            Some("expired_token") => return Err(tr("The sign-in code expired. Start again.").into()),
            _ => return Err(error_text(&v, r.status)),
        }
    };

    send(Event::Status(tr("Signed in. Reading your lists…").into()));
    let get_all = |first: String| -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        let mut next = Some(first);
        while let Some(url) = next.take() {
            if stop.load(Ordering::Relaxed) {
                return Err(String::new());
            }
            let r = fetch(&graph_get(&url, &token))?;
            let v = json_of(&r);
            if !r.ok {
                return Err(error_text(&v, r.status));
            }
            items.extend(v["value"].as_array().cloned().unwrap_or_default());
            next = v["@odata.nextLink"].as_str().map(str::to_string);
        }
        Ok(items)
    };
    let mut out = Vec::new();
    for list in get_all(lists_url())? {
        send(Event::Status(trf("Reading “{name}”…", &[("name", &list_name(&list))])));
        let tasks = match tasks_url(&list) {
            Some(url) => get_all(url)?,
            None => Vec::new(),
        };
        out.push((list, tasks));
    }
    Ok(assemble(out))
}

// ---------------------------------------------------------------- web app

/// Web: goes to Microsoft's sign-in page. RustDo comes back to rustdo.app afterwards and
/// [`resume_web`] carries on.
#[cfg(target_arch = "wasm32")]
pub fn start(_ctx: &eframe::egui::Context) -> Session {
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let state = uuid::Uuid::new_v4().simple().to_string();
    let challenge = base64url(&sha256(verifier.as_bytes()));
    let storage = web_sys::window().and_then(|w| w.session_storage().ok().flatten());
    let saved = storage.as_ref().is_some_and(|s| {
        s.set_item("rustdo.mstodo.verifier", &verifier).is_ok() && s.set_item("rustdo.mstodo.state", &state).is_ok()
    });
    if !saved {
        let _ = tx.send(Event::Done(Err(tr("This browser won't let RustDo sign in (storage is blocked).").into())));
        return Session { rx, cancel };
    }
    let url = format!(
        "{AUTHORITY}/authorize?client_id={CLIENT_ID}&response_type=code&response_mode=fragment&redirect_uri={}&scope={}&state={state}&code_challenge={challenge}&code_challenge_method=S256&prompt=select_account",
        urlencode(REDIRECT),
        urlencode(SCOPE)
    );
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_href(&url);
    }
    let _ = tx.send(Event::Status(tr("Opening Microsoft sign-in…").into()));
    Session { rx, cancel }
}

/// Web: if the page was opened by Microsoft's sign-in (`#code=…&state=…`), finishes it and
/// reads To Do. `None` when there's nothing to resume.
#[cfg(target_arch = "wasm32")]
pub fn resume_web(ctx: &eframe::egui::Context) -> Option<Session> {
    let window = web_sys::window()?;
    let hash = window.location().hash().ok()?;
    let params: std::collections::HashMap<String, String> = hash
        .trim_start_matches('#')
        .split('&')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_string(), urldecode(v)))
        .collect();
    let storage = window.session_storage().ok().flatten()?;
    let expected = storage.get_item("rustdo.mstodo.state").ok().flatten()?;
    if params.get("state") != Some(&expected) {
        return None;
    }
    let verifier = storage.get_item("rustdo.mstodo.verifier").ok().flatten().unwrap_or_default();
    let _ = storage.remove_item("rustdo.mstodo.state");
    let _ = storage.remove_item("rustdo.mstodo.verifier");
    // Take the code out of the address bar.
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/"));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let ctx = ctx.clone();
    let code = params.get("code").cloned();
    let failure = params.get("error_description").or(params.get("error")).cloned();
    wasm_bindgen_futures::spawn_local(async move {
        let send = |e: Event| {
            let _ = tx.send(e);
            ctx.request_repaint();
        };
        let result = match (code, failure) {
            (Some(code), _) => web_import(&send, &code, &verifier).await,
            (None, Some(f)) if f.contains("access_denied") || f.contains("cancel") => Err(tr("Sign-in was cancelled.").into()),
            (None, Some(f)) => Err(f),
            (None, None) => Err(tr("Sign-in didn't finish. Try again.").into()),
        };
        send(Event::Done(result));
    });
    Some(Session { rx, cancel })
}

#[cfg(target_arch = "wasm32")]
async fn web_import(send: &dyn Fn(Event), code: &str, verifier: &str) -> Result<String, String> {
    let fetch = |r: ehttp::Request| async move {
        ehttp::fetch_async(r).await.map_err(|_| tr("Couldn't reach Microsoft. Check your internet connection.").to_string())
    };
    send(Event::Status(tr("Signing in…").into()));
    let r = fetch(post_form(
        &format!("{AUTHORITY}/token"),
        &[
            ("client_id", CLIENT_ID),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("code_verifier", verifier),
            ("scope", SCOPE),
        ],
    ))
    .await?;
    let v = json_of(&r);
    let Some(token) = v["access_token"].as_str().map(str::to_string) else {
        return Err(error_text(&v, r.status));
    };
    send(Event::Status(tr("Signed in. Reading your lists…").into()));
    let get_all = |first: String| {
        let token = token.clone();
        async move {
            let mut items = Vec::new();
            let mut next = Some(first);
            while let Some(url) = next.take() {
                let r = ehttp::fetch_async(graph_get(&url, &token))
                    .await
                    .map_err(|_| tr("Couldn't reach Microsoft. Check your internet connection.").to_string())?;
                let v = json_of(&r);
                if !r.ok {
                    return Err(error_text(&v, r.status));
                }
                items.extend(v["value"].as_array().cloned().unwrap_or_default());
                next = v["@odata.nextLink"].as_str().map(str::to_string);
            }
            Ok::<_, String>(items)
        }
    };
    let mut out = Vec::new();
    for list in get_all(lists_url()).await? {
        send(Event::Status(trf("Reading “{name}”…", &[("name", &list_name(&list))])));
        let tasks = match tasks_url(&list) {
            Some(url) => get_all(url).await?,
            None => Vec::new(),
        };
        out.push((list, tasks));
    }
    Ok(assemble(out))
}

#[cfg(target_arch = "wasm32")]
fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(target_arch = "wasm32")]
fn base64url(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (*b as u32) << (16 - 8 * i));
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(target_arch = "wasm32")]
fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(data).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembled_lists_import_like_an_export_file() {
        let list = serde_json::json!({ "id": "L1", "displayName": "Groceries", "wellknownListName": "none" });
        let task = serde_json::json!({
            "id": "T1", "title": "Milk", "status": "notStarted", "importance": "high",
            "checklistItems": [{ "displayName": "2%", "isChecked": false }]
        });
        let json = assemble(vec![(list, vec![task])]);
        let import = crate::import::parse_graph_json(&json, "x", |n| n).unwrap();
        assert_eq!(import.lists.len(), 1);
        assert_eq!(import.lists[0].name, "Groceries");
        assert_eq!(import.lists[0].tasks[0].title, "Milk");
        assert!(import.lists[0].tasks[0].important);
        assert_eq!(import.lists[0].tasks[0].steps.len(), 1);
    }

    #[test]
    fn form_values_are_encoded() {
        assert_eq!(String::from_utf8(form(&[("scope", "https://graph.microsoft.com/Tasks.Read")])).unwrap(), "scope=https%3A%2F%2Fgraph.microsoft.com%2FTasks.Read");
    }
}
