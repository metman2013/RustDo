//! Family sharing through a RustDo Hub (a small server you run at home; see docs/hub.md).
//!
//! How a shared list stays in sync without tracking every edit: for each shared list we
//! remember a fingerprint of every task as of the last sync. Anything whose fingerprint
//! changed is sent to the hub; anything that disappeared is sent as deleted. The hub
//! keeps the newest version of each task and hands back what others changed.
//!
//! "My Day" and whether a reminder already fired stay personal to each device.

use crate::model::{Store, Task, TaskList};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Task fields that aren't shared: each person plans their own day.
const PERSONAL_FIELDS: [&str; 2] = ["my_day", "reminder_fired"];

/// A list on this device that syncs with a hub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Share {
    pub list_id: Uuid,
    /// e.g. `http://192.168.1.20:7979`
    pub hub: String,
    /// e.g. `K7QM-2XPD`
    pub code: String,
    /// The hub revision this device has caught up to.
    #[serde(default)]
    pub rev: u64,
    /// Fingerprints of the list name and each task as last synced.
    #[serde(default)]
    pub known: HashMap<Uuid, u64>,
    /// Local time of the last successful sync.
    #[serde(default)]
    pub last_sync: Option<chrono::NaiveDateTime>,
}

/// What goes over the wire (the hub's `Item`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub kind: String,
    pub id: Uuid,
    pub modified: i64,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub rev: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncRequest {
    pub code: String,
    pub since: u64,
    pub changes: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncResponse {
    pub list_id: Uuid,
    pub rev: u64,
    #[serde(default)]
    pub changes: Vec<Item>,
}

/// One round in flight: what was sent, so the answer can be applied safely.
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub list_id: Uuid,
    pub request: SyncRequest,
    /// The fingerprint of each item as sent (`None` = sent as deleted).
    pub sent: HashMap<Uuid, Option<u64>>,
}

/// FNV-1a: a fingerprint that stays the same across Rust versions and platforms.
fn fingerprint(value: &Value) -> u64 {
    let text = value.to_string();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn task_data(task: &Task) -> Value {
    let mut v = serde_json::to_value(task).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        for f in PERSONAL_FIELDS {
            obj.remove(f);
        }
        obj.remove("list_id");
    }
    v
}

fn list_data(list: &TaskList) -> Value {
    serde_json::json!({ "name": list.name })
}

/// The shared content of a list right now: id → (kind, data).
fn current(store: &Store, list_id: Uuid) -> HashMap<Uuid, (&'static str, Value)> {
    let mut out = HashMap::new();
    if let Some(list) = store.lists.iter().find(|l| l.id == list_id) {
        out.insert(list_id, ("list", list_data(list)));
    }
    for t in store.tasks.iter().filter(|t| t.list_id == list_id) {
        out.insert(t.id, ("task", task_data(t)));
    }
    out
}

/// What this device changed in a shared list since its last sync.
pub fn outgoing(store: &Store, share: &Share, now_ms: i64) -> Outgoing {
    let now = current(store, share.list_id);
    let mut changes = Vec::new();
    let mut sent = HashMap::new();
    for (id, (kind, data)) in &now {
        let fp = fingerprint(data);
        if share.known.get(id) != Some(&fp) {
            changes.push(Item { kind: kind.to_string(), id: *id, modified: now_ms, deleted: false, data: data.clone(), rev: 0 });
            sent.insert(*id, Some(fp));
        }
    }
    for id in share.known.keys().filter(|id| !now.contains_key(id)) {
        // The list itself can't be deleted through sync: deleting it here only stops syncing.
        if *id == share.list_id {
            continue;
        }
        changes.push(Item { kind: "task".into(), id: *id, modified: now_ms, deleted: true, data: Value::Null, rev: 0 });
        sent.insert(*id, None);
    }
    Outgoing {
        list_id: share.list_id,
        request: SyncRequest { code: share.code.clone(), since: share.rev, changes },
        sent,
    }
}

/// Applies the hub's answer. Returns how many items changed on this device.
///
/// If you edited something while the request was on its way, your edit is kept (and goes
/// out next round, as the newest change) instead of being replaced by the hub's copy.
pub fn apply(store: &mut Store, share: &mut Share, out: &Outgoing, resp: &SyncResponse) -> usize {
    // What this device believes the hub has, once our changes landed.
    for (id, fp) in &out.sent {
        match fp {
            Some(fp) => share.known.insert(*id, *fp),
            None => share.known.remove(id),
        };
    }
    let list_id = share.list_id;
    let mut applied = 0;
    for item in &resp.changes {
        let local = current_fingerprint(store, list_id, item.id);
        // Edited (or deleted) here since we sent: keep the local version.
        if local != share.known.get(&item.id).copied() {
            continue;
        }
        match item.kind.as_str() {
            "list" => {
                let Some(name) = item.data.get("name").and_then(Value::as_str) else { continue };
                if let Some(list) = store.lists.iter_mut().find(|l| l.id == list_id) {
                    if list.name != name {
                        list.name = name.to_string();
                        applied += 1;
                    }
                    share.known.insert(list_id, fingerprint(&list_data(list)));
                }
            }
            "task" if item.deleted => {
                let before = store.tasks.len();
                store.tasks.retain(|t| !(t.id == item.id && t.list_id == list_id));
                applied += before - store.tasks.len();
                share.known.remove(&item.id);
            }
            "task" => {
                let Some(task) = incoming_task(store, list_id, item) else { continue };
                let fp = fingerprint(&task_data(&task));
                match store.tasks.iter_mut().find(|t| t.id == task.id) {
                    Some(existing) if *existing == task => {}
                    Some(existing) => {
                        *existing = task;
                        applied += 1;
                    }
                    None => {
                        store.tasks.push(task);
                        applied += 1;
                    }
                }
                share.known.insert(item.id, fp);
            }
            _ => {}
        }
    }
    share.rev = resp.rev;
    applied
}

/// Adds a list someone shared, from the hub's first answer. `None` if it's already here.
pub fn join(store: &mut Store, hub: &str, code: &str, resp: &SyncResponse) -> Option<Share> {
    if store.lists.iter().any(|l| l.id == resp.list_id) {
        return None;
    }
    let name = resp
        .changes
        .iter()
        .find(|i| i.kind == "list")
        .and_then(|i| i.data.get("name").and_then(Value::as_str))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or("Shared list")
        .to_string();
    let list = TaskList { id: resp.list_id, name };
    let mut share = Share {
        list_id: list.id,
        hub: hub.to_string(),
        code: code.to_string(),
        rev: 0,
        known: HashMap::from([(list.id, fingerprint(&list_data(&list)))]),
        last_sync: None,
    };
    store.lists.push(list);
    let nothing_sent = Outgoing {
        list_id: share.list_id,
        request: SyncRequest { code: code.to_string(), since: 0, changes: Vec::new() },
        sent: HashMap::new(),
    };
    apply(store, &mut share, &nothing_sent, resp);
    Some(share)
}

fn current_fingerprint(store: &Store, list_id: Uuid, id: Uuid) -> Option<u64> {
    if id == list_id {
        return store.lists.iter().find(|l| l.id == id).map(|l| fingerprint(&list_data(l)));
    }
    store
        .tasks
        .iter()
        .find(|t| t.id == id && t.list_id == list_id)
        .map(|t| fingerprint(&task_data(t)))
}

/// Builds the local task from the hub's copy, keeping this device's personal fields.
fn incoming_task(store: &Store, list_id: Uuid, item: &Item) -> Option<Task> {
    let mut data = item.data.clone();
    let obj = data.as_object_mut()?;
    obj.insert("id".into(), serde_json::to_value(item.id).ok()?);
    obj.insert("list_id".into(), serde_json::to_value(list_id).ok()?);
    if let Some(local) = store.tasks.iter().find(|t| t.id == item.id) {
        let local = serde_json::to_value(local).ok()?;
        for f in PERSONAL_FIELDS {
            if let Some(v) = local.get(f) {
                obj.insert(f.into(), v.clone());
            }
        }
    }
    serde_json::from_value(data).ok()
}

/// Accepts what people type: `192.168.1.20`, `192.168.1.20:7979`, `http://hub.local:7979/`.
pub fn normalize_hub(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() || s.contains(char::is_whitespace) {
        return None;
    }
    let with_scheme = if s.starts_with("http://") || s.starts_with("https://") { s.to_string() } else { format!("http://{s}") };
    let host_part = with_scheme.split("://").nth(1)?;
    if host_part.is_empty() {
        return None;
    }
    // No port given: the hub's default.
    let has_port = host_part.rsplit_once(':').is_some_and(|(_, p)| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty());
    Some(if has_port || with_scheme.starts_with("https://") { with_scheme } else { format!("{with_scheme}:7979") })
}

/// "k7qm2xpd" → "K7QM-2XPD".
pub fn normalize_code(input: &str) -> Option<String> {
    let c: String = input.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    (c.len() == 8).then(|| format!("{}-{}", &c[..4], &c[4..]))
}

// ------------------------------------------------------------------- networking

/// A finished request, handed back to the UI thread.
pub enum Done {
    Synced(Outgoing, Result<SyncResponse, String>),
    Shared { list_id: Uuid, hub: String, result: Result<String, String> },
    Joined { hub: String, code: String, result: Result<SyncResponse, String> },
}

pub enum Job {
    Sync { hub: String, out: Outgoing },
    Share { hub: String, list_id: Uuid },
    Join { hub: String, code: String },
}

/// Whether this build can talk to a hub. The web app can't: a page served over HTTPS
/// isn't allowed to call a plain-HTTP address on your network.
pub const SUPPORTED: bool = cfg!(all(not(target_arch = "wasm32"), feature = "updates"));

#[cfg(all(not(target_arch = "wasm32"), feature = "updates"))]
pub fn run(jobs: Vec<Job>, ctx: eframe::egui::Context) -> std::sync::mpsc::Receiver<Vec<Done>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(20))
            .user_agent(concat!("RustDo/", env!("CARGO_PKG_VERSION")))
            .build();
        let results = jobs.into_iter().map(|job| run_one(&agent, job)).collect();
        let _ = tx.send(results);
        ctx.request_repaint();
    });
    rx
}

#[cfg(any(target_arch = "wasm32", not(feature = "updates")))]
pub fn run(_jobs: Vec<Job>, _ctx: eframe::egui::Context) -> std::sync::mpsc::Receiver<Vec<Done>> {
    std::sync::mpsc::channel().1
}

#[cfg(all(not(target_arch = "wasm32"), feature = "updates"))]
fn post(agent: &ureq::Agent, hub: &str, path: &str, body: Value) -> Result<Value, String> {
    let read = |r: ureq::Response| -> Result<Value, String> {
        let text = r.into_string().map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("unexpected answer from the hub: {e}"))
    };
    let request = agent.post(&format!("{hub}{path}")).set("Content-Type", "application/json");
    match request.send_string(&body.to_string()) {
        Ok(r) => read(r),
        Err(ureq::Error::Status(_, r)) => {
            let msg = read(r).ok().and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string));
            Err(msg.unwrap_or_else(|| "the hub refused the request".into()))
        }
        Err(ureq::Error::Transport(_)) => Err(format!("can't reach the hub at {hub}. Is it running, and are you on the same network?")),
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "updates"))]
fn run_one(agent: &ureq::Agent, job: Job) -> Done {
    let sync = |hub: &str, req: &SyncRequest| -> Result<SyncResponse, String> {
        let v = post(agent, hub, "/api/sync", serde_json::to_value(req).map_err(|e| e.to_string())?)?;
        serde_json::from_value(v).map_err(|e| format!("unexpected answer from the hub: {e}"))
    };
    match job {
        Job::Sync { hub, out } => {
            let result = sync(&hub, &out.request);
            Done::Synced(out, result)
        }
        Job::Share { hub, list_id } => {
            let result = post(agent, &hub, "/api/share", serde_json::json!({ "list_id": list_id })).and_then(|v| {
                v.get("code").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "unexpected answer from the hub".into())
            });
            Done::Shared { list_id, hub, result }
        }
        Job::Join { hub, code } => {
            let result = sync(&hub, &SyncRequest { code: code.clone(), since: 0, changes: Vec::new() });
            Done::Joined { hub, code, result }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn now() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 22).unwrap().and_hms_opt(9, 0, 0).unwrap()
    }

    /// A miniature hub (same rules as hub/src/store.rs) to sync two stores through.
    #[derive(Default)]
    struct FakeHub {
        list_id: Option<Uuid>,
        rev: u64,
        items: HashMap<Uuid, Item>,
    }

    impl FakeHub {
        fn sync(&mut self, req: &SyncRequest) -> SyncResponse {
            let mut turned_down = Vec::new();
            for c in &req.changes {
                if self.items.get(&c.id).is_some_and(|o| o.modified > c.modified) {
                    turned_down.push(c.id);
                    continue;
                }
                self.rev += 1;
                let mut c = c.clone();
                c.rev = self.rev;
                self.items.insert(c.id, c);
            }
            let changes = self.items.values().filter(|i| i.rev > req.since || turned_down.contains(&i.id)).cloned().collect();
            SyncResponse { list_id: self.list_id.unwrap(), rev: self.rev, changes }
        }
    }

    fn round(store: &mut Store, share: &mut Share, hub: &mut FakeHub, ms: i64) -> usize {
        let out = outgoing(store, share, ms);
        let resp = hub.sync(&out.request);
        apply(store, share, &out, &resp)
    }

    fn share_for(list_id: Uuid) -> Share {
        Share { list_id, hub: "http://hub:7979".into(), code: "AAAA-BBBB".into(), rev: 0, known: HashMap::new(), last_sync: None }
    }

    /// Alice shares "Groceries"; Bob joins (an empty list with the same id).
    fn setup() -> (Store, Share, Store, Share, FakeHub, Uuid) {
        let mut alice = Store::new();
        let list = alice.add_list("Groceries").unwrap();
        alice.add_task("Milk", crate::model::View::List(list), now());
        let mut a_share = share_for(list);
        let mut hub = FakeHub { list_id: Some(list), ..Default::default() };
        round(&mut alice, &mut a_share, &mut hub, 1000);

        let mut bob = Store::new();
        let first = hub.sync(&SyncRequest { code: "AAAA-BBBB".into(), since: 0, changes: Vec::new() });
        let b_share = join(&mut bob, "http://hub:7979", "AAAA-BBBB", &first).unwrap();
        assert!(join(&mut bob, "http://hub:7979", "AAAA-BBBB", &first).is_none());
        (alice, a_share, bob, b_share, hub, list)
    }

    fn titles(store: &Store, list: Uuid) -> Vec<String> {
        let mut t: Vec<String> = store.tasks.iter().filter(|t| t.list_id == list).map(|t| t.title.clone()).collect();
        t.sort();
        t
    }

    #[test]
    fn join_gets_name_and_tasks() {
        let (_, _, bob, _, _, list) = setup();
        assert_eq!(bob.list_name(list), "Groceries");
        assert_eq!(titles(&bob, list), ["Milk"]);
    }

    #[test]
    fn edits_adds_and_deletes_flow_both_ways() {
        let (mut alice, mut a, mut bob, mut b, mut hub, list) = setup();
        bob.add_task("Eggs", crate::model::View::List(list), now());
        let milk = bob.tasks.iter().find(|t| t.title == "Milk").unwrap().id;
        bob.task_mut(milk).unwrap().done = true;
        round(&mut bob, &mut b, &mut hub, 2000);
        round(&mut alice, &mut a, &mut hub, 2001);
        assert_eq!(titles(&alice, list), ["Eggs", "Milk"]);
        assert!(alice.task(milk).unwrap().done);

        alice.remove_task(milk);
        round(&mut alice, &mut a, &mut hub, 3000);
        round(&mut bob, &mut b, &mut hub, 3001);
        assert_eq!(titles(&bob, list), ["Eggs"]);
        // Nothing left to send once everyone is caught up.
        assert!(outgoing(&bob, &b, 4000).request.changes.is_empty());
        assert!(outgoing(&alice, &a, 4000).request.changes.is_empty());
    }

    #[test]
    fn my_day_stays_personal() {
        let (mut alice, mut a, mut bob, mut b, mut hub, list) = setup();
        let milk = alice.tasks.iter().find(|t| t.list_id == list).unwrap().id;
        alice.task_mut(milk).unwrap().my_day = Some(now().date());
        // Adding to My Day isn't a shared change at all.
        assert!(outgoing(&alice, &a, 2000).request.changes.is_empty());
        alice.task_mut(milk).unwrap().important = true;
        round(&mut alice, &mut a, &mut hub, 2000);
        round(&mut bob, &mut b, &mut hub, 2001);
        let bobs = bob.task(milk).unwrap();
        assert!(bobs.important);
        assert_eq!(bobs.my_day, None);
        assert_eq!(alice.task(milk).unwrap().my_day, Some(now().date()));
    }

    #[test]
    fn rename_syncs() {
        let (mut alice, mut a, mut bob, mut b, mut hub, list) = setup();
        alice.rename_list(list, "Shopping");
        round(&mut alice, &mut a, &mut hub, 2000);
        round(&mut bob, &mut b, &mut hub, 2001);
        assert_eq!(bob.list_name(list), "Shopping");
    }

    #[test]
    fn edit_made_while_syncing_is_kept() {
        let (mut alice, mut a, mut bob, mut b, mut hub, list) = setup();
        let milk = alice.tasks.iter().find(|t| t.list_id == list).unwrap().id;
        alice.task_mut(milk).unwrap().title = "Milk (2%)".into();
        round(&mut alice, &mut a, &mut hub, 2000);
        // Bob's request goes out, then he edits Milk before the answer arrives.
        let out = outgoing(&bob, &b, 2001);
        let resp = hub.sync(&out.request);
        bob.task_mut(milk).unwrap().title = "Oat milk".into();
        apply(&mut bob, &mut b, &out, &resp);
        assert_eq!(bob.task(milk).unwrap().title, "Oat milk");
        // ...and it reaches Alice next round, as the newest edit.
        round(&mut bob, &mut b, &mut hub, 3000);
        round(&mut alice, &mut a, &mut hub, 3001);
        assert_eq!(alice.task(milk).unwrap().title, "Oat milk");
    }

    #[test]
    fn tasks_moved_out_are_removed_for_others() {
        let (mut alice, mut a, mut bob, mut b, mut hub, list) = setup();
        let milk = alice.tasks.iter().find(|t| t.list_id == list).unwrap().id;
        let private = alice.default_list;
        alice.task_mut(milk).unwrap().list_id = private;
        round(&mut alice, &mut a, &mut hub, 2000);
        round(&mut bob, &mut b, &mut hub, 2001);
        assert!(titles(&bob, list).is_empty());
        // Alice still has it, in her own list.
        assert_eq!(alice.task(milk).unwrap().list_id, private);
    }

    #[test]
    fn hub_address_and_code_input() {
        assert_eq!(normalize_hub("192.168.1.20").as_deref(), Some("http://192.168.1.20:7979"));
        assert_eq!(normalize_hub(" 192.168.1.20:8080/ ").as_deref(), Some("http://192.168.1.20:8080"));
        assert_eq!(normalize_hub("http://nas.local:7979").as_deref(), Some("http://nas.local:7979"));
        assert_eq!(normalize_hub("https://hub.example.ts.net").as_deref(), Some("https://hub.example.ts.net"));
        assert_eq!(normalize_hub(""), None);
        assert_eq!(normalize_code("k7qm 2xpd").as_deref(), Some("K7QM-2XPD"));
        assert_eq!(normalize_code("K7QM-2XP"), None);
    }
}
