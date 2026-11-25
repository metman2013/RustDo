//! Persistence. Desktop and Android save a JSON file (atomic writes, a backup of the last
//! good file); the web app saves the same JSON in the browser's localStorage.

use crate::i18n::{tr, trf};
use crate::model::Store;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct Loaded {
    pub store: Store,
    /// Shown to the user if something went wrong while loading.
    pub warning: Option<String>,
    /// False when existing data could not be read *or* moved aside: we must not
    /// overwrite it, so saving is disabled for this session.
    pub can_save: bool,
}

fn parse(text: &str) -> Result<Store, String> {
    // Windows editors (Notepad, PowerShell) often prepend a UTF-8 byte-order mark.
    let mut store: Store = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    store.repair();
    Ok(store)
}

fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Where this platform keeps the user's tasks.
pub enum Storage {
    File(PathBuf),
    #[cfg(target_arch = "wasm32")]
    Browser,
}

impl Storage {
    /// The storage for this platform, plus a message to show if data was moved.
    pub fn for_platform() -> (Self, Option<String>) {
        #[cfg(target_arch = "wasm32")]
        {
            (Storage::Browser, None)
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            let path = data_path();
            if std::env::var_os("RUSTDO_DATA").is_some() {
                return (Storage::File(path), None);
            }
            match legacy_data_path() {
                Some(old) => match migrate(&old, &path) {
                    Ok(true) => (
                        Storage::File(path.clone()),
                        Some(trf("Your tasks now live in {path} so they're easy to find.", &[("path", &path.parent().unwrap_or(&path).display())])),
                    ),
                    Ok(false) => (Storage::File(path), None),
                    // Couldn't move: keep using the old location rather than starting empty.
                    Err(e) => (
                        Storage::File(old.clone()),
                        Some(trf("Couldn't move your tasks to Documents ({e}); still using {path}.", &[("e", &e), ("path", &old.display())])),
                    ),
                },
                None => (Storage::File(path), None),
            }
        }
        #[cfg(target_os = "android")]
        {
            (Storage::File(data_path()), None)
        }
    }

    /// The folder holding the data file (desktop and Android).
    pub fn folder(&self) -> Option<PathBuf> {
        match self {
            Storage::File(path) => path.parent().map(Path::to_path_buf),
            #[cfg(target_arch = "wasm32")]
            Storage::Browser => None,
        }
    }

    pub fn load(&self) -> Loaded {
        match self {
            Storage::File(path) => load(path),
            #[cfg(target_arch = "wasm32")]
            Storage::Browser => browser::load(),
        }
    }

    pub fn save(&self, store: &Store) -> Result<(), String> {
        match self {
            Storage::File(path) => save(path, store).map_err(|e| e.to_string()),
            #[cfg(target_arch = "wasm32")]
            Storage::Browser => browser::save(store),
        }
    }

    /// Writes today's automatic backup (replacing today's earlier one) and prunes old ones.
    /// Desktop/Android: `backups/` next to the data file, plus the user's extra backup folder
    /// if set. Web: browser storage.
    pub fn auto_backup(&self, store: &Store, today: chrono::NaiveDate) -> Result<(), String> {
        match self {
            Storage::File(path) => {
                write_daily_backup(&backups_dir(path), store, today, KEEP_BACKUPS).map_err(|e| e.to_string())?;
                if let Some(extra) = store.settings.backup_dir.as_deref().filter(|d| !d.trim().is_empty()) {
                    write_daily_backup(Path::new(extra), store, today, KEEP_BACKUPS)
                        .map_err(|e| trf("couldn't write to your backup folder {extra}: {e}", &[("extra", &extra), ("e", &e)]))?;
                }
                Ok(())
            }
            #[cfg(target_arch = "wasm32")]
            Storage::Browser => browser::auto_backup(store, today),
        }
    }

    /// Desktop/Android: the folders that receive automatic backups (Backups next to the
    /// data file first, then the user's extra backup folder if set).
    pub fn backup_dirs(&self, settings: &crate::model::Settings) -> Vec<PathBuf> {
        match self {
            Storage::File(path) => {
                let mut dirs = vec![backups_dir(path)];
                if let Some(extra) = settings.backup_dir.as_deref().filter(|d| !d.trim().is_empty()) {
                    dirs.push(PathBuf::from(extra));
                }
                dirs
            }
            #[cfg(target_arch = "wasm32")]
            Storage::Browser => Vec::new(),
        }
    }

    /// Desktop and Android: today's sticky notes backup, next to the task backups.
    pub fn auto_backup_notes(
        &self,
        notes: &crate::notes::Notes,
        settings: &crate::model::Settings,
        today: chrono::NaiveDate,
    ) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        for dir in self.backup_dirs(settings) {
            crate::notes::file::write_daily_backup(&dir, notes, today, KEEP_BACKUPS)
                .map_err(|e| trf("couldn't back up your sticky notes to {path}: {e}", &[("path", &dir.display()), ("e", &e)]))?;
        }
        let _ = (notes, settings, today);
        Ok(())
    }

    /// Where the automatic backups go, for showing to the user.
    pub fn backups_location(&self) -> String {
        match self {
            Storage::File(path) => backups_dir(path).display().to_string(),
            #[cfg(target_arch = "wasm32")]
            Storage::Browser => tr("this browser's storage (last 7 days)").into(),
        }
    }

    /// Keeps a copy of `store` (e.g. before an import). Does nothing if there's no data yet.
    pub fn snapshot(&self, store: &Store, label: &str) -> Result<(), String> {
        match self {
            Storage::File(path) => {
                if path.exists() {
                    snapshot(path, store, label).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            #[cfg(target_arch = "wasm32")]
            Storage::Browser => browser::snapshot(store, label),
        }
    }
}

// ---------------------------------------------------------------- backups

/// How many daily backup files to keep.
pub const KEEP_BACKUPS: usize = 14;
const BACKUP_PREFIX: &str = "RustDo-backup-";

pub fn backup_file_name(date: chrono::NaiveDate) -> String {
    format!("{BACKUP_PREFIX}{}.json", date.format("%Y-%m-%d"))
}

/// The full data as pretty JSON: the export format, identical to the data file.
pub fn export_json(store: &Store) -> String {
    serde_json::to_string_pretty(store).unwrap_or_default()
}

/// The export format, with the sticky notes alongside (under `sticky_notes`) when there
/// are any. Older RustDo versions read it fine: they ignore the extra field.
pub fn export_json_with_notes(store: &Store, notes: Option<&crate::notes::Notes>) -> String {
    let Some(notes) = notes.filter(|n| !n.notes.is_empty()) else {
        return export_json(store);
    };
    let Ok(mut value) = serde_json::to_value(store) else { return export_json(store) };
    if let (Some(map), Ok(n)) = (value.as_object_mut(), serde_json::to_value(notes)) {
        map.insert(crate::notes::BACKUP_KEY.to_string(), n);
    }
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| export_json(store))
}

/// Reads a RustDo backup/export (or a copy of tasks.json).
pub fn parse_backup(bytes: &[u8]) -> Result<Store, String> {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with('{') || !trimmed.contains("\"default_list\"") {
        return Err(tr("This isn't a RustDo backup.").into());
    }
    parse(trimmed).map_err(|e| trf("This backup is damaged ({e}).", &[("e", &e)]))
}

/// Writes `RustDo-backup-<today>.json` into `dir` and deletes all but the newest `keep`.
pub fn write_daily_backup(dir: &Path, store: &Store, today: chrono::NaiveDate, keep: usize) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(backup_file_name(today));
    save(&path, store)?;
    // Names sort by date, so the newest come last.
    let mut backups: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(BACKUP_PREFIX) && n.ends_with(".json"))
        })
        .collect();
    backups.sort();
    let excess = backups.len().saturating_sub(keep);
    for old in &backups[..excess] {
        let _ = fs::remove_file(old);
    }
    Ok(path)
}

// ---------------------------------------------------------------- files (desktop, Android)

/// Desktop: `Documents\RustDo\tasks.json` (following a Documents folder redirected to
/// OneDrive). Android: the app's private internal storage. Override with `RUSTDO_DATA`.
pub fn data_path() -> PathBuf {
    if let Some(p) = std::env::var_os("RUSTDO_DATA") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "android")]
    {
        crate::platform::android_data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tasks.json")
    }
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        directories::UserDirs::new()
            .and_then(|u| u.document_dir().map(|d| d.join("RustDo").join("tasks.json")))
            .or_else(legacy_data_path)
            .unwrap_or_else(|| PathBuf::from("tasks.json"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::from("tasks.json")
    }
}

/// Where version 0.1 kept the data: `%APPDATA%\RustDo\data\tasks.json` on Windows.
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn legacy_data_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "RustDo").map(|d| d.data_dir().join("tasks.json"))
}

/// Daily backups and safety copies live in `Backups` next to the data file.
pub fn backups_dir(data_file: &Path) -> PathBuf {
    data_file.parent().unwrap_or(Path::new(".")).join("Backups")
}

/// Moves data from `old` to `new` if `new` doesn't exist yet. Copies first, then renames
/// the old file (never deletes it). Returns whether anything was moved.
pub fn migrate(old: &Path, new: &Path) -> io::Result<bool> {
    if old == new || new.exists() || !old.exists() {
        return Ok(false);
    }
    if let Some(dir) = new.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::copy(old, new)?;
    let old_bak = old.with_extension("json.bak");
    if old_bak.exists() {
        let _ = fs::copy(&old_bak, new.with_extension("json.bak"));
    }
    // Make sure the copy is readable before retiring the original.
    if parse(&fs::read_to_string(new)?).is_err() {
        let _ = fs::remove_file(new);
        return Err(io::Error::other("the copied file couldn't be read back"));
    }
    let _ = fs::rename(old, old.with_extension("json.moved-to-Documents"));
    Ok(true)
}

pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Loaded { store: Store::new(), warning: None, can_save: true };
        }
        Err(e) => return set_aside(path, format!("Couldn't read {}: {e}", path.display())),
    };
    match parse(&text) {
        Ok(store) => {
            // Keep a copy of the last file that loaded successfully.
            let _ = fs::write(path.with_extension("json.bak"), &text);
            Loaded { store, warning: None, can_save: true }
        }
        Err(e) => set_aside(path, trf("Your task file was damaged ({e}).", &[("e", &e)])),
    }
}

/// Moves an unreadable file out of the way so starting fresh never destroys it.
fn set_aside(path: &Path, reason: String) -> Loaded {
    let aside = path.with_extension(format!("corrupt-{}.json", stamp()));
    match fs::rename(path, &aside) {
        Ok(()) => Loaded {
            store: Store::new(),
            warning: Some(format!("{reason} {}", trf("It was saved as {path}.", &[("path", &aside.display())]))),
            can_save: true,
        },
        Err(_) => Loaded {
            store: Store::new(),
            warning: Some(trf("{reason} Saving is disabled to protect it.", &[("reason", &reason)])),
            can_save: false,
        },
    }
}

/// Saves a timestamped copy of `store` in the Backups folder and returns its path.
pub fn snapshot(path: &Path, store: &Store, label: &str) -> io::Result<PathBuf> {
    let copy = backups_dir(path).join(format!("RustDo-{label}-{}.json", stamp()));
    save(&copy, store)?;
    Ok(copy)
}

pub fn save(path: &Path, store: &Store) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(store).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

// ---------------------------------------------------------------- browser (PWA)

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;

    const KEY: &str = "rustdo.tasks";

    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn load() -> Loaded {
        let Some(ls) = local_storage() else {
            return Loaded {
                store: Store::new(),
                warning: Some(tr("This browser won't let RustDo store data, so your tasks won't be saved.").into()),
                can_save: false,
            };
        };
        let Ok(Some(text)) = ls.get_item(KEY) else {
            return Loaded { store: Store::new(), warning: None, can_save: true };
        };
        match parse(&text) {
            Ok(store) => {
                let _ = ls.set_item(&format!("{KEY}.bak"), &text);
                Loaded { store, warning: None, can_save: true }
            }
            Err(e) => {
                let aside = format!("{KEY}.corrupt-{}", stamp());
                let kept = ls.set_item(&aside, &text).is_ok();
                Loaded {
                    store: Store::new(),
                    warning: Some(if kept {
                        trf("Your saved tasks were damaged ({e}). A copy was kept in browser storage as “{aside}”.", &[("e", &e), ("aside", &aside)])
                    } else {
                        trf("Your saved tasks were damaged ({e}). Saving is disabled to protect them.", &[("e", &e)])
                    }),
                    can_save: kept,
                }
            }
        }
    }

    fn put(key: &str, store: &Store) -> Result<(), String> {
        let ls = local_storage().ok_or("browser storage is unavailable")?;
        let json = serde_json::to_string(store).map_err(|e| e.to_string())?;
        ls.set_item(key, &json).map_err(|_| "browser storage is full or blocked".to_string())
    }

    pub fn save(store: &Store) -> Result<(), String> {
        put(KEY, store)
    }

    /// Keeps one copy per label (browser storage is small, so no timestamped history).
    pub fn snapshot(store: &Store, label: &str) -> Result<(), String> {
        put(&format!("{KEY}.{label}"), store)
    }

    const BACKUP_KEY: &str = "rustdo.backup.";
    const KEEP: usize = 7;

    pub fn auto_backup(store: &Store, today: chrono::NaiveDate) -> Result<(), String> {
        put(&format!("{BACKUP_KEY}{}", today.format("%Y-%m-%d")), store)?;
        let ls = local_storage().ok_or("browser storage is unavailable")?;
        let mut keys: Vec<String> = (0..ls.length().unwrap_or(0))
            .filter_map(|i| ls.key(i).ok().flatten())
            .filter(|k| k.starts_with(BACKUP_KEY))
            .collect();
        keys.sort();
        let excess = keys.len().saturating_sub(KEEP);
        for k in &keys[..excess] {
            let _ = ls.remove_item(k);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::View;

    fn temp_file() -> PathBuf {
        std::env::temp_dir()
            .join(format!("rustdo-test-{}", uuid::Uuid::new_v4()))
            .join("tasks.json")
    }

    #[test]
    fn round_trip() {
        let path = temp_file();
        let mut store = Store::new();
        let now = chrono::Local::now().naive_local();
        store.add_task("Hello", View::MyDay, now);
        save(&path, &store).unwrap();
        let loaded = load(&path);
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.store.tasks, store.tasks);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn utf8_bom_is_accepted() {
        let path = temp_file();
        let store = Store::new();
        save(&path, &store).unwrap();
        let json = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("\u{feff}{json}")).unwrap();
        let loaded = load(&path);
        assert!(loaded.warning.is_none(), "{:?}", loaded.warning);
        assert_eq!(loaded.store.default_list, store.default_list);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn corrupt_file_is_preserved() {
        let path = temp_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();
        let loaded = load(&path);
        assert!(loaded.warning.is_some());
        assert!(loaded.can_save);
        assert!(!path.exists(), "corrupt file moved aside");
        let kept = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
        assert!(kept);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn daily_backups_rotate() {
        let dir = std::env::temp_dir().join(format!("rustdo-test-{}", uuid::Uuid::new_v4()));
        let store = Store::new();
        let day = |d: u32| chrono::NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        for d in 1..=5 {
            write_daily_backup(&dir, &store, day(d), 3).unwrap();
        }
        // Writing the same day twice replaces that day's file.
        write_daily_backup(&dir, &store, day(5), 3).unwrap();
        fs::write(dir.join("unrelated.txt"), "keep me").unwrap();
        let mut names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "RustDo-backup-2026-11-03.json",
                "RustDo-backup-2026-11-04.json",
                "RustDo-backup-2026-11-05.json",
                "unrelated.txt"
            ]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_and_restore_round_trip() {
        let mut store = Store::new();
        store.add_task("Backed up", View::All, chrono::Local::now().naive_local());
        let json = export_json(&store);
        let back = parse_backup(format!("\u{feff}{json}").as_bytes()).unwrap();
        assert_eq!(back.tasks, store.tasks);
        assert!(parse_backup(br#"{"lists":[]}"#).is_err(), "Microsoft To Do exports aren't backups");
        assert!(parse_backup(b"not json").is_err());
    }

    #[test]
    fn exports_carry_sticky_notes_and_still_restore_as_backups() {
        let now = chrono::Local::now().naive_local();
        let mut store = Store::new();
        store.add_task("Backed up", View::All, now);
        let mut notes = crate::notes::Notes::default();
        let id = notes.add(now, None);
        notes.get_mut(id).unwrap().text = "Call mum".into();

        let json = export_json_with_notes(&store, Some(&notes));
        assert_eq!(parse_backup(json.as_bytes()).unwrap().tasks, store.tasks);
        assert_eq!(crate::notes::parse_backup(json.as_bytes()).unwrap(), notes);

        // No notes: exactly the old format.
        assert_eq!(export_json_with_notes(&store, Some(&crate::notes::Notes::default())), export_json(&store));
        assert_eq!(export_json_with_notes(&store, None), export_json(&store));
    }

    #[test]
    fn migrates_old_location_without_deleting() {
        let base = std::env::temp_dir().join(format!("rustdo-test-{}", uuid::Uuid::new_v4()));
        let old = base.join("AppData").join("tasks.json");
        let new = base.join("Documents").join("RustDo").join("tasks.json");
        let mut store = Store::new();
        store.add_task("Move me", View::All, chrono::Local::now().naive_local());
        save(&old, &store).unwrap();

        assert!(migrate(&old, &new).unwrap());
        assert_eq!(load(&new).store.tasks, store.tasks);
        assert!(!old.exists());
        assert!(old.with_extension("json.moved-to-Documents").exists(), "original kept, renamed");
        // Second start: nothing to do.
        assert!(!migrate(&old, &new).unwrap());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn snapshots_go_to_backups_folder() {
        let path = temp_file();
        save(&path, &Store::new()).unwrap();
        let copy = snapshot(&path, &Store::new(), "before-import").unwrap();
        assert_eq!(copy.parent().unwrap(), backups_dir(&path));
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn snapshot_skips_when_no_data_yet() {
        let path = temp_file();
        let storage = Storage::File(path.clone());
        storage.snapshot(&Store::new(), "before-import").unwrap();
        assert!(!path.parent().unwrap().exists());
    }
}
