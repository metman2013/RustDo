//! The hub's data and sync rules (no networking here, so it's easy to test).
//!
//! Two kinds of collections, both made of items the hub doesn't look inside (RustDo apps
//! own the format of an item's `data`):
//! * a **shared list** (family sharing), found by its share code: one `list` item (its
//!   name) and one `task` item per task;
//! * an **account** (one person's devices), found by its sync key: all their lists, tasks,
//!   sticky notes, settings, and which shared lists they're in.
//!
//! Every accepted change gets the collection's next revision number, so an app asks only
//! for what changed since the revision it last saw. Deletions stay as small "tombstones"
//! so devices that were offline learn about them too. The newest edit of an item wins.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Bumped only for changes old apps can't understand.
pub const PROTOCOL: u32 = 1;
const FILE: &str = "hub.json";
/// Daily copies of hub.json, in `backups/` next to it.
const BACKUP_DIR: &str = "backups";
/// How many daily copies to keep.
pub const KEEP_BACKUPS: usize = 14;
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

/// One person's synced data, found by its sync key.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Account {
    #[serde(default)]
    rev: u64,
    #[serde(default)]
    items: BTreeMap<String, Item>,
}

#[derive(Debug, Deserialize)]
pub struct AccountSyncRequest {
    pub key: String,
    #[serde(default)]
    pub since: u64,
    #[serde(default)]
    pub changes: Vec<Item>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AccountSyncResponse {
    pub rev: u64,
    pub changes: Vec<Item>,
}

/// What an account may hold.
const ACCOUNT_KINDS: [&str; 5] = ["list", "task", "note", "settings", "share"];

#[derive(Debug, Default, Serialize, Deserialize)]
struct Db {
    #[serde(default)]
    lists: BTreeMap<String, SharedList>,
    /// Sync key (normalized) → account.
    #[serde(default)]
    accounts: BTreeMap<String, Account>,
}

/// Applies `changes` to a collection (newest edit of each item wins) and returns what a
/// device that has revision `since` is missing, and whether anything changed.
fn merge(
    rev: &mut u64,
    items: &mut BTreeMap<String, Item>,
    since: u64,
    changes: Vec<Item>,
    allowed: &[&str],
    mut fix: impl FnMut(&mut Item),
) -> (Vec<Item>, bool) {
    // A device that's ahead of the hub (e.g. the hub's data was restored from an older
    // copy) gets everything again.
    let since = if since > *rev { 0 } else { since };
    let mut turned_down = HashSet::new();
    let mut changed = false;
    for mut item in changes {
        if !allowed.contains(&item.kind.as_str()) || item.id.is_empty() || item.id.len() > 64 {
            continue;
        }
        fix(&mut item);
        let key = item.key();
        if items.get(&key).is_some_and(|old| old.modified > item.modified) {
            turned_down.insert(key);
            continue;
        }
        *rev += 1;
        item.rev = *rev;
        if item.deleted {
            item.data = Value::Null;
        }
        items.insert(key, item);
        changed = true;
    }
    let out = items
        .iter()
        .filter(|(key, item)| item.rev > since || turned_down.contains(*key))
        .map(|(_, item)| item.clone())
        .collect();
    (out, changed)
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

    /// Where the daily copies go.
    pub fn backup_dir(&self) -> PathBuf {
        self.path.with_file_name(BACKUP_DIR)
    }

    /// Keeps today's copy of hub.json (`backups/hub-YYYY-MM-DD.json`) and the last
    /// [`KEEP_BACKUPS`] days. A backup problem is reported but never stops the hub.
    fn daily_backup(&self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        let dir = self.backup_dir();
        let name = format!("hub-{}.json", today());
        let target = dir.join(&name);
        if target.exists() {
            return;
        }
        let result = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(&self.path, &target));
        if let Err(e) = result {
            eprintln!("rustdo-hub: couldn't save today's backup in {}: {e}", dir.display());
            return;
        }
        let mut old: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("hub-") && n.ends_with(".json")))
                    .collect()
            })
            .unwrap_or_default();
        old.sort();
        let extra = old.len().saturating_sub(KEEP_BACKUPS);
        for p in old.into_iter().take(extra) {
            let _ = std::fs::remove_file(p);
        }
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
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        self.daily_backup();
        Ok(())
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
        let list_id = list.list_id.clone();
        // The list item always describes this list, whatever id the device sent.
        let (changes, changed) = merge(&mut list.rev, &mut list.items, req.since, req.changes, &["list", "task"], |item| {
            if item.kind == "list" {
                item.id = list_id.clone();
            }
        });
        let response = SyncResponse { list_id: list.list_id.clone(), rev: list.rev, changes };
        if changed {
            self.save()?;
        }
        Ok(Some(response))
    }

    pub fn account_count(&self) -> usize {
        self.db.accounts.len()
    }

    /// Starts a new account; returns its sync key, formatted like `ABCDE-FGHJK-MNPQR-STUVW`.
    pub fn new_account(&mut self) -> Result<String, String> {
        let key = loop {
            let key = random_code(20)?;
            if !self.db.accounts.contains_key(&key) {
                break key;
            }
        };
        self.db.accounts.insert(key.clone(), Account::default());
        self.save()?;
        Ok(key.as_bytes().chunks(5).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-"))
    }

    /// Syncs one person's devices. `None`: unknown sync key.
    pub fn account_sync(&mut self, req: AccountSyncRequest) -> Result<Option<AccountSyncResponse>, String> {
        let key = normalize_code(&req.key);
        if key.len() != 20 {
            return Ok(None);
        }
        let Some(account) = self.db.accounts.get_mut(&key) else {
            return Ok(None);
        };
        let (changes, changed) = merge(&mut account.rev, &mut account.items, req.since, req.changes, &ACCOUNT_KINDS, |_| {});
        let response = AccountSyncResponse { rev: account.rev, changes };
        if changed {
            self.save()?;
        }
        Ok(Some(response))
    }
}

/// Today's date (UTC) as YYYY-MM-DD, without a date library.
fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// "k7qm 2xpd", "K7QM-2XPD" → "K7QM2XPD".
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

fn new_code() -> Result<String, String> {
    random_code(8)
}

/// `len` random characters from [`CODE_CHARS`]. A sync key (20 characters) has about 99
/// bits of randomness.
fn random_code(len: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; len];
    getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
    // Rejection sampling: no bias towards the first characters.
    let limit = (256 / CODE_CHARS.len() * CODE_CHARS.len()) as u8;
    let mut out = String::with_capacity(len);
    let mut i = 0;
    while out.len() < len {
        if i == bytes.len() {
            getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
            i = 0;
        }
        let b = bytes[i];
        i += 1;
        if b < limit {
            out.push(CODE_CHARS[b as usize % CODE_CHARS.len()] as char);
        }
    }
    Ok(out)
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
    fn accounts_sync_everything_and_stay_separate() {
        let mut hub = Hub::memory();
        let key = hub.new_account().unwrap();
        assert_eq!(key.len(), 23);
        let other = hub.new_account().unwrap();
        assert_ne!(key, other);
        let note = Item { kind: "note".into(), id: "n1".into(), modified: 5, deleted: false, data: json!({ "text": "Hi" }), rev: 0 };
        let bad = Item { kind: "virus".into(), id: "x".into(), modified: 5, deleted: false, data: json!({}), rev: 0 };
        let req = |key: &str, since, changes| AccountSyncRequest { key: key.into(), since, changes };
        let r = hub.account_sync(req(&key, 0, vec![task("a", 10, "Milk"), note, bad])).unwrap().unwrap();
        assert_eq!(r.rev, 2);
        assert_eq!(r.changes.len(), 2);
        // Another device with the key typed loosely gets both; the other account nothing.
        let r2 = hub.account_sync(req(&key.to_lowercase().replace('-', ""), 0, vec![])).unwrap().unwrap();
        assert_eq!(r2.changes.len(), 2);
        let r3 = hub.account_sync(req(&other, 0, vec![])).unwrap().unwrap();
        assert!(r3.changes.is_empty());
        assert!(hub.account_sync(req("ABCDE-ABCDE-ABCDE-ABCDE", 0, vec![])).unwrap().is_none());
        assert!(hub.account_sync(req("short", 0, vec![])).unwrap().is_none());
        // Share codes and sync keys don't open each other.
        assert!(hub.sync(SyncRequest { code: key.clone(), since: 0, changes: vec![] }).unwrap().is_none());
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_783), (2026, 11, 26));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn keeps_daily_backups() {
        let dir = std::env::temp_dir().join(format!("rustdo-hub-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut hub = Hub::open(&dir).unwrap();
        let backups = hub.backup_dir();
        // Pretend there are older copies, more than are kept.
        for day in 1..=20 {
            std::fs::write(backups.join(format!("hub-2020-01-{day:02}.json")), "{}").unwrap();
        }
        // Today's copy already exists from opening; force a fresh one.
        let today_file = backups.join(format!("hub-{}.json", today()));
        std::fs::remove_file(&today_file).unwrap();
        hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
        assert!(today_file.exists(), "today's backup is written");
        let count = std::fs::read_dir(&backups).unwrap().count();
        assert_eq!(count, KEEP_BACKUPS, "only the last {KEEP_BACKUPS} days are kept");
        assert!(!backups.join("hub-2020-01-01.json").exists(), "the oldest go first");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_and_reloads() {
        let dir = std::env::temp_dir().join(format!("rustdo-hub-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let code = {
            let mut hub = Hub::open(&dir).unwrap();
            let code = hub.share(ShareRequest { list_id: "L1".into() }).unwrap();
            sync(&mut hub, &code, 0, vec![task("a", 10, "Milk")]);
            let key = hub.new_account().unwrap();
            hub.account_sync(AccountSyncRequest { key, since: 0, changes: vec![task("b", 1, "Eggs")] }).unwrap();
            code
        };
        let mut hub = Hub::open(&dir).unwrap();
        assert_eq!(hub.list_count(), 1);
        assert_eq!(hub.account_count(), 1);
        assert_eq!(sync(&mut hub, &code, 0, vec![]).changes.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
