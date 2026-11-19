//! System notifications for reminders.
//!
//! * **Windows**: a small background *reminder helper* (`rustdo.exe --reminders`: no
//!   window, no graphics) watches the task file and shows a Windows notification when a
//!   reminder is due, so reminders arrive even after RustDo's window is closed. It starts
//!   with RustDo and, for the installed version, when you sign in. Only one copy runs.
//!   (Windows discards *scheduled* notifications from desktop apps when the app exits,
//!   so scheduling ahead isn't an option.)
//! * **Linux**: a desktop notification when a reminder fires while RustDo is running.
//! * **Web**: a browser notification when a reminder fires while the app is open
//!   (after the user allows notifications).
//! * **Android**: in-app reminders only, for now.

use crate::model::{relative_date, Store};
use chrono::NaiveDateTime;

/// Text under the task title in a notification.
pub fn body_text(due: Option<chrono::NaiveDate>, today: chrono::NaiveDate) -> String {
    match due {
        Some(d) if d < today => format!("Reminder · overdue since {}", relative_date(d, today)),
        Some(d) => format!("Reminder · due {}", relative_date(d, today).to_lowercase()),
        None => "Reminder".to_string(),
    }
}

/// A reminder that came due: a stable key (task + reminder time, so a snoozed reminder
/// counts as new), the title and the notification text.
#[derive(Debug, Clone, PartialEq)]
pub struct Due {
    pub key: String,
    pub title: String,
    pub body: String,
}

/// Reminders on open tasks whose time falls in `(after, until]`.
pub fn due_between(store: &Store, after: NaiveDateTime, until: NaiveDateTime) -> Vec<Due> {
    store
        .tasks
        .iter()
        .filter(|t| !t.done)
        .filter_map(|t| {
            let at = t.reminder?;
            (at > after && at <= until).then(|| Due {
                key: format!("{}@{}", t.id, at),
                title: t.title.clone(),
                body: body_text(t.due, until.date()),
            })
        })
        .collect()
}

/// Shows a notification right now while the app is running (Linux and web). On Windows
/// the reminder helper shows them, so the app doesn't (no duplicates).
#[allow(unused_variables)]
pub fn show_now(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        let (title, body) = (title.to_string(), body.to_string());
        // D-Bus can take a moment; never block the UI on it.
        std::thread::spawn(move || {
            let _ = notify_rust::Notification::new()
                .appname("RustDo")
                .summary(&title)
                .body(&body)
                .icon("rustdo")
                .show();
        });
    }
    #[cfg(target_arch = "wasm32")]
    web::show(title, body);
}

/// Windows: starts the reminder helper (if it isn't already running) and keeps the
/// sign-in entry in line with the setting. Elsewhere: nothing.
#[allow(unused_variables)]
pub fn ensure_helper(enabled: bool) {
    #[cfg(windows)]
    win::ensure_helper(enabled);
}

/// Entry point for `rustdo.exe --reminders`. Runs until notifications are switched off.
pub fn run_reminder_helper() {
    #[cfg(windows)]
    win::helper_main();
}

/// Web: "granted", "denied", "default" (not asked yet) or "unsupported".
pub fn web_permission() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        return web::permission();
    }
    #[allow(unreachable_code)]
    "granted"
}

/// Web: asks the browser for permission (must follow a click).
pub fn web_request_permission() {
    #[cfg(target_arch = "wasm32")]
    web::request();
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
mod win {
    use std::collections::HashSet;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::time::Duration;
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    /// Identifies RustDo to Windows' notification system (also set on the Start menu
    /// shortcut by the installer).
    pub const AUMID: &str = "metman2013.RustDo";
    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_VALUE: &str = "RustDo reminders";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    fn reg(args: &[&str]) {
        let _ = std::process::Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).status();
    }

    fn local_dir() -> Option<PathBuf> {
        Some(std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?.join("RustDo"))
    }

    /// Registers RustDo as a notification sender for this user (name and icon).
    pub fn register_app() {
        let key = format!(r"HKCU\Software\Classes\AppUserModelId\{AUMID}");
        reg(&["add", &key, "/v", "DisplayName", "/t", "REG_SZ", "/d", "RustDo", "/f"]);
        if let Some(dir) = local_dir() {
            let icon = dir.join("notification-icon.png");
            if !icon.exists() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(&icon, include_bytes!("../web/icons/icon-192.png"));
            }
            reg(&["add", &key, "/v", "IconUri", "/t", "REG_SZ", "/d", &icon.display().to_string(), "/f"]);
        }
    }

    pub fn ensure_helper(enabled: bool) {
        register_app();
        let Ok(exe) = std::env::current_exe() else { return };
        if !enabled {
            reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
            return; // A running helper sees the setting and exits by itself.
        }
        // Start at sign-in, for the installed copy only (its path doesn't move).
        if crate::update::is_installed_copy() {
            let command = format!("\"{}\" --reminders", exe.display());
            reg(&["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &command, "/f"]);
        }
        // Start now; a second copy notices the first and quits straight away.
        let _ = std::process::Command::new(exe)
            .arg("--reminders")
            .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
            .spawn();
    }

    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn show(title: &str, body: &str) -> windows::core::Result<()> {
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(format!(
            r#"<toast launch="rustdo-reminder"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual><audio src="ms-winsoundevent:Notification.Reminder"/></toast>"#,
            escape(title),
            escape(body)
        )))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)
    }

    /// One helper per user: holds an exclusive lock on a file while it runs.
    fn single_instance_lock() -> Option<std::fs::File> {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = local_dir()?;
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0) // nobody else may open it while we hold it
            .open(dir.join("reminders.lock"))
            .ok()
    }

    pub fn helper_main() {
        let Some(_lock) = single_instance_lock() else { return };
        register_app();
        let path = crate::storage::data_path();
        let now = || chrono::Local::now().naive_local();
        // Reminders that passed shortly before the helper started (e.g. during sign-in)
        // still get shown; older ones don't.
        let mut checked_until = now() - chrono::Duration::minutes(10);
        let mut shown: HashSet<String> = HashSet::new();
        loop {
            let until = now();
            // Read-only: the helper never writes the task file.
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(store) = crate::storage::parse_backup(&bytes) {
                    if !store.settings.notifications {
                        return; // switched off in ⚙
                    }
                    for due in super::due_between(&store, checked_until, until) {
                        if shown.insert(due.key.clone()) {
                            let _ = show(&due.title, &due.body);
                        }
                    }
                }
            }
            checked_until = until;
            std::thread::sleep(Duration::from_secs(15));
        }
    }
}

// ---------------------------------------------------------------- Web

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::JsCast;

    fn supported() -> bool {
        web_sys::window()
            .is_some_and(|w| js_sys::Reflect::has(&w, &"Notification".into()).unwrap_or(false))
    }

    pub fn permission() -> &'static str {
        if !supported() {
            return "unsupported";
        }
        match web_sys::Notification::permission() {
            web_sys::NotificationPermission::Granted => "granted",
            web_sys::NotificationPermission::Denied => "denied",
            _ => "default",
        }
    }

    pub fn request() {
        if supported() {
            let _ = web_sys::Notification::request_permission();
        }
    }

    /// Prefers the service worker (required on Android Chrome), falls back to the page.
    pub fn show(title: &str, body: &str) {
        if permission() != "granted" {
            return;
        }
        let options = web_sys::NotificationOptions::new();
        options.set_body(body);
        options.set_icon("icons/icon-192.png");
        options.set_tag("rustdo-reminder");
        let title = title.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let via_worker = async {
                let window = web_sys::window()?;
                let ready = window.navigator().service_worker().ready().ok()?;
                let reg = wasm_bindgen_futures::JsFuture::from(ready).await.ok()?;
                let reg: web_sys::ServiceWorkerRegistration = reg.dyn_into().ok()?;
                reg.show_notification_with_options(&title, &options).ok()
            }
            .await;
            if via_worker.is_none() {
                let _ = web_sys::Notification::new_with_options(&title, &options);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::View;

    #[test]
    fn due_between_window_and_keys() {
        let mut s = Store::new();
        let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap().and_hms_opt(9, 0, 0).unwrap();
        let a = s.add_task("In window", View::All, now).unwrap();
        let b = s.add_task("Too early", View::All, now).unwrap();
        let c = s.add_task("Done", View::All, now).unwrap();
        let d = s.add_task("Later", View::All, now).unwrap();
        s.task_mut(a).unwrap().reminder = Some(now + chrono::Duration::seconds(10));
        s.task_mut(b).unwrap().reminder = Some(now - chrono::Duration::minutes(1));
        s.task_mut(c).unwrap().reminder = Some(now + chrono::Duration::seconds(10));
        s.task_mut(c).unwrap().done = true;
        s.task_mut(d).unwrap().reminder = Some(now + chrono::Duration::hours(1));
        let due = due_between(&s, now, now + chrono::Duration::seconds(15));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "In window");
        // Snoozing changes the reminder time, which makes it a new key.
        let key = due[0].key.clone();
        s.task_mut(a).unwrap().reminder = Some(now + chrono::Duration::minutes(10));
        let later = due_between(&s, now, now + chrono::Duration::minutes(11));
        assert!(later.iter().all(|x| x.key != key));
    }

    #[test]
    fn body_mentions_due_date() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        assert_eq!(body_text(Some(today), today), "Reminder · due today");
        assert_eq!(body_text(None, today), "Reminder");
        assert!(body_text(Some(today - chrono::Duration::days(3)), today).contains("overdue"));
    }
}
