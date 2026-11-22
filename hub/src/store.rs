//! The hub's data and sync rules (no networking here, so it's easy to test).
//!
//! Each shared list holds items: one `list` item (its name) and one `task` item per task.
//! The hub doesn't look inside an item's `data`; RustDo apps own that format. Every
//! accepted change gets the list's next revision number, so an app asks only for what
//! changed since the revision it last saw. Deletions stay as small "tombstones" so
//! devices that were offline learn about them too.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Bumped only for changes old apps can't understand.
pub const PROTOCOL: u32 = 1;
const FILE: &str = "hub.json";
/// Unambiguous characters for share codes (no 0/O, 1/I/L).
const CODE_CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// "list" or "task".
    pub kind: String,
    pub id: String,
    /// When the change was made, in milliseconds since 1970 (UTC). Newest wins.
    pub modified: i64,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub data: Value,
    /// Set by the hub.
    #[serde(default)]
    pub rev: u64,
}

impl Item {
    fn key(&self) -> String {
        format!("{}:{}", self.kind, self.id)
    }
}

#[derive(Debug, Deserialize)]
pub struct ShareRequest {
    pub list_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub code: String,
    /// The last revision this device has (0 = send everything).
    #[serde(default)]
    pub since: u64,
    #[serde(default)]
    pub changes: Vec<Item>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SyncResponse {
    pub list_id: String,
    pub rev: u64,
    /// Everything changed after `since`, plus the hub's version of any change it
    /// turned down because it already had a newer one.
    pub changes: Vec<Item>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SharedList {
    list_id: String,
    #[serde(default)]
    rev: u64,
    #[serde(default)]
    items: BTreeMap<String, Item>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Db {
    #[serde(default)]
    lists: BTreeMap<String, SharedList>,
}

pub struct Hub {
    path: PathBuf,
    db: Db,
}

impl Hub {
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(FILE);
        let db = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{} is damaged: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Db::default(),
            Err(e) => return Err(e.to_string()),
        };
        let hub = Self { path, db };
        hub.save()?; // fail now, not on the first share, if the folder isn't writable
        Ok(hub)
    }

    /// In-memory hub for tests.
    #[cfg(test)]
    fn memory() -> Self {
        Self { path: PathBuf::new(), db: Db::default() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn list_count(&self) -> usize {
        self.db.lists.len()
    }

    fn save(&self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let text = serde_json::to_vec(&self.db).map_err(|e| e.to_string())?;
        // Write a new file, then swap it in: a crash mid-write never leaves half a file.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Starts sharing a list; returns its new code, formatted like `K7QM-2XPD`.
    pub fn share(&mut self, req: ShareRequest) -> Result<String, String> {
        let list_id = req.list_id.trim().to_string();
        if list_id.is_empty() || list_id.len() > 64 {
            return Err("bad list id".into());
        }
        let code = loop {
            let code = new_code()?;
            if !self.db.lists.contains_key(&code) {
                break code;
            }
        };
        self.db.lists.insert(code.clone(), SharedList { list_id, ..Default::default() });
        self.save()?;
        Ok(format!("{}-{}", &code[..4], &code[4..]))
    }

    /// Applies a device's changes and returns what it's missing. `None`: unknown code.
    pub fn sync(&mut self, req: SyncRequest) -> Result<Option<SyncResponse>, String> {
        let Some(list) = self.db.lists.get_mut(&normalize_code(&req.code)) else {
            return Ok(None);
        };
        // A device that's ahead of the hub (e.g. the hub's data was restored from an
        // older copy) gets everything again.
        let since = if req.since > list.rev { 0 } else { req.since };
        let mut turned_down = HashSet::new();
        let mut changed = false;
        for mut item in req.changes {
            if !matches!(item.kind.as_str(), "list" | "task") || item.id.is_empty() || item.id.len() > 64 {
                continue;
            }
            // The list item always describes this list, whatever id the device sent.
            if item.kind == "list" {
                item.id = list.list_id.clone();
            }
            let key = item.key();
            if list.items.get(&key).is_some_and(|old| old.modified > item.modified) {
                turned_down.insert(key);
                continue;
            }
            list.rev += 1;
            item.rev = list.rev;
            if item.deleted {
                item.data = Value::Null;
            }
            list.items.insert(key, item);
            changed = true;
        }
        let changes = list
            .items
            .iter()
            .filter(|(key, item)| item.rev > since || turned_down.contains(*key))
            .map(|(_, item)| item.clone())
            .collect();
        let response = SyncResponse { list_id: list.list_id.clone(), rev: list.rev, changes };
        if changed {
            self.save()?;
        }
        Ok(Some(response))
    }
}

/// "k7qm 2xpd", "K7QM-2XPD" → "K7QM2XPD".
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

fn new_code() -> Result<String, String> {
    let mut bytes = [0u8; 8];
    getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
    // 31 characters: the tiny bias from `%` doesn't matter for a code this long.
    Ok(bytes.iter().map(|b| CODE_CHARS[*b as usize % CODE_CHARS.len()] as char).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(id: &str, modified: i64, title: &str) -> Item {
        Item { kind: "task".into(), id: id.into(), modified, deleted: false, data: json!({ "title": title }), rev: 0 }
    }

    fn sync(hub: &mut Hub, code: &str, since: u64, changes: Vec<Item>) -> SyncResponse {
        hub.sync(SyncRequest { code: code.into(), since, changes }).unwrap().unwrap()
    }

    #[test]
    fn share_and_join() {
        let mut hub = Hub::memory();
        let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
        assert_eq!(code.len(), 9);
        let r = sync(&mut hub, &code, 0, vec![task("a", 10, "Milk")]);
        assert_eq!(r.rev, 1);
        // Someone else joins with the code typed differently.
        let joined = sync(&mut hub, &code.to_lowercase().replace('-', " "), 0, vec![]);
        assert_eq!(joined.list_id, "L1");
        assert_eq!(joined.changes.len(), 1);
        assert_eq!(joined.changes[0].data["title"], "Milk");
        assert!(hub.sync(SyncRequest { code: "NOPE-NOPE".into(), since: 0, changes: vec![] }).unwrap().is_none());
    }

    #[test]
    fn only_new_changes_are_sent() {
        let mut hub = Hub::memory();
        let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
        let r1 = sync(&mut hub, &code, 0, vec![task("a", 10, "Milk"), task("b", 10, "Eggs")]);
        let r2 = sync(&mut hub, &code, r1.rev, vec![task("c", 20, "Bread")]);
        assert_eq!(r2.changes.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["c"]);
        let r3 = sync(&mut hub, &code, r2.rev, vec![]);
        assert!(r3.changes.is_empty());
    }

    #[test]
    fn newest_edit_wins() {
        let mut hub = Hub::memory();
        let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
        let r = sync(&mut hub, &code, 0, vec![task("a", 100, "Milk (2%)")]);
        // An older edit from a device that was offline is turned down, and that device
        // gets the newer version back even though it isn't "new" since its revision.
        let stale = sync(&mut hub, &code, r.rev, vec![task("a", 50, "Milk")]);
        assert_eq!(stale.rev, r.rev);
        assert_eq!(stale.changes.len(), 1);
        assert_eq!(stale.changes[0].data["title"], "Milk (2%)");
        // A newer edit replaces it.
        let newer = sync(&mut hub, &code, r.rev, vec![task("a", 200, "Oat milk")]);
        assert_eq!(newer.changes[0].data["title"], "Oat milk");
    }

    #[test]
    fn deletions_reach_other_devices() {
        let mut hub = Hub::memory();
        let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
        let r = sync(&mut hub, &code, 0, vec![task("a", 10, "Milk")]);
        let mut gone = task("a", 20, "");
        gone.deleted = true;
        sync(&mut hub, &code, r.rev, vec![gone]);
        let other = sync(&mut hub, &code, r.rev, vec![]);
        assert!(other.changes[0].deleted);
        assert_eq!(other.changes[0].data, Value::Null);
    }

    #[test]
    fn list_item_is_pinned_to_the_list() {
        let mut hub = Hub::memory();
        let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
        let rename = Item { kind: "list".into(), id: "something-else".into(), modified: 5, deleted: false, data: json!({ "name": "Groceries" }), rev: 0 };
        let r = sync(&mut hub, &code, 0, vec![rename]);
        assert_eq!(r.changes[0].id, "L1");
    }

    #[test]
    fn device_ahead_of_hub_gets_everything() {
        let mut hub = Hub::memory();
        let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
        sync(&mut hub, &code, 0, vec![task("a", 10, "Milk")]);
        let r = sync(&mut hub, &code, 999, vec![]);
        assert_eq!(r.changes.len(), 1);
    }

    #[test]
    fn saves_and_reloads() {
        let dir = std::env::temp_dir().join(format!("rustdo-hub-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let code = {
            let mut hub = Hub::open(&dir).unwrap();
            let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
            sync(&mut hub, &code, 0, vec![task("a", 10, "Milk")]);
            code
        };
        let mut hub = Hub::open(&dir).unwrap();
        assert_eq!(hub.list_count(), 1);
        assert_eq!(sync(&mut hub, &code, 0, vec![]).changes.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
