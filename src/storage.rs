//! Local JSON persistence with atomic writes and a backup of the last good file.

use crate::model::Store;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// `%APPDATA%\RustDo\data\tasks.json` on Windows (platform equivalent elsewhere).
/// Override with the `RUSTDO_DATA` environment variable.
pub fn data_path() -> PathBuf {
    if let Some(p) = std::env::var_os("RUSTDO_DATA") {
        return PathBuf::from(p);
    }
    directories::ProjectDirs::from("", "", "RustDo")
        .map(|d| d.data_dir().join("tasks.json"))
        .unwrap_or_else(|| PathBuf::from("tasks.json"))
}

pub struct Loaded {
    pub store: Store,
    /// Shown to the user if something went wrong while loading.
    pub warning: Option<String>,
    /// False when the existing file could not be read *or* moved aside: we must not
    /// overwrite it, so saving is disabled for this session.
    pub can_save: bool,
}

pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Loaded { store: Store::new(), warning: None, can_save: true };
        }
        Err(e) => return set_aside(path, format!("Couldn't read {}: {e}", path.display())),
    };
    // Windows editors (Notepad, PowerShell) often prepend a UTF-8 byte-order mark.
    match serde_json::from_str::<Store>(text.trim_start_matches('\u{feff}')) {
        Ok(mut store) => {
            store.repair();
            // Keep a copy of the last file that loaded successfully.
            let _ = fs::write(path.with_extension("json.bak"), &text);
            Loaded { store, warning: None, can_save: true }
        }
        Err(e) => set_aside(path, format!("Your task file was damaged ({e}).")),
    }
}

/// Moves an unreadable file out of the way so starting fresh never destroys it.
fn set_aside(path: &Path, reason: String) -> Loaded {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let aside = path.with_extension(format!("corrupt-{stamp}.json"));
    match fs::rename(path, &aside) {
        Ok(()) => Loaded {
            store: Store::new(),
            warning: Some(format!("{reason} It was saved as {}.", aside.display())),
            can_save: true,
        },
        Err(_) => Loaded {
            store: Store::new(),
            warning: Some(format!("{reason} Saving is disabled to protect it.")),
            can_save: false,
        },
    }
}

/// Saves a timestamped copy of `store` next to the data file (e.g. before an import)
/// and returns its path.
pub fn snapshot(path: &Path, store: &Store, label: &str) -> io::Result<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let copy = path.with_extension(format!("{label}-{stamp}.json"));
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
}
