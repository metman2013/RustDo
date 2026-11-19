//! Persistence. Desktop and Android save a JSON file (atomic writes, a backup of the last
//! good file); the web app saves the same JSON in the browser's localStorage.

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
    pub fn for_platform() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Storage::Browser
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Storage::File(data_path())
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

// ---------------------------------------------------------------- files (desktop, Android)

/// Desktop: `%APPDATA%\RustDo\data\tasks.json` on Windows (platform equivalent elsewhere).
/// Android: the app's private internal storage. Override with `RUSTDO_DATA`.
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
        directories::ProjectDirs::from("", "", "RustDo")
            .map(|d| d.data_dir().join("tasks.json"))
            .unwrap_or_else(|| PathBuf::from("tasks.json"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::from("tasks.json")
    }
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
        Err(e) => set_aside(path, format!("Your task file was damaged ({e}).")),
    }
}

/// Moves an unreadable file out of the way so starting fresh never destroys it.
fn set_aside(path: &Path, reason: String) -> Loaded {
    let aside = path.with_extension(format!("corrupt-{}.json", stamp()));
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

/// Saves a timestamped copy of `store` next to the data file and returns its path.
pub fn snapshot(path: &Path, store: &Store, label: &str) -> io::Result<PathBuf> {
    let copy = path.with_extension(format!("{label}-{}.json", stamp()));
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
                warning: Some("This browser won't let RustDo store data, so your tasks won't be saved.".into()),
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
                        format!("Your saved tasks were damaged ({e}). A copy was kept in browser storage as “{aside}”.")
                    } else {
                        format!("Your saved tasks were damaged ({e}). Saving is disabled to protect them.")
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
    fn snapshot_skips_when_no_data_yet() {
        let path = temp_file();
        let storage = Storage::File(path.clone());
        storage.snapshot(&Store::new(), "before-import").unwrap();
        assert!(!path.parent().unwrap().exists());
    }
}
