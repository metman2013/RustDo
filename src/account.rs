//! "Sync my devices": one person's lists, tasks, sticky notes and settings, kept the same
//! on all their devices through their own RustDo Hub (see docs/hub.md).
//!
//! An account is a random **sync key** the hub hands out; any device that has the key
//! and can reach the hub syncs it. Like family sharing (sync.rs), nothing tracks single
//! edits: we keep a fingerprint of every item as of the last sync, send whatever changed
//! or disappeared, and apply what the hub says other devices changed. The newest edit of
//! an item wins.
//!
//! What's an item:
//! * `list`: a list's name (lists shared with family are left out, see `share`);
//! * `task`: a task in one of those lists, except "reminder already shown" (per device);
//! * `share`: being in a family-shared list (its hub and code), so this person's other
//!   devices join it too. The shared list's tasks sync through its own share code, never
//!   through the account, so nothing private reaches the people it's shared with;
//! * `note`: a sticky note, except where it sits on this device's screen and whether it's
//!   showing there;
//! * `settings`: sort order, theme, 12/24-hour clock and which list is the default.

use crate::model::{Store, Task, TaskList};
use crate::notes::{Notes, StickyNote};
use crate::sync::Share;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Task fields that stay on each device.
const DEVICE_TASK_FIELDS: [&str; 1] = ["reminder_fired"];
/// Sticky note fields that stay on each device.
const DEVICE_NOTE_FIELDS: [&str; 3] = ["pos", "open", "on_top"];
/// Settings that sync (the rest, like backups and the background companion, are per device).
const SYNCED_SETTINGS: [&str; 3] = ["sort", "theme", "clock_24h"];
const SETTINGS_ID: &str = "settings";

/// This device's link to an account (saved with the tasks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// e.g. `https://todo.example.com` or `http://192.168.1.20:7979`
    pub hub: String,
    /// e.g. `ABCDE-FGHJK-MNPQR-STUVW`
    pub key: String,
    /// The hub revision this device has caught up to.
    #[serde(default)]
    pub rev: u64,
    /// Fingerprint of each item (`kind:id`) as last synced.
    #[serde(default)]
    pub known: HashMap<String, u64>,
    #[serde(default)]
    pub last_sync: Option<NaiveDateTime>,
    /// The first round of a device that joined an account and started empty: take what
    /// the account has, send nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pull_only: bool,
}

impl Account {
    pub fn new(hub: String, key: String) -> Self {
        Self { hub, key, rev: 0, known: HashMap::new(), last_sync: None, pull_only: false }
    }
}

/// What goes over the wire (the hub's `Item`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub kind: String,
    pub id: String,
    pub modified: i64,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub rev: u64,
}

impl Item {
    fn key(&self) -> String {
        key(&self.kind, &self.id)
    }
}

fn key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncRequest {
    pub key: String,
    pub since: u64,
    pub changes: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncResponse {
    pub rev: u64,
    #[serde(default)]
    pub changes: Vec<Item>,
}

/// One round in flight: what was sent, so the answer can be applied safely.
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub request: SyncRequest,
    /// Fingerprint of each item as sent (`None` = sent as deleted).
    pub sent: HashMap<String, Option<u64>>,
}

/// FNV-1a: the same fingerprint everywhere (see sync.rs).
fn fingerprint(value: &Value) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in value.to_string().as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn without(mut v: Value, fields: &[&str]) -> Value {
    if let Some(obj) = v.as_object_mut() {
        for f in fields {
            obj.remove(*f);
        }
    }
    v
}

fn task_data(task: &Task) -> Value {
    without(serde_json::to_value(task).unwrap_or(Value::Null), &["id"]).pipe(|v| without(v, &DEVICE_TASK_FIELDS))
}

fn note_data(note: &StickyNote) -> Value {
    without(serde_json::to_value(note).unwrap_or(Value::Null), &["id"]).pipe(|v| without(v, &DEVICE_NOTE_FIELDS))
}

/// A list's name and its place in the menu (so every device shows lists in one order).
fn list_data(store: &Store, list: &TaskList) -> Value {
    let pos = store.lists.iter().position(|l| l.id == list.id).unwrap_or_default();
    serde_json::json!({ "name": list.name, "pos": pos })
}

fn share_data(share: &Share, store: &Store) -> Value {
    serde_json::json!({ "hub": share.hub, "code": share.code, "name": store.list_name(share.list_id) })
}

fn settings_data(store: &Store) -> Value {
    let all = serde_json::to_value(&store.settings).unwrap_or(Value::Null);
    let mut out = serde_json::Map::new();
    for f in SYNCED_SETTINGS {
        if let Some(v) = all.get(f) {
            out.insert(f.into(), v.clone());
        }
    }
    out.insert("default_list".into(), serde_json::to_value(store.default_list).unwrap_or(Value::Null));
    Value::Object(out)
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl Pipe for Value {}

fn shared_lists(store: &Store) -> HashSet<Uuid> {
    store.shares.iter().map(|s| s.list_id).collect()
}

/// Everything this device has to sync right now: `kind:id` → (kind, id, data).
fn current(store: &Store, notes: Option<&Notes>) -> HashMap<String, (&'static str, String, Value)> {
    let shared = shared_lists(store);
    let mut out = HashMap::new();
    let mut put = |kind: &'static str, id: String, data: Value| {
        out.insert(key(kind, &id), (kind, id, data));
    };
    put("settings", SETTINGS_ID.into(), settings_data(store));
    for l in store.lists.iter().filter(|l| !shared.contains(&l.id)) {
        put("list", l.id.to_string(), list_data(store, l));
    }
    for t in store.tasks.iter().filter(|t| !shared.contains(&t.list_id)) {
        put("task", t.id.to_string(), task_data(t));
    }
    for s in &store.shares {
        put("share", s.list_id.to_string(), share_data(s, store));
    }
    for n in notes.map(|n| n.notes.as_slice()).unwrap_or_default() {
        put("note", n.id.to_string(), note_data(n));
    }
    out
}

/// The fingerprint of one item as it is on this device now (`None`: not here).
fn local_fingerprint(store: &Store, notes: Option<&Notes>, kind: &str, id: &str) -> Option<u64> {
    let uuid = || Uuid::parse_str(id).ok();
    let shared = || shared_lists(store);
    let data = match kind {
        "settings" => Some(settings_data(store)),
        "list" => uuid().filter(|u| !shared().contains(u)).and_then(|u| store.lists.iter().find(|l| l.id == u)).map(|l| list_data(store, l)),
        "task" => uuid().and_then(|u| store.tasks.iter().find(|t| t.id == u)).filter(|t| !shared().contains(&t.list_id)).map(task_data),
        "share" => uuid().and_then(|u| store.shares.iter().find(|s| s.list_id == u)).map(|s| share_data(s, store)),
        "note" => uuid().and_then(|u| notes?.notes.iter().find(|n| n.id == u)).map(note_data),
        _ => None,
    };
    data.as_ref().map(fingerprint)
}

/// True for a device with nothing of its own yet (only the empty default list): joining
/// an account then takes the account's lists instead of adding an empty "Tasks" list.
pub fn is_fresh(store: &Store, notes: Option<&Notes>) -> bool {
    store.tasks.is_empty() && store.shares.is_empty() && store.lists.len() <= 1 && notes.is_none_or(|n| n.notes.is_empty())
}

/// Makes `store` ready to take an account's data: drops the empty default list.
pub fn clear_for_pull(store: &mut Store) {
    store.lists.clear();
    store.tasks.clear();
}

/// What this device changed since its last sync.
pub fn outgoing(store: &Store, notes: Option<&Notes>, account: &Account, now_ms: i64) -> Outgoing {
    let mut changes = Vec::new();
    let mut sent = HashMap::new();
    if !account.pull_only {
        let now = current(store, notes);
        for (k, (kind, id, data)) in &now {
            let fp = fingerprint(data);
            if account.known.get(k) != Some(&fp) {
                changes.push(Item { kind: kind.to_string(), id: id.clone(), modified: now_ms, deleted: false, data: data.clone(), rev: 0 });
                sent.insert(k.clone(), Some(fp));
            }
        }
        for k in account.known.keys().filter(|k| !now.contains_key(*k)) {
            let Some((kind, id)) = k.split_once(':') else { continue };
            if kind == "settings" {
                continue;
            }
            changes.push(Item { kind: kind.into(), id: id.into(), modified: now_ms, deleted: true, data: Value::Null, rev: 0 });
            sent.insert(k.clone(), None);
        }
        // Lists before their tasks, so a device applying them in order finds the list.
        changes.sort_by_key(|i| order(&i.kind));
    }
    Outgoing { request: SyncRequest { key: account.key.clone(), since: account.rev, changes }, sent }
}

fn order(kind: &str) -> u8 {
    match kind {
        "settings" => 0,
        "list" => 1,
        "share" => 2,
        "task" => 3,
        _ => 4,
    }
}

/// What applying the hub's answer changed on this device.
#[derive(Debug, Default, PartialEq)]
pub struct Applied {
    /// Lists, tasks or settings.
    pub tasks: usize,
    pub notes: usize,
    /// Shared lists this device joined because another of the person's devices is in them.
    pub joined: usize,
}

/// Applies the hub's answer. If you edited something while the request was on its way,
/// your edit is kept (it goes out next round, as the newest change).
pub fn apply(store: &mut Store, mut notes: Option<&mut Notes>, account: &mut Account, out: &Outgoing, resp: &SyncResponse) -> Applied {
    for (k, fp) in &out.sent {
        match fp {
            Some(fp) => account.known.insert(k.clone(), *fp),
            None => account.known.remove(k),
        };
    }
    let mut applied = Applied::default();
    // Settings name the default list, so they're applied last, once the lists are here.
    // Whether they were edited here is judged by how they were before this round.
    let settings_before = local_fingerprint(store, notes.as_deref(), "settings", SETTINGS_ID);
    let mut items: Vec<&Item> = resp.changes.iter().filter(|i| i.kind != "settings").collect();
    // Where incoming lists go in the menu.
    let mut list_pos: HashMap<Uuid, f64> = HashMap::new();
    items.sort_by_key(|i| order(&i.kind));
    for item in items {
        let k = item.key();
        let local = local_fingerprint(store, notes.as_deref(), &item.kind, &item.id);
        if item.deleted && local.is_none() {
            account.known.remove(&k);
            continue;
        }
        // Edited (or deleted) here since we sent: keep the local version.
        if local != account.known.get(&k).copied() {
            continue;
        }
        let Ok(id) = Uuid::parse_str(&item.id) else { continue };
        match (item.kind.as_str(), item.deleted) {
            ("list", true) => {
                if store.shares.iter().any(|s| s.list_id == id) {
                    continue;
                }
                store.lists.retain(|l| l.id != id);
                let gone: Vec<Uuid> = store.tasks.iter().filter(|t| t.list_id == id).map(|t| t.id).collect();
                store.tasks.retain(|t| t.list_id != id);
                for t in gone {
                    account.known.remove(&key("task", &t.to_string()));
                }
                account.known.remove(&k);
                applied.tasks += 1;
            }
            ("list", false) => {
                let Some(name) = item.data.get("name").and_then(Value::as_str) else { continue };
                if let Some(pos) = item.data.get("pos").and_then(Value::as_f64) {
                    list_pos.insert(id, pos);
                }
                match store.lists.iter_mut().find(|l| l.id == id) {
                    Some(l) if l.name == name => {}
                    Some(l) => {
                        l.name = name.to_string();
                        applied.tasks += 1;
                    }
                    None => {
                        store.lists.push(TaskList { id, name: name.to_string() });
                        applied.tasks += 1;
                    }
                }
                account.known.insert(k, fingerprint(&item.data));
            }
            ("share", true) => {
                // Another device stopped syncing it: keep the list here as a normal list.
                store.shares.retain(|s| s.list_id != id);
                account.known.remove(&k);
                applied.tasks += 1;
            }
            ("share", false) => {
                let (Some(hub), Some(code)) = (item.data.get("hub").and_then(Value::as_str), item.data.get("code").and_then(Value::as_str)) else {
                    continue;
                };
                if !store.shares.iter().any(|s| s.list_id == id) {
                    if !store.lists.iter().any(|l| l.id == id) {
                        let name = item.data.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).unwrap_or("Shared list");
                        store.lists.push(TaskList { id, name: name.to_string() });
                    }
                    // Its tasks now come through the share code: stop tracking them here.
                    for t in store.tasks.iter().filter(|t| t.list_id == id) {
                        account.known.remove(&key("task", &t.id.to_string()));
                    }
                    account.known.remove(&key("list", &item.id));
                    store.shares.push(Share { list_id: id, hub: hub.into(), code: code.into(), rev: 0, known: HashMap::new(), last_sync: None });
                    applied.joined += 1;
                }
                account.known.insert(k, local_fingerprint(store, notes.as_deref(), "share", &item.id).unwrap_or_default());
            }
            ("task", true) => {
                if store.tasks.iter().any(|t| t.id == id && shared_lists(store).contains(&t.list_id)) {
                    continue;
                }
                store.tasks.retain(|t| t.id != id);
                account.known.remove(&k);
                applied.tasks += 1;
            }
            ("task", false) => {
                let Some(task) = incoming_task(store, id, &item.data) else { continue };
                if shared_lists(store).contains(&task.list_id) {
                    continue;
                }
                let fp = fingerprint(&task_data(&task));
                match store.tasks.iter_mut().find(|t| t.id == id) {
                    Some(existing) if *existing == task => {}
                    Some(existing) => {
                        *existing = task;
                        applied.tasks += 1;
                    }
                    None => {
                        store.tasks.push(task);
                        applied.tasks += 1;
                    }
                }
                account.known.insert(k, fp);
            }
            ("note", deleted) => {
                let Some(notes) = notes.as_deref_mut() else { continue };
                if deleted {
                    notes.remove(id);
                    account.known.remove(&k);
                    applied.notes += 1;
                    continue;
                }
                let Some(note) = incoming_note(notes, id, &item.data) else { continue };
                let fp = fingerprint(&note_data(&note));
                match notes.notes.iter_mut().find(|n| n.id == id) {
                    Some(existing) if *existing == note => {}
                    Some(existing) => {
                        *existing = note;
                        applied.notes += 1;
                    }
                    None => {
                        notes.notes.push(note);
                        applied.notes += 1;
                    }
                }
                account.known.insert(k, fp);
            }
            _ => {}
        }
    }
    if !list_pos.is_empty() {
        let before: Vec<Uuid> = store.lists.iter().map(|l| l.id).collect();
        let mut keyed: Vec<(f64, usize, TaskList)> =
            store.lists.drain(..).enumerate().map(|(i, l)| (list_pos.get(&l.id).copied().unwrap_or(i as f64), i, l)).collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        store.lists = keyed.into_iter().map(|(_, _, l)| l).collect();
        if store.lists.iter().map(|l| l.id).collect::<Vec<_>>() != before {
            applied.tasks += 1;
        }
    }
    store.repair();
    if let Some(item) = resp.changes.iter().find(|i| i.kind == "settings" && !i.deleted) {
        let k = item.key();
        // A device that joined empty takes the account's settings, as it takes everything.
        let pulling = account.pull_only && !account.known.contains_key(&k);
        if pulling || account.known.get(&k).copied() == settings_before {
            if apply_settings(store, &item.data) {
                applied.tasks += 1;
            }
            // What the hub has, unless the default list it names isn't here: then ours
            // goes out next round.
            account.known.insert(k, fingerprint(&item.data));
        }
    }
    account.rev = resp.rev;
    account.pull_only = false;
    applied
}

/// Sets the synced settings from the hub's copy. Returns whether anything changed.
fn apply_settings(store: &mut Store, data: &Value) -> bool {
    let before = settings_data(store);
    if let Ok(mut all) = serde_json::to_value(&store.settings) {
        if let Some(obj) = all.as_object_mut() {
            for f in SYNCED_SETTINGS {
                if let Some(v) = data.get(f) {
                    obj.insert(f.into(), v.clone());
                }
            }
        }
        if let Ok(settings) = serde_json::from_value(all) {
            store.settings = settings;
        }
    }
    if let Some(id) = data.get("default_list").and_then(|v| serde_json::from_value::<Uuid>(v.clone()).ok()) {
        if store.lists.iter().any(|l| l.id == id) {
            store.default_list = id;
        }
    }
    settings_data(store) != before
}

/// The local task from the hub's copy, keeping this device's own fields.
fn incoming_task(store: &Store, id: Uuid, data: &Value) -> Option<Task> {
    let mut data = data.clone();
    let obj = data.as_object_mut()?;
    obj.insert("id".into(), serde_json::to_value(id).ok()?);
    if let Some(local) = store.tasks.iter().find(|t| t.id == id) {
        let local = serde_json::to_value(local).ok()?;
        for f in DEVICE_TASK_FIELDS {
            if let Some(v) = local.get(f) {
                obj.insert(f.into(), v.clone());
            }
        }
    }
    serde_json::from_value(data).ok()
}

/// The local note from the hub's copy, keeping where it sits on this device. A note new
/// to this device shows straight away (and finds itself a place).
fn incoming_note(notes: &Notes, id: Uuid, data: &Value) -> Option<StickyNote> {
    let mut data = data.clone();
    let obj = data.as_object_mut()?;
    obj.insert("id".into(), serde_json::to_value(id).ok()?);
    match notes.notes.iter().find(|n| n.id == id) {
        Some(local) => {
            let local = serde_json::to_value(local).ok()?;
            for f in DEVICE_NOTE_FIELDS {
                if let Some(v) = local.get(f) {
                    obj.insert(f.into(), v.clone());
                }
            }
        }
        None => {
            obj.insert("open".into(), Value::Bool(true));
        }
    }
    serde_json::from_value(data).ok()
}

/// "abcde fghjk…" → "ABCDE-FGHJK-MNPQR-STUVW"; `None` unless it has 20 letters and digits.
pub fn normalize_key(input: &str) -> Option<String> {
    let c: String = input.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    (c.len() == 20).then(|| c.as_bytes().chunks(5).map(|p| String::from_utf8_lossy(p).into_owned()).collect::<Vec<_>>().join("-"))
}

// ------------------------------------------------------------------- networking

/// A finished request, handed back to the UI.
pub enum Done {
    Created { hub: String, result: Result<String, String> },
    Synced(Outgoing, Result<SyncResponse, String>),
}

/// Whether this build can sync with a hub (all of them: the web app too, over HTTPS).
pub const SUPPORTED: bool = true;

/// Asks the hub for a new account.
pub fn create(hub: String, ctx: &eframe::egui::Context) -> std::sync::mpsc::Receiver<Done> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    post(&hub.clone(), "/api/account/new", Value::Object(Default::default()), move |result| {
        let result = result.and_then(|v| v.get("key").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "unexpected answer from the hub".to_string()));
        let _ = tx.send(Done::Created { hub, result });
        ctx.request_repaint();
    });
    rx
}

/// Sends one round.
pub fn sync(hub: &str, out: Outgoing, ctx: &eframe::egui::Context) -> std::sync::mpsc::Receiver<Done> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    let body = serde_json::to_value(&out.request).unwrap_or(Value::Null);
    post(hub, "/api/account/sync", body, move |result| {
        let result = result.and_then(|v| serde_json::from_value(v).map_err(|e| format!("unexpected answer from the hub: {e}")));
        let _ = tx.send(Done::Synced(out, result));
        ctx.request_repaint();
    });
    rx
}

fn post(hub: &str, path: &str, body: Value, done: impl FnOnce(Result<Value, String>) + Send + 'static) {
    let mut request = ehttp::Request::post(format!("{hub}{path}"), body.to_string().into_bytes());
    request.headers.insert("Content-Type", "application/json");
    let hub = hub.to_string();
    ehttp::fetch(request, move |result| {
        let out = match result {
            Ok(r) => {
                let value: Option<Value> = serde_json::from_slice(&r.bytes).ok();
                let message = value.as_ref().and_then(|v| v.get("error")).and_then(Value::as_str).map(str::to_string);
                match (r.ok, value) {
                    (true, Some(v)) => Ok(v),
                    (true, None) => Err("unexpected answer from the hub".to_string()),
                    // A hub from before 0.4.0 doesn't know these addresses.
                    (false, _) if r.status == 404 && message.as_deref().is_none_or(|m| m == "not found") => {
                        Err("this RustDo Hub is too old for Sync my devices. Update it to version 0.4.0 or later.".to_string())
                    }
                    (false, _) => Err(message.unwrap_or_else(|| format!("the hub refused the request ({})", r.status))),
                }
            }
            Err(_) if cfg!(target_arch = "wasm32") && hub.starts_with("http://") => Err(format!(
                "can't reach the hub at {hub}. In the web app the hub needs an https:// address (see How syncing works)."
            )),
            Err(_) => Err(format!("can't reach the hub at {hub}. Is it running, and can this device reach it?")),
        };
        done(out);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 25).unwrap().and_hms_opt(9, 0, 0).unwrap()
    }

    /// A miniature hub (same rules as hub/src/store.rs).
    #[derive(Default)]
    struct FakeHub {
        rev: u64,
        items: HashMap<String, Item>,
    }

    impl FakeHub {
        fn sync(&mut self, req: &SyncRequest) -> SyncResponse {
            let since = if req.since > self.rev { 0 } else { req.since };
            let mut turned_down = Vec::new();
            for c in &req.changes {
                if self.items.get(&c.key()).is_some_and(|o| o.modified > c.modified) {
                    turned_down.push(c.key());
                    continue;
                }
                self.rev += 1;
                let mut c = c.clone();
                c.rev = self.rev;
                if c.deleted {
                    c.data = Value::Null;
                }
                self.items.insert(c.key(), c);
            }
            let changes = self.items.values().filter(|i| i.rev > since || turned_down.contains(&i.key())).cloned().collect();
            SyncResponse { rev: self.rev, changes }
        }
    }

    struct Device {
        store: Store,
        notes: Notes,
        account: Account,
    }

    impl Device {
        fn new() -> Self {
            Self { store: Store::new(), notes: Notes::default(), account: Account::new("http://hub".into(), "KEY".into()) }
        }

        fn round(&mut self, hub: &mut FakeHub, ms: i64) -> Applied {
            let out = outgoing(&self.store, Some(&self.notes), &self.account, ms);
            let resp = hub.sync(&out.request);
            apply(&mut self.store, Some(&mut self.notes), &mut self.account, &out, &resp)
        }
    }

    fn add_task(store: &mut Store, title: &str) -> Uuid {
        let t = Task::new(title, store.default_list, now());
        let id = t.id;
        store.tasks.push(t);
        id
    }

    #[test]
    fn everything_reaches_a_fresh_device() {
        let mut hub = FakeHub::default();
        let mut a = Device::new();
        let work = a.store.add_list("Work").unwrap();
        add_task(&mut a.store, "Milk");
        a.store.settings.clock_24h = true;
        let n = a.notes.add(now(), Some([10.0, 20.0]));
        a.notes.get_mut(n).unwrap().text = "Wi-Fi password".into();
        a.round(&mut hub, 1);

        let mut b = Device::new();
        assert!(is_fresh(&b.store, Some(&b.notes)));
        clear_for_pull(&mut b.store);
        b.account.pull_only = true;
        let got = b.round(&mut hub, 2);
        assert!(got.tasks >= 3 && got.notes == 1);
        assert_eq!(b.store.lists.len(), 2, "no extra empty Tasks list");
        let names = |s: &Store| s.lists.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&b.store), names(&a.store), "lists in the same order");
        assert_eq!(b.store.default_list, a.store.default_list);
        assert!(b.store.lists.iter().any(|l| l.id == work));
        assert_eq!(b.store.tasks[0].title, "Milk");
        assert!(b.store.settings.clock_24h);
        let note = &b.notes.notes[0];
        assert_eq!(note.text, "Wi-Fi password");
        assert!(note.open, "a new note shows on this device");
        assert_eq!(note.pos, None, "where it sits on A's screen stays on A");
        // Nothing left to send either way.
        assert!(outgoing(&b.store, Some(&b.notes), &b.account, 3).request.changes.is_empty());
        a.round(&mut hub, 4);
        assert!(outgoing(&a.store, Some(&a.notes), &a.account, 5).request.changes.is_empty());
    }

    #[test]
    fn edits_and_deletes_go_both_ways() {
        let mut hub = FakeHub::default();
        let mut a = Device::new();
        let milk = add_task(&mut a.store, "Milk");
        let eggs = add_task(&mut a.store, "Eggs");
        a.round(&mut hub, 1);
        let mut b = Device::new();
        clear_for_pull(&mut b.store);
        b.account.pull_only = true;
        b.round(&mut hub, 2);

        a.store.task_mut(milk).unwrap().title = "Oat milk".into();
        b.store.tasks.retain(|t| t.id != eggs);
        a.round(&mut hub, 10);
        b.round(&mut hub, 11);
        a.round(&mut hub, 12);
        for d in [&a, &b] {
            assert_eq!(d.store.tasks.len(), 1);
            assert_eq!(d.store.tasks[0].title, "Oat milk");
        }
        // Deleting a list removes it and its tasks on the other device.
        let list = a.store.add_list("Trip").unwrap();
        a.store.tasks.push(Task::new("Passport", list, now()));
        a.round(&mut hub, 20);
        b.round(&mut hub, 21);
        assert!(b.store.tasks.iter().any(|t| t.title == "Passport"));
        a.store.lists.retain(|l| l.id != list);
        a.store.tasks.retain(|t| t.list_id != list);
        a.round(&mut hub, 30);
        b.round(&mut hub, 31);
        assert!(!b.store.lists.iter().any(|l| l.id == list));
        assert!(!b.store.tasks.iter().any(|t| t.title == "Passport"));
    }

    #[test]
    fn a_local_edit_in_flight_is_kept() {
        let mut hub = FakeHub::default();
        let mut a = Device::new();
        let id = add_task(&mut a.store, "Milk");
        a.round(&mut hub, 1);
        let mut b = Device::new();
        clear_for_pull(&mut b.store);
        b.account.pull_only = true;
        b.round(&mut hub, 2);
        // B renames; A renames too but B's request is already out when A's lands.
        b.store.task_mut(id).unwrap().title = "B's milk".into();
        let out = outgoing(&b.store, Some(&b.notes), &b.account, 5);
        a.store.task_mut(id).unwrap().title = "A's milk".into();
        a.round(&mut hub, 6);
        // While B's request was on its way, B edited again: that edit stays.
        b.store.task_mut(id).unwrap().title = "B's oat milk".into();
        let resp = hub.sync(&out.request);
        apply(&mut b.store, Some(&mut b.notes), &mut b.account, &out, &resp);
        assert_eq!(b.store.tasks[0].title, "B's oat milk");
    }

    #[test]
    fn shared_lists_stay_out_of_the_account_but_other_devices_join_them() {
        let mut hub = FakeHub::default();
        let mut a = Device::new();
        let groceries = a.store.add_list("Groceries").unwrap();
        a.store.tasks.push(Task::new("Bananas", groceries, now()));
        a.store.shares.push(Share { list_id: groceries, hub: "http://hub".into(), code: "K7QM-2XPD".into(), rev: 3, known: HashMap::new(), last_sync: None });
        let out = outgoing(&a.store, Some(&a.notes), &a.account, 1);
        assert!(out.request.changes.iter().all(|i| i.kind != "task"), "a shared list's tasks never go into the account");
        assert!(out.request.changes.iter().any(|i| i.kind == "share" && i.data["code"] == "K7QM-2XPD"));
        a.round(&mut hub, 1);

        let mut b = Device::new();
        clear_for_pull(&mut b.store);
        b.account.pull_only = true;
        let got = b.round(&mut hub, 2);
        assert_eq!(got.joined, 1);
        let share = b.store.shares.iter().find(|s| s.list_id == groceries).unwrap();
        assert_eq!((share.code.as_str(), share.rev), ("K7QM-2XPD", 0), "B fetches the list's tasks through the share");
        assert_eq!(b.store.list_name(groceries), "Groceries");
    }

    #[test]
    fn note_positions_and_reminders_stay_per_device() {
        let mut hub = FakeHub::default();
        let mut a = Device::new();
        let n = a.notes.add(now(), Some([100.0, 100.0]));
        let t = add_task(&mut a.store, "Call Mom");
        a.round(&mut hub, 1);
        let mut b = Device::new();
        clear_for_pull(&mut b.store);
        b.account.pull_only = true;
        b.round(&mut hub, 2);
        // Moving a note or a reminder firing on B isn't a change to sync.
        b.notes.get_mut(n).unwrap().pos = Some([5.0, 5.0]);
        b.notes.get_mut(n).unwrap().open = false;
        b.store.task_mut(t).unwrap().reminder_fired = true;
        assert!(outgoing(&b.store, Some(&b.notes), &b.account, 3).request.changes.is_empty());
        // A's edit to the text arrives without moving B's note.
        a.notes.get_mut(n).unwrap().text = "Hello".into();
        a.round(&mut hub, 4);
        b.round(&mut hub, 5);
        let note = b.notes.notes.iter().find(|x| x.id == n).unwrap();
        assert_eq!((note.text.as_str(), note.pos, note.open), ("Hello", Some([5.0, 5.0]), false));
    }

    #[test]
    fn combining_two_devices_keeps_both() {
        let mut hub = FakeHub::default();
        let mut a = Device::new();
        add_task(&mut a.store, "From A");
        a.round(&mut hub, 1);
        let mut b = Device::new();
        add_task(&mut b.store, "From B");
        assert!(!is_fresh(&b.store, Some(&b.notes)));
        b.round(&mut hub, 2);
        a.round(&mut hub, 3);
        for d in [&a, &b] {
            let mut titles: Vec<&str> = d.store.tasks.iter().map(|t| t.title.as_str()).collect();
            titles.sort();
            assert_eq!(titles, ["From A", "From B"]);
        }
    }

    #[test]
    fn keys_are_normalized() {
        assert_eq!(normalize_key(" abcde fghjk-mnpqr stuvw ").as_deref(), Some("ABCDE-FGHJK-MNPQR-STUVW"));
        assert_eq!(normalize_key("ABCD"), None);
    }
}
