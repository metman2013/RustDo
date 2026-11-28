//! The background companion (Windows and Linux): `rustdo.exe --companion`.
//!
//! The same program, started with a flag. It keeps running after the main window closes,
//! with a tray icon (Open RustDo, New sticky note, Show / Hide all notes, Quit), and owns
//! the sticky note windows while it runs. On Windows it also shows reminder notifications
//! (this replaces the older `--reminders` helper, which now starts the companion too).
//!
//! It starts when RustDo opens and, if allowed in settings, when the user signs in. Only
//! one copy runs (a lock file). The main window and the companion share `notes.json`; each
//! reloads it when the other saves.
//!
//! Started at sign-in with "Start the companion when I sign in" turned off but reminders on
//! (Windows), it runs for reminders only: no sticky notes, until RustDo is opened and asks
//! it to wake up. It always shows its tray icon, so it's never running unseen, and the
//! icon's menu can quit RustDo completely.

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use crate::i18n::tr;
use crate::model::Settings;
use std::fs::File;
use std::path::PathBuf;

/// The companion needs real desktop windows and a tray: not on Android or the web.
pub const SUPPORTED: bool = crate::notes::SUPPORTED;

/// Arguments that start the companion. `--reminders` is what versions before 0.2.7
/// registered to start at sign-in.
pub fn is_companion_launch(args: &[String]) -> bool {
    args.iter().any(|a| a == "--companion" || a == "--reminders")
}

/// Started by Windows / the desktop at sign-in (rather than by RustDo).
pub fn is_sign_in_launch(args: &[String]) -> bool {
    args.iter().any(|a| a == "--at-sign-in" || a == "--reminders")
}

/// Whether the companion should be running at all.
pub fn wanted(settings: &Settings) -> bool {
    SUPPORTED && (settings.companion || reminders_when_closed(settings))
}

/// Windows: reminders keep arriving after RustDo is closed (the companion shows them).
pub fn reminders_when_closed(settings: &Settings) -> bool {
    cfg!(windows) && settings.notifications && settings.background_reminders
}

/// Whether it shows the tray icon and sticky notes (not just reminders).
pub fn full(settings: &Settings, started_at_sign_in: bool) -> bool {
    SUPPORTED && settings.companion && (settings.companion_autostart || !started_at_sign_in)
}

/// Whether it should start when the user signs in.
pub fn starts_at_sign_in(settings: &Settings) -> bool {
    SUPPORTED && ((settings.companion && settings.companion_autostart) || reminders_when_closed(settings))
}

/// A small folder for the lock and request files (not the user's Documents).
fn runtime_dir() -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        let dir = directories::ProjectDirs::from("", "", "RustDo")?.data_local_dir().to_path_buf();
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }
    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    None
}

/// A file in the runtime folder (desktop).
pub fn runtime_path(name: &str) -> Option<PathBuf> {
    runtime_dir().map(|d| d.join(name))
}

/// An exclusive lock on `name` in the runtime folder, or `None` if another process holds it.
fn try_lock(name: &str) -> Option<File> {
    let path = runtime_dir()?.join(name);
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(path).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

const COMPANION_LOCK: &str = "companion.lock";
const APP_LOCK: &str = "app.lock";
/// The companion asks the main window to come to the front.
const SHOW_APP_REQUEST: &str = "show-app.request";
/// RustDo asks a quiet (reminders-only) companion to show its tray icon and notes.
const WAKE_REQUEST: &str = "companion-wake.request";
/// "Quit RustDo completely" in the tray: the main window closes too.
const QUIT_APP_REQUEST: &str = "quit-app.request";
/// "Quit RustDo completely" in the main window: the companion stops too.
const QUIT_COMPANION_REQUEST: &str = "quit-companion.request";

/// Held by the companion while it runs.
pub fn lock_companion() -> Option<File> {
    // A moment's patience: RustDo briefly takes the lock to check whether we're running.
    for _ in 0..5 {
        if let Some(f) = try_lock(COMPANION_LOCK) {
            return Some(f);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    None
}

/// Held by the main window while it's open (so the tray's "Open RustDo" brings it to the
/// front instead of starting a second copy).
pub fn lock_app() -> Option<File> {
    try_lock(APP_LOCK)
}

/// Whether the companion is running (its lock is held).
pub fn is_running() -> bool {
    match try_lock(COMPANION_LOCK) {
        Some(_free) => false, // dropped right away: we only looked
        None => runtime_dir().is_some(),
    }
}

/// Whether the main window is open.
pub fn app_is_open() -> bool {
    runtime_dir().is_some() && try_lock(APP_LOCK).is_none()
}

fn request(name: &str) {
    if let Some(dir) = runtime_dir() {
        let _ = std::fs::write(dir.join(name), b"1");
    }
}

/// Takes (and clears) a pending request.
fn take_request(name: &str) -> bool {
    runtime_dir().is_some_and(|dir| std::fs::remove_file(dir.join(name)).is_ok())
}

pub fn request_show_app() {
    request(SHOW_APP_REQUEST);
}

pub fn take_show_app_request() -> bool {
    take_request(SHOW_APP_REQUEST)
}

/// The tray's "Quit RustDo completely" asked the main window to close.
pub fn take_quit_app_request() -> bool {
    take_request(QUIT_APP_REQUEST)
}

/// Asks the companion to stop (RustDo is quitting completely). Until RustDo is opened
/// again or the next sign-in.
pub fn request_quit() {
    request(QUIT_COMPANION_REQUEST);
}

/// Starts the companion if it should be running (and isn't), and keeps the start-at-sign-in
/// entry in line with the settings. Called by RustDo shortly after it opens and whenever
/// these settings change. Runs on a background thread (registry/desktop file writes).
pub fn ensure(settings: &Settings) {
    if !SUPPORTED {
        return;
    }
    crate::notify::register_app();
    set_sign_in_entry(starts_at_sign_in(settings));
    if !wanted(settings) {
        return; // a running companion sees the settings and stops by itself
    }
    if is_running() {
        // It may be running quietly (reminders only): ask it to show the tray and notes.
        if settings.companion {
            request(WAKE_REQUEST);
        }
        return;
    }
    spawn(&["--companion"]);
}

/// Starts this program again with `args`, detached from the current one.
pub fn spawn(args: &[&str]) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut command = std::process::Command::new(exe);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    let _ = command.spawn();
}

/// The command that starts the companion at sign-in. On Linux an AppImage runs from a
/// temporary mount, so its own file ($APPIMAGE) is what must be started.
#[cfg(any(windows, target_os = "linux"))]
fn sign_in_command() -> Option<String> {
    let exe = std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok())?;
    Some(format!("\"{}\" --companion --at-sign-in", exe.display()))
}

#[allow(unused_variables)]
fn set_sign_in_entry(on: bool) {
    #[cfg(windows)]
    if crate::store_package::is_packaged() {
        // A Store package can't use the Run key: it has a startup task instead.
        crate::store_package::set_startup_task(on);
    } else {
        // Only the installed copy: a portable exe may move or be deleted.
        if on && crate::update::is_installed_copy() {
            crate::notify::set_run_entry(sign_in_command().as_deref());
        } else {
            crate::notify::set_run_entry(None);
        }
    }
    #[cfg(target_os = "linux")]
    {
        let Some(dir) = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("autostart"))
        else {
            return;
        };
        let file = dir.join("rustdo-companion.desktop");
        match sign_in_command().filter(|_| on) {
            Some(command) => {
                let entry = format!(
                    "[Desktop Entry]\nType=Application\nName=RustDo companion\n\
                     Comment=Sticky notes and the RustDo tray icon\nExec={command}\nIcon=rustdo\n\
                     NoDisplay=true\nX-GNOME-Autostart-enabled=true\n"
                );
                if std::fs::read_to_string(&file).ok().as_deref() != Some(entry.as_str()) {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(&file, entry);
                }
            }
            None => {
                let _ = std::fs::remove_file(&file);
            }
        }
    }
}

// ---------------------------------------------------------------- the companion process

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub use process::run;

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
mod process {
    use super::*;
    use crate::notes_ui::{self, SharedNotes};
    use eframe::egui;
    use std::sync::mpsc::{channel, Receiver};
    use std::time::{Duration, Instant};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    const POLL: Duration = Duration::from_secs(1);

    enum TrayCommand {
        OpenApp,
        NewNote,
        ShowAll,
        HideAll,
        Quit,
    }

    /// Entry point for `rustdo.exe --companion`.
    pub fn run(args: &[String]) {
        let Some(lock) = lock_companion() else { return }; // already running
        let started_at_sign_in = is_sign_in_launch(args);
        let Some(settings) = read_settings() else { return };
        if !wanted(&settings) {
            return;
        }
        let options = || eframe::NativeOptions {
            // A hidden window: the notes are separate windows, the tray icon is the way in.
            viewport: egui::ViewportBuilder::default()
                .with_title("RustDo companion")
                .with_app_id("rustdo")
                .with_inner_size([1.0, 1.0])
                .with_decorations(false)
                .with_taskbar(false)
                .with_visible(false),
            ..Default::default()
        };
        log::info!(target: "rustdo", "companion starting (at sign-in: {started_at_sign_in})");
        // The lock and settings move into the app the first time it's created; a second
        // attempt (Direct3D fallback) gets fresh ones.
        let mut first = Some((settings, lock));
        let result = crate::diagnostics::run_native("RustDo companion", options, |cc| {
            let (settings, lock) = first.take().unwrap_or_else(|| {
                (read_settings().unwrap_or_default(), lock_companion().expect("companion lock"))
            });
            crate::i18n::set(settings.language);
            Box::new(Companion::new(cc, settings, started_at_sign_in, lock))
        });
        match result {
            Ok(()) => log::info!(target: "rustdo", "companion stopped"),
            Err(e) => log::error!(target: "rustdo", "companion couldn't start: {e}"),
        }
    }

    /// The settings saved by RustDo.
    fn read_settings() -> Option<Settings> {
        read_store().map(|s| s.settings)
    }

    /// The tasks and settings RustDo saved. The companion writes them back only for pinned
    /// lists' cards, and only while the main window is closed (see pinned.rs).
    fn read_store() -> Option<crate::model::Store> {
        let bytes = std::fs::read(crate::storage::data_path()).ok();
        match bytes {
            Some(b) => crate::storage::parse_backup(&b).ok(),
            None => Some(crate::model::Store::default()), // no tasks yet: defaults
        }
    }

    struct Companion {
        notes: SharedNotes,
        settings: Settings,
        started_at_sign_in: bool,
        tray: Option<TrayIcon>,
        /// What the tray icon was built for: the full companion (notes too) or reminders only.
        tray_full: bool,
        commands: Receiver<TrayCommand>,
        /// When to next ask the Store about an update (Store package only).
        #[cfg(windows)]
        next_update_check: Instant,
        command_tx: std::sync::mpsc::Sender<TrayCommand>,
        last_poll: Instant,
        settings_stamp: Option<std::time::SystemTime>,
        /// The tasks, for pinned lists' cards (reloaded when RustDo saves them).
        store: Option<crate::model::Store>,
        #[cfg(windows)]
        reminders: Option<std::thread::JoinHandle<()>>,
        _lock: File,
    }

    impl Companion {
        fn new(cc: &eframe::CreationContext<'_>, settings: Settings, started_at_sign_in: bool, lock: File) -> Self {
            // Same look as the main window (the notes' menus use the theme).
            cc.egui_ctx.set_theme(match settings.theme {
                crate::model::ThemeChoice::System => egui::ThemePreference::System,
                crate::model::ThemeChoice::Light => egui::ThemePreference::Light,
                crate::model::ThemeChoice::Dark => egui::ThemePreference::Dark,
            });
            let folder = crate::storage::data_path().parent().map(|p| p.to_path_buf());
            let (tx, rx) = channel();
            // Menu and icon clicks arrive on other threads: forward them and wake the loop.
            {
                let tx = tx.clone();
                let ctx = cc.egui_ctx.clone();
                MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                    let cmd = match e.id.0.as_str() {
                        "open" => TrayCommand::OpenApp,
                        "new" => TrayCommand::NewNote,
                        "show" => TrayCommand::ShowAll,
                        "hide" => TrayCommand::HideAll,
                        "quit" => TrayCommand::Quit,
                        _ => return,
                    };
                    let _ = tx.send(cmd);
                    ctx.request_repaint();
                }));
            }
            {
                let tx = tx.clone();
                let ctx = cc.egui_ctx.clone();
                TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        let _ = tx.send(TrayCommand::OpenApp);
                        ctx.request_repaint();
                    }
                }));
            }
            let mut me = Self {
                notes: notes_ui::Shared::load(folder),
                settings,
                started_at_sign_in,
                tray: None,
                tray_full: false,
                commands: rx,
                #[cfg(windows)]
                next_update_check: Instant::now() + Duration::from_secs(10 * 60),
                command_tx: tx,
                last_poll: Instant::now(),
                settings_stamp: None,
                store: None,
                #[cfg(windows)]
                reminders: None,
                _lock: lock,
            };
            me.sync_parts();
            me
        }

        fn full(&self) -> bool {
            full(&self.settings, self.started_at_sign_in)
        }

        /// Tray icon and reminders follow the settings. The icon is always there while the
        /// companion runs (with fewer items when it's only for reminders).
        fn sync_parts(&mut self) {
            let full = self.full();
            if self.tray.is_none() || self.tray_full != full {
                self.tray = None;
                self.tray = build_tray(full).map_err(|e| log::warn!(target: "rustdo", "tray icon: {e}")).ok();
                self.tray_full = full;
            }
            #[cfg(windows)]
            if self.settings.notifications && self.reminders.as_ref().is_none_or(|h| h.is_finished()) {
                self.reminders = Some(std::thread::spawn(crate::notify::reminder_loop));
            }
        }

        fn poll(&mut self, ctx: &egui::Context) {
            // Settings changed in RustDo?
            let stamp = std::fs::metadata(crate::storage::data_path()).and_then(|m| m.modified()).ok();
            if stamp != self.settings_stamp {
                self.settings_stamp = stamp;
                if let Some(s) = read_store() {
                    self.settings = s.settings.clone();
                    self.store = Some(s);
                }
            }
            // RustDo opened and asked a quiet companion to show itself.
            if take_request(WAKE_REQUEST) {
                self.started_at_sign_in = false;
            }
            // RustDo is quitting completely.
            if take_request(QUIT_COMPANION_REQUEST) {
                self.handle(ctx, TrayCommand::Quit);
                return;
            }
            #[cfg(windows)]
            self.step_aside_for_update(ctx);
            if !wanted(&self.settings) {
                notes_ui::lock(&self.notes).save_if_due(true);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.sync_parts();
            notes_ui::lock(&self.notes).reload_if_changed();
        }

        /// Store package: every few hours, while the main window is closed, install a
        /// waiting Store update (Windows closes and restarts us for it). Otherwise the
        /// running companion would hold the update back.
        #[cfg(windows)]
        fn step_aside_for_update(&mut self, _ctx: &egui::Context) {
            const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
            if !crate::store_package::is_packaged() || Instant::now() < self.next_update_check || app_is_open() {
                return;
            }
            self.next_update_check = Instant::now() + EVERY;
            notes_ui::lock(&self.notes).save_if_due(true);
            std::thread::spawn(|| match crate::store_package::install_waiting_update() {
                Ok(found) => log::info!(target: "rustdo", "Store update check: {}", if found { "update found" } else { "up to date" }),
                Err(e) => log::warn!(target: "rustdo", "Store update check failed: {e}"),
            });
        }

        /// Pinned lists: what was done on the cards goes to the main window if it's open
        /// (it owns the tasks then), else into the task file. Either way the cards show it
        /// at once.
        fn pinned_tick(&mut self, ctx: &egui::Context) {
            let (lists, actions) = {
                let mut s = notes_ui::lock(&self.notes);
                (s.notes.pinned_lists(), std::mem::take(&mut s.card_actions))
            };
            if lists.is_empty() && actions.is_empty() {
                return;
            }
            if self.store.is_none() {
                self.store = read_store();
            }
            let Some(store) = &mut self.store else { return };
            let now = chrono::Local::now().naive_local();
            if !actions.is_empty() && crate::pinned::apply(store, &actions, now) {
                if app_is_open() {
                    crate::pinned::handoff::send(&actions);
                } else {
                    let path = crate::storage::data_path();
                    match crate::storage::save(&path, store) {
                        Ok(()) => self.settings_stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
                        Err(e) => log::warn!(target: "rustdo", "pinned list: couldn't save the tasks: {e}"),
                    }
                }
            }
            let cards = crate::pinned::cards_from_store(store, lists, now.date());
            notes_ui::set_cards(ctx, &self.notes, cards);
        }

        fn handle(&mut self, ctx: &egui::Context, cmd: TrayCommand) {
            match cmd {
                TrayCommand::OpenApp => {
                    if app_is_open() {
                        request_show_app();
                    } else {
                        spawn(&[]);
                    }
                }
                TrayCommand::NewNote => {
                    let mut s = notes_ui::lock(&self.notes);
                    let near = s.notes.notes.iter().rev().find(|n| n.open).and_then(|n| n.pos);
                    s.new_note(near);
                    s.save_if_due(true);
                }
                TrayCommand::ShowAll => notes_ui::lock(&self.notes).set_all_open(true),
                TrayCommand::HideAll => notes_ui::lock(&self.notes).set_all_open(false),
                TrayCommand::Quit => {
                    // "Quit RustDo completely": the main window closes too.
                    if app_is_open() {
                        request(QUIT_APP_REQUEST);
                    }
                    notes_ui::lock(&self.notes).save_if_due(true);
                    self.tray = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    impl eframe::App for Companion {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            while let Ok(cmd) = self.commands.try_recv() {
                self.handle(ctx, cmd);
            }
            if self.last_poll.elapsed() >= POLL {
                self.last_poll = Instant::now();
                self.poll(ctx);
            }
            if self.full() {
                self.pinned_tick(ctx);
                notes_ui::show_windows(ctx, &self.notes);
            }
            let _ = &self.command_tx;
            ctx.request_repaint_after(POLL);
        }

        fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
            notes_ui::lock(&self.notes).save_if_due(true);
        }
    }

    /// The tray icon: `full` with the sticky-note items, else just for reminders.
    fn build_tray(full: bool) -> Result<TrayIcon, String> {
        let menu = Menu::new();
        let open = MenuItem::with_id("open", tr("Open RustDo"), true, None);
        let quit = MenuItem::with_id("quit", tr("Quit RustDo completely"), true, None);
        if full {
            let new = MenuItem::with_id("new", tr("New sticky note"), true, None);
            let show = MenuItem::with_id("show", tr("Show all notes"), true, None);
            let hide = MenuItem::with_id("hide", tr("Hide all notes"), true, None);
            menu.append_items(&[&open, &new, &PredefinedMenuItem::separator(), &show, &hide]).map_err(|e| e.to_string())?;
        } else {
            menu.append(&open).map_err(|e| e.to_string())?;
        }
        menu.append_items(&[&PredefinedMenuItem::separator(), &quit]).map_err(|e| e.to_string())?;
        let tooltip = if full { tr("RustDo: sticky notes and reminders") } else { tr("RustDo: reminders") };
        let png = eframe::icon_data::from_png_bytes(include_bytes!("../web/icons/icon-192.png")).map_err(|e| e.to_string())?;
        let icon = tray_icon::Icon::from_rgba(png.rgba, png.width, png.height).map_err(|e| e.to_string())?;
        TrayIconBuilder::new()
            .with_id("rustdo")
            .with_tooltip(tooltip)
            .with_title("RustDo")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(companion: bool, autostart: bool, notifications: bool) -> Settings {
        Settings { companion, companion_autostart: autostart, notifications, ..Settings::default() }
    }

    #[test]
    fn reminders_when_closed_can_be_turned_off() {
        // Sticky notes off and "Remind me even when RustDo is closed" off: nothing stays.
        let off = Settings { background_reminders: false, ..s(false, true, true) };
        assert!(!wanted(&off) && !starts_at_sign_in(&off));
    }

    #[test]
    fn launch_flags() {
        let args = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(is_companion_launch(&args(&["rustdo", "--companion"])));
        assert!(is_companion_launch(&args(&["rustdo", "--reminders"])), "pre-0.2.7 sign-in entries");
        assert!(!is_companion_launch(&args(&["rustdo"])));
        assert!(is_sign_in_launch(&args(&["rustdo", "--companion", "--at-sign-in"])));
        assert!(!is_sign_in_launch(&args(&["rustdo", "--companion"])));
    }

    #[test]
    fn what_runs_for_which_settings() {
        if !SUPPORTED {
            return;
        }
        // Defaults: companion on, starting at sign-in, with tray and notes.
        let d = Settings::default();
        assert!(wanted(&d) && starts_at_sign_in(&d) && full(&d, true) && full(&d, false));

        // Not at sign-in: still the full companion once RustDo is opened.
        let no_auto = s(true, false, false);
        assert!(!full(&no_auto, true));
        assert!(full(&no_auto, false));
        assert_eq!(starts_at_sign_in(&no_auto), false);

        // Companion off: no tray or notes. Windows keeps running for reminders.
        let off = s(false, true, true);
        assert!(!full(&off, false));
        assert_eq!(wanted(&off), cfg!(windows));
        assert_eq!(starts_at_sign_in(&off), cfg!(windows));

        // Everything off: nothing runs.
        let none = s(false, false, false);
        assert!(!wanted(&none) && !starts_at_sign_in(&none));
    }
}
