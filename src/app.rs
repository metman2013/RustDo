//! The egui user interface.

use crate::model::{
    relative_date, sort_tasks, PlannedBucket, Recurrence, SortMode, Step, Store, Task,
    ThemeChoice, View,
};
use crate::import::{self, Import};
use crate::notify;
use crate::platform;
use crate::storage::{self, Storage};
use crate::update::{self, CheckResult, Release};
use std::sync::mpsc::Receiver;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use eframe::egui::{
    self, text::LayoutJob, text::TextWrapping, Align, Align2, Color32, CursorIcon, FontId, Id,
    Key, Layout, Modifiers, Pos2, Rect, RichText, Sense, Stroke, TextFormat, TextStyle, Vec2,
};
use std::time::Duration as StdDuration;
use uuid::Uuid;
// std::time::Instant panics in the browser; web-time works everywhere.
use web_time::Instant;

/// An update found at launch is installed automatically only within this time, so a slow
/// download never restarts the app in the middle of someone's work.
const AUTO_UPDATE_WINDOW: StdDuration = StdDuration::from_secs(120);

/// Below this width (in points) the app switches to the one-panel phone layout.
const NARROW_WIDTH: f32 = 700.0;

/// Import is offered where we can pick a file: desktop and web, not the Android build.
const CAN_IMPORT: bool = cfg!(not(target_os = "android"));

const ACCENT: Color32 = Color32::from_rgb(70, 125, 230);
const DANGER: Color32 = Color32::from_rgb(215, 75, 75);
const STAR: Color32 = Color32::from_rgb(240, 170, 30);

enum Action {
    ToggleDone(Uuid),
    ToggleImportant(Uuid),
    ToggleMyDay(Uuid),
    Select(Uuid),
    Delete(Uuid),
}

enum AlertAction {
    Open,
    Complete,
    Snooze(i64),
    Dismiss,
}

struct PendingImport {
    file_name: String,
    import: Import,
    include_completed: bool,
}

struct Toast {
    text: String,
    undo: Option<(usize, Task)>,
    at: Instant,
}

pub struct TodoApp {
    /// Bigger, finger-friendly layout (phones, tablets, touch screens).
    touch_mode: bool,
    store: Store,
    storage: Storage,
    can_save: bool,
    /// Phone layout: showing the list picker instead of the task list.
    show_nav: bool,
    keyboard_shown: bool,
    update_rx: Option<Receiver<CheckResult>>,
    /// True when the user pressed "Check now" (so "up to date" and errors are reported).
    update_manual: bool,
    update_available: Option<Release>,
    update_dismissed: bool,
    install_rx: Option<Receiver<Result<std::path::PathBuf, String>>>,
    started: Instant,
    /// The launch-time update check has run (Windows auto-update).
    startup_checked: bool,
    /// The current download was started automatically at launch.
    auto_installing: bool,
    /// The user asked to install as soon as the download is ready.
    install_now: bool,
    /// A downloaded installer waiting to run.
    ready_installer: Option<std::path::PathBuf>,
    /// The upcoming reminders last handed to the OS (None = not yet this session).
    /// The reminder helper has been started (or stopped) for the current setting.
    helper_handled: bool,
    /// A RustDo backup waiting for the user to confirm "replace all my tasks".
    pending_restore: Option<(String, Store)>,
    /// Android: the "paste a backup" box, when open.
    paste_restore: Option<String>,
    backup_day: Option<NaiveDate>,
    last_backup: Option<Instant>,
    /// Data changed since the last automatic backup.
    backup_pending: bool,
    backup_error_shown: bool,
    export_nag_dismissed: bool,
    /// Android: space taken by the status bar, navigation bar and keyboard (points).
    insets: [f32; 4],
    insets_checked: Option<Instant>,
    // Pull-to-refresh state.
    pull: f32,
    pull_active: bool,
    refresh_requested: bool,
    list_scroll_y: f32,
    list_rect: Option<Rect>,
    /// The web file picker is async; the chosen file lands here and is picked up next frame.
    #[cfg(target_arch = "wasm32")]
    web_import: std::rc::Rc<std::cell::RefCell<Option<(String, Vec<u8>)>>>,
    dirty: bool,
    view: View,
    selected: Option<Uuid>,
    search: String,
    new_task: String,
    new_list: String,
    new_step: String,
    renaming_list: Option<(Uuid, String)>,
    rename_focus: bool,
    confirm_delete_list: Option<Uuid>,
    alerts: Vec<Uuid>,
    toast: Option<Toast>,
    focus_add: bool,
    focus_search: bool,
    pending_import: Option<PendingImport>,
}

impl TodoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (storage, moved_notice) = Storage::for_platform();
        let mut loaded = storage.load();
        // A load problem is more important to show than the "moved to Documents" notice.
        if loaded.warning.is_none() {
            loaded.warning = moved_notice;
        }
        apply_theme(&cc.egui_ctx, loaded.store.settings.theme);
        let touch_mode = platform::prefers_touch();
        apply_style(&cc.egui_ctx, touch_mode);
        Self {
            touch_mode,
            store: loaded.store,
            storage,
            can_save: loaded.can_save,
            show_nav: false,
            keyboard_shown: false,
            update_rx: None,
            update_manual: false,
            update_available: None,
            update_dismissed: false,
            install_rx: None,
            started: Instant::now(),
            startup_checked: false,
            auto_installing: false,
            install_now: false,
            ready_installer: None,
            helper_handled: false,
            pending_restore: None,
            paste_restore: None,
            backup_day: None,
            last_backup: None,
            backup_pending: false,
            backup_error_shown: false,
            export_nag_dismissed: false,
            insets: [0.0; 4],
            insets_checked: None,
            pull: 0.0,
            pull_active: false,
            refresh_requested: false,
            list_scroll_y: 0.0,
            list_rect: None,
            #[cfg(target_arch = "wasm32")]
            web_import: Default::default(),
            dirty: false,
            view: View::MyDay,
            selected: None,
            search: String::new(),
            new_task: String::new(),
            new_list: String::new(),
            new_step: String::new(),
            renaming_list: None,
            rename_focus: false,
            confirm_delete_list: None,
            alerts: Vec::new(),
            toast: loaded.warning.map(|text| Toast { text, undo: None, at: Instant::now() }),
            // Don't pop up the on-screen keyboard as soon as a phone opens the app.
            focus_add: !platform::IS_MOBILE_OR_WEB,
            focus_search: false,
            pending_import: None,
        }
    }

    /// Desktop: choose a RustDo backup to restore, starting in the Backups folder.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn pick_restore_file(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Restore RustDo backup")
            .add_filter("RustDo backup", &["json"]);
        if let Some(dir) = self.storage.folder() {
            let backups = dir.join("Backups");
            dialog = dialog.set_directory(if backups.is_dir() { backups } else { dir });
        }
        if let Some(path) = dialog.pick_file() {
            self.read_picked_file(&path);
        }
        let _ = ctx;
    }

    /// Web: the browser's file picker serves both import and restore.
    #[cfg(target_arch = "wasm32")]
    fn pick_restore_file(&mut self, ctx: &egui::Context) {
        self.pick_import_file(ctx);
    }

    #[cfg(target_os = "android")]
    fn pick_restore_file(&mut self, _ctx: &egui::Context) {}

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn read_picked_file(&mut self, path: &std::path::Path) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.load_import(&name, &bytes);
            }
            Err(e) => self.notify(format!("Couldn't read {}: {e}", path.display())),
        }
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn pick_import_file(&mut self, _ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .set_title("Import from Microsoft To Do")
            .add_filter("Microsoft To Do / Outlook export", &["json", "csv"])
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(path) = picked {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.load_import(&name, &bytes);
                }
                Err(e) => self.notify(format!("Couldn't read {}: {e}", path.display())),
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_import_file(&mut self, ctx: &egui::Context) {
        let slot = self.web_import.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("Microsoft To Do / Outlook export", &["json", "csv"])
                .pick_file()
                .await;
            if let Some(file) = file {
                let bytes = file.read().await;
                *slot.borrow_mut() = Some((file.file_name(), bytes));
                ctx.request_repaint();
            }
        });
    }

    #[cfg(target_os = "android")]
    fn pick_import_file(&mut self, _ctx: &egui::Context) {}

    fn load_import(&mut self, file_name: &str, bytes: &[u8]) {
        // A RustDo backup is restored (after confirmation) rather than merged.
        if let Ok(store) = storage::parse_backup(bytes) {
            self.pending_restore = Some((file_name.to_string(), store));
            return;
        }
        match import::parse_bytes(file_name, bytes) {
            Ok(import) => {
                self.pending_import = Some(PendingImport {
                    file_name: file_name.to_string(),
                    import,
                    include_completed: false,
                });
            }
            Err(e) => self.notify(e),
        }
    }

    // ------------------------------------------------------------------ updates

    /// Whether this copy can update itself silently at startup: the installed Windows app
    /// (a portable exe is left alone; it only shows the banner).
    fn can_auto_update(&self) -> bool {
        cfg!(all(windows, feature = "updates")) && self.store.settings.auto_update && update::is_installed_copy()
    }

    /// Starts an update check: at launch when auto-update is on (Windows), otherwise at
    /// most once a day (desktop and Android; the web app updates itself).
    fn maybe_check_updates(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        if cfg!(target_arch = "wasm32") || self.update_rx.is_some() || self.update_available.is_some() {
            return;
        }
        let at_launch = self.can_auto_update() && !self.startup_checked;
        let due = self
            .store
            .settings
            .last_update_check
            .is_none_or(|last| now - last >= Duration::hours(24) || last > now);
        if self.update_manual || at_launch || (self.store.settings.check_updates && due) {
            self.startup_checked = true;
            self.store.settings.last_update_check = Some(now);
            self.dirty = true;
            self.update_rx = Some(update::check_in_background(ctx.clone()));
        }
    }

    fn poll_updates(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.update_rx {
            if let Ok(result) = rx.try_recv() {
                self.update_rx = None;
                let manual = std::mem::take(&mut self.update_manual);
                match result {
                    CheckResult::Available(release) => {
                        self.update_dismissed = false;
                        // Found right after launch: update straight away, no questions.
                        let auto = self.can_auto_update()
                            && self.started.elapsed() < AUTO_UPDATE_WINDOW
                            && release.download_url.is_some();
                        let version = release.version.clone();
                        self.update_available = Some(release);
                        if auto {
                            self.auto_installing = true;
                            self.install_update(ctx);
                            self.notify(format!("Updating RustDo to {version}… it will reopen in a moment."));
                        }
                    }
                    CheckResult::UpToDate if manual => {
                        self.notify(format!("You're up to date (RustDo {}).", update::CURRENT_VERSION))
                    }
                    CheckResult::Failed(e) if manual => self.notify(format!("Couldn't check for updates: {e}")),
                    _ => {}
                }
            }
        }
        if let Some(rx) = &self.install_rx {
            if let Ok(result) = rx.try_recv() {
                self.install_rx = None;
                match result {
                    Ok(installer) => {
                        self.ready_installer = Some(installer);
                        // An automatic update that took too long to download becomes a
                        // "Restart to update" banner instead of restarting out of the blue.
                        if self.auto_installing && self.started.elapsed() >= AUTO_UPDATE_WINDOW {
                            self.auto_installing = false;
                            self.update_dismissed = false;
                        }
                    }
                    Err(e) => {
                        self.auto_installing = false;
                        self.notify(format!("Couldn't download the update: {e}"));
                    }
                }
            }
        }
        // Manual "Update & restart": install now. Automatic: wait until you're not typing.
        if self.ready_installer.is_some() && !self.auto_installing_waiting(ctx) && (self.auto_installing || self.install_now) {
            self.launch_installer(ctx);
        }
    }

    /// An automatic update holds off while there's half-typed text that isn't saved yet
    /// (edits to existing tasks save instantly) or a dialog is open.
    fn auto_installing_waiting(&self, _ctx: &egui::Context) -> bool {
        self.auto_installing
            && (!self.new_task.trim().is_empty()
                || !self.new_step.trim().is_empty()
                || !self.new_list.trim().is_empty()
                || self.renaming_list.is_some()
                || self.paste_restore.is_some()
                || self.pending_import.is_some()
                || self.pending_restore.is_some()
                || self.confirm_delete_list.is_some())
    }

    /// Runs the downloaded installer silently (no wizard; it reopens RustDo when done).
    fn launch_installer(&mut self, ctx: &egui::Context) {
        let Some(installer) = self.ready_installer.take() else { return };
        self.install_now = false;
        // Test hook (debug builds only): stop right before installing.
        if cfg!(debug_assertions) && std::env::var_os("RUSTDO_TEST_NO_INSTALL").is_some() {
            self.auto_installing = false;
            self.notify(format!("TEST: would now run {} silently", installer.display()));
            return;
        }
        match std::process::Command::new(&installer)
            .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"])
            .spawn()
        {
            Ok(_) => {
                // The installer replaces this program, so get out of its way.
                self.persist();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => {
                self.auto_installing = false;
                self.notify(format!("Couldn't start the installer: {e}"));
            }
        }
    }

    fn install_update(&mut self, ctx: &egui::Context) {
        // Already downloaded (e.g. "Restart to update"): install it right away.
        if self.ready_installer.is_some() {
            self.install_now = true;
            return;
        }
        if !self.auto_installing {
            self.install_now = true;
        }
        let Some(release) = self.update_available.clone() else { return };
        #[cfg(all(windows, feature = "updates"))]
        if let (Some(url), Some(name)) = (release.download_url.clone(), release.download_name.clone()) {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(update::download_installer(&url, &name));
                ctx2.request_repaint();
            });
            self.install_rx = Some(rx);
            self.notify("Downloading the update…");
            return;
        }
        // Android (APK) and anything without an installer: open the download in the browser.
        let url = release.download_url.unwrap_or(release.notes_url);
        ctx.open_url(egui::OpenUrl::new_tab(url));
    }

    fn update_banner(&mut self, ui: &mut egui::Ui) {
        if self.update_dismissed {
            return;
        }
        let Some(release) = self.update_available.clone() else { return };
        let mut install = false;
        egui::Frame::group(ui.style())
            .fill(ACCENT.gamma_multiply(0.18))
            .stroke(Stroke::new(1.0_f32, ACCENT))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("RustDo {} is available.", release.version)).strong());
                    let busy = self.install_rx.is_some() || (self.auto_installing && self.ready_installer.is_some());
                    let label = if self.auto_installing && self.ready_installer.is_some() {
                        "Updating when you finish typing…"
                    } else if busy {
                        "Downloading…"
                    } else if self.ready_installer.is_some() {
                        "Restart to update"
                    } else if cfg!(windows) {
                        "Update & restart"
                    } else {
                        "Update now"
                    };
                    if ui
                        .add_enabled(!busy, egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(ACCENT))
                        .clicked()
                    {
                        install = true;
                    }
                    ui.hyperlink_to("What's new", &release.notes_url);
                    if ui.button("Later").clicked() {
                        self.update_dismissed = true;
                    }
                });
            });
        ui.add_space(6.0);
        if install {
            let ctx = ui.ctx().clone();
            self.install_update(&ctx);
        }
    }

    fn settings_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("⚙", |ui| {
            // A fixed width: wrapping text in an auto-sized menu never settles on a size.
            ui.set_width(300.0);
            ui.label(RichText::new(format!("RustDo {}", update::CURRENT_VERSION)).strong());
            ui.separator();
            if cfg!(target_arch = "wasm32") {
                ui.label(RichText::new("The web app updates itself automatically.").weak());
            } else {
                if ui.checkbox(&mut self.store.settings.check_updates, "Check for updates daily").changed() {
                    self.dirty = true;
                }
                if cfg!(windows) {
                    let installed = update::is_installed_copy();
                    let r = ui.add_enabled(
                        installed,
                        egui::Checkbox::new(&mut self.store.settings.auto_update, "Install updates automatically"),
                    );
                    if r.changed() {
                        self.dirty = true;
                    }
                    let note = if installed {
                        "Checks each time RustDo opens and updates itself silently, then reopens."
                    } else {
                        "Only for the installed version (this is the portable exe)."
                    };
                    ui.label(RichText::new(note).weak().small());
                }
                ui.label(
                    RichText::new("Asks GitHub for the latest version. No task data is sent.")
                        .weak()
                        .small(),
                );
                let checking = self.update_rx.is_some();
                if ui.add_enabled(!checking, egui::Button::new("Check now")).clicked() {
                    self.update_manual = true;
                    self.update_available = None;
                    ui.close_menu();
                }
            }
            ui.separator();
            self.notification_menu_items(ui);
            self.backup_menu_items(ui);
            #[cfg(target_os = "android")]
            {
                ui.separator();
                let (jni, content) = platform::android_inset_sources();
                let fmt = |v: Option<[i32; 4]>| {
                    v.map_or("none".to_string(), |[l, t, r, b]| format!("{t} top, {b} bottom, {l}/{r} sides"))
                };
                ui.label(
                    RichText::new(format!("Screen edges (px): system {} · window {}", fmt(jni), fmt(content)))
                        .weak()
                        .small(),
                );
            }
            ui.separator();
            // The full Releases page: newest version first, with every earlier one below.
            let releases = format!("https://github.com/{}/releases", update::REPO);
            if cfg!(target_arch = "wasm32") {
                ui.hyperlink_to("⬇ Get the desktop & mobile apps", releases)
                    .on_hover_text("Windows, Linux and Android downloads");
            } else {
                ui.hyperlink_to("Downloads & release notes", releases);
            }
            ui.hyperlink_to("Join the RustDo Discord", update::DISCORD)
                .on_hover_text("Chat, ideas and help: opens in your browser");
            ui.hyperlink_to("Project page", format!("https://github.com/{}", update::REPO));
        });
    }

    /// Windows: starts the background reminder helper shortly after launch (and again
    /// whenever the setting changes), so reminders still pop up after RustDo is closed.
    fn maybe_start_helper(&mut self) {
        if self.helper_handled || self.started.elapsed() < StdDuration::from_secs(1) || !self.can_save {
            return;
        }
        self.helper_handled = true;
        // Make sure the helper reads the current setting from disk before it starts.
        if self.dirty {
            self.persist();
        }
        let enabled = self.store.settings.notifications;
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || notify::ensure_helper(enabled));
        #[cfg(target_arch = "wasm32")]
        let _ = enabled;
    }

    fn notification_menu_items(&mut self, ui: &mut egui::Ui) {
        if cfg!(target_os = "android") {
            return;
        }
        ui.label(RichText::new("Notifications").strong());
        if ui
            .checkbox(&mut self.store.settings.notifications, "Notify me about reminders")
            .changed()
        {
            self.dirty = true;
            self.helper_handled = false; // start or stop the Windows reminder helper
        }
        let note = if cfg!(windows) {
            "Windows shows them at the reminder time, even after you close RustDo (a small background helper starts with Windows)."
        } else if cfg!(target_arch = "wasm32") {
            "Shown while this app is open (browsers can't notify a closed web page)."
        } else {
            "Shown while RustDo is running (minimized is fine)."
        };
        ui.label(RichText::new(note).weak().small());
        if cfg!(target_arch = "wasm32") && self.store.settings.notifications {
            match notify::web_permission() {
                "default" => {
                    if ui.button("Allow notifications").clicked() {
                        notify::web_request_permission();
                    }
                }
                "denied" => {
                    ui.label(RichText::new("Notifications are blocked in this browser's site settings.").color(DANGER).small());
                }
                "unsupported" => {
                    ui.label(RichText::new("This browser doesn't support notifications.").weak().small());
                }
                _ => {}
            }
        }
        ui.separator();
    }

    fn backup_menu_items(&mut self, ui: &mut egui::Ui) {
        let now = Local::now().naive_local();
        ui.label(RichText::new("Backups").strong());
        let where_text = if cfg!(target_arch = "wasm32") {
            "A backup is saved automatically each day in this browser (last 7 days).".to_string()
        } else if cfg!(target_os = "android") {
            "A backup is saved automatically each day on this phone (last 14 days). Uninstalling the app removes them.".to_string()
        } else {
            format!(
                "A backup is saved automatically each day in the Backups folder (last {} days).",
                storage::KEEP_BACKUPS
            )
        };
        ui.add(egui::Label::new(RichText::new(where_text).weak().small()).wrap())
            .on_hover_text(self.storage.backups_location());
        if let Some(last) = self.store.settings.last_export {
            let when = relative_date(last.date(), now.date()).to_lowercase();
            ui.label(RichText::new(format!("Last exported: {when}")).weak().small());
        }

        let export_label = if cfg!(target_os = "android") { "Copy backup to clipboard" } else { "Export backup…" };
        if ui.button(export_label).clicked() {
            ui.close_menu();
            let ctx = ui.ctx().clone();
            self.export_backup(&ctx, now);
        }
        if cfg!(target_os = "android") {
            if ui.button("Restore from pasted backup…").clicked() {
                ui.close_menu();
                self.paste_restore = Some(String::new());
            }
        } else if ui.button("Restore from backup…").clicked() {
            ui.close_menu();
            let ctx = ui.ctx().clone();
            self.pick_restore_file(&ctx);
        }

        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        {
            if let Some(dir) = self.storage.folder() {
                if ui
                    .button("Open RustDo folder")
                    .on_hover_text(format!("{}\nYour tasks (tasks.json) and the Backups folder", dir.display()))
                    .clicked()
                {
                    ui.close_menu();
                    platform::open_folder(&dir);
                }
            }
            match self.store.settings.backup_dir.clone() {
                Some(extra) => {
                    ui.add(egui::Label::new(RichText::new(format!("Also backing up to {extra}")).small()).wrap());
                    ui.horizontal(|ui| {
                        if small_btn(ui, "Change…").clicked() {
                            ui.close_menu();
                            self.choose_backup_folder(now.date());
                        }
                        if small_btn(ui, "Stop").clicked() {
                            self.store.settings.backup_dir = None;
                            self.dirty = true;
                            ui.close_menu();
                        }
                    });
                }
                None => {
                    if ui
                        .button("Also back up to a folder…")
                        .on_hover_text("For example a OneDrive or USB folder, so a copy lives off this PC.")
                        .clicked()
                    {
                        ui.close_menu();
                        self.choose_backup_folder(now.date());
                    }
                }
            }
        }
    }

    /// Files dropped onto the window (desktop gives a path, the browser gives the bytes).
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.first().cloned());
        let Some(file) = dropped else { return };
        if let Some(bytes) = &file.bytes {
            self.load_import(&file.name, bytes);
            return;
        }
        if let Some(path) = &file.path {
            match std::fs::read(path) {
                Ok(bytes) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.load_import(&name, &bytes);
                }
                Err(e) => self.notify(format!("Couldn't read {}: {e}", path.display())),
            }
        }
    }

    fn import_dialog(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let Some(pending) = &mut self.pending_import else { return };
        let mut confirm = false;
        let mut cancel = false;
        let existing: Vec<String> = self.store.lists.iter().map(|l| l.name.to_lowercase()).collect();
        egui::Window::new("Import tasks")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(480.0);
                let imp = &pending.import;
                ui.label(format!(
                    "{} from {}: {} list{}, {} task{} ({} completed).",
                    imp.source,
                    pending.file_name,
                    imp.lists.len(),
                    if imp.lists.len() == 1 { "" } else { "s" },
                    imp.task_count(),
                    if imp.task_count() == 1 { "" } else { "s" },
                    imp.completed_count(),
                ));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    egui::Grid::new("import_lists").striped(true).num_columns(3).show(ui, |ui| {
                        ui.label(RichText::new("List").strong());
                        ui.label(RichText::new("Open").strong());
                        ui.label(RichText::new("").strong());
                        ui.end_row();
                        for l in &imp.lists {
                            let open = l.tasks.iter().filter(|t| !t.done).count();
                            ui.label(&l.name);
                            ui.label(open.to_string());
                            let note = if l.is_default {
                                "into your Tasks list"
                            } else if existing.contains(&l.name.trim().to_lowercase()) {
                                "merged with existing list"
                            } else {
                                "new list"
                            };
                            ui.label(RichText::new(note).weak());
                            ui.end_row();
                        }
                    });
                });
                if !imp.warnings.is_empty() {
                    ui.add_space(4.0);
                    egui::CollapsingHeader::new(format!("⚠ {} note(s)", imp.warnings.len())).show(ui, |ui| {
                        for w in &imp.warnings {
                            ui.add(egui::Label::new(RichText::new(w).weak()).wrap());
                        }
                    });
                }
                ui.add_space(6.0);
                ui.checkbox(&mut pending.include_completed, format!("Also import {} completed tasks", imp.completed_count()));
                ui.label(
                    RichText::new("Tasks you've already imported are skipped. A backup of your current tasks is saved first.")
                        .weak()
                        .small(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Import").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            self.pending_import = None;
        } else if confirm {
            let pending = self.pending_import.take().unwrap();
            if self.can_save {
                if let Err(e) = self.storage.snapshot(&self.store, "before-import") {
                    self.notify(format!("Import cancelled: couldn't save a backup first ({e})."));
                    return;
                }
            }
            let report = import::apply(&mut self.store, &pending.import, pending.include_completed, now);
            self.dirty = true;
            self.view = View::All;
            self.search.clear();
            self.notify(report.summary());
        }
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast { text: text.into(), undo: None, at: Instant::now() });
    }

    fn persist(&mut self) {
        self.dirty = false;
        if !self.can_save {
            return;
        }
        match self.storage.save(&self.store) {
            Ok(()) => self.backup_pending = true,
            Err(e) => self.notify(format!("Couldn't save: {e}")),
        }
    }

    // ------------------------------------------------------------------ backups

    /// Keeps today's automatic backup current: written at startup, on the first change of
    /// each day, and then at most every 5 minutes while you work.
    fn maybe_auto_backup(&mut self, today: NaiveDate) {
        if !self.can_save {
            return;
        }
        let new_day = self.backup_day != Some(today);
        let due = self.backup_pending
            && self.last_backup.is_none_or(|t| t.elapsed() >= StdDuration::from_secs(5 * 60));
        if !(new_day || due) {
            return;
        }
        self.backup_day = Some(today);
        self.last_backup = Some(Instant::now());
        self.backup_pending = false;
        if let Err(e) = self.storage.auto_backup(&self.store, today) {
            if !self.backup_error_shown {
                self.backup_error_shown = true;
                self.notify(format!("Automatic backup failed: {e}"));
            }
        }
    }

    fn export_backup(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let json = storage::export_json(&self.store);
        let name = storage::backup_file_name(now.date());
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        {
            let _ = ctx;
            // Start in the folder used last time, else the RustDo folder in Documents.
            let start_dir = self
                .store
                .settings
                .last_export_dir
                .clone()
                .map(std::path::PathBuf::from)
                .filter(|d| d.is_dir())
                .or_else(|| self.storage.folder());
            let mut dialog = rfd::FileDialog::new()
                .set_title("Export RustDo backup")
                .set_file_name(&name)
                .add_filter("RustDo backup", &["json"]);
            if let Some(dir) = start_dir {
                dialog = dialog.set_directory(dir);
            }
            let Some(path) = dialog.save_file() else {
                return;
            };
            match std::fs::write(&path, json) {
                Ok(()) => {
                    self.store.settings.last_export_dir = path.parent().map(|d| d.display().to_string());
                    self.notify(format!("Backup saved to {}", path.display()));
                }
                Err(e) => {
                    self.notify(format!("Couldn't save the backup: {e}"));
                    return;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            if let Err(e) = platform::download_text(&name, &json) {
                self.notify(format!("Couldn't export: {e}"));
                return;
            }
            self.notify(format!("Downloaded {name}. Keep it somewhere safe."));
        }
        #[cfg(target_os = "android")]
        {
            let _ = name;
            ctx.copy_text(json);
            self.notify("Backup copied. Paste it into a note or email to keep it safe.");
        }
        self.store.settings.last_export = Some(now);
        self.dirty = true;
    }

    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    fn choose_backup_folder(&mut self, today: NaiveDate) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Also back up to this folder").pick_folder() {
            self.store.settings.backup_dir = Some(dir.display().to_string());
            self.dirty = true;
            // Write a backup there straight away so the user sees it works.
            self.backup_day = None;
            self.backup_error_shown = false;
            self.persist();
            self.maybe_auto_backup(today);
            if !self.backup_error_shown {
                self.notify(format!("Daily backups will also go to {}", dir.display()));
            }
        }
    }

    fn restore_dialog(&mut self, ctx: &egui::Context) {
        // Android: paste a backup that was exported to the clipboard.
        if let Some(text) = &mut self.paste_restore {
            let mut restore = false;
            let mut cancel = false;
            egui::Window::new("Restore from a backup")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_max_width(420.0);
                    ui.label("Paste the backup text you saved earlier:");
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(text).desired_rows(6).desired_width(f32::INFINITY));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Continue").clicked() {
                            restore = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            if restore {
                let text = self.paste_restore.take().unwrap_or_default();
                match storage::parse_backup(text.as_bytes()) {
                    Ok(store) => self.pending_restore = Some(("pasted backup".into(), store)),
                    Err(e) => self.notify(e),
                }
            } else if cancel {
                self.paste_restore = None;
            }
        }

        let Some((name, backup)) = &self.pending_restore else { return };
        let mut confirm = false;
        let mut cancel = false;
        let open = backup.tasks.iter().filter(|t| !t.done).count();
        let newest = backup.tasks.iter().map(|t| t.created).max();
        egui::Window::new("Restore backup?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(format!(
                    "{name}: {} lists, {} tasks ({open} open).",
                    backup.lists.len(),
                    backup.tasks.len()
                ));
                if let Some(newest) = newest {
                    ui.label(RichText::new(format!("Newest task created {}.", newest.format("%b %-d, %Y"))).weak());
                }
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        "This replaces all your current tasks and lists with the backup. \
                         Your current tasks are saved first, so this can be undone.",
                    )
                    .wrap(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Replace my tasks").color(Color32::WHITE)).fill(DANGER))
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if cancel {
            self.pending_restore = None;
        } else if confirm {
            let (name, mut backup) = self.pending_restore.take().unwrap();
            if self.can_save {
                if let Err(e) = self.storage.snapshot(&self.store, "before-restore") {
                    self.notify(format!("Restore cancelled: couldn't save your current tasks first ({e})."));
                    return;
                }
            }
            // Keep this device's preferences (theme, update checks, backup folder).
            backup.settings = self.store.settings.clone();
            self.store = backup;
            self.selected = None;
            self.alerts.clear();
            self.view = View::MyDay;
            self.search.clear();
            self.dirty = true;
            self.notify(format!("Restored from {name}."));
        }
    }

    /// Web only: browsers may clear stored data, so nudge towards a manual export.
    fn export_nag(&mut self, ui: &mut egui::Ui, now: NaiveDateTime) {
        if !cfg!(target_arch = "wasm32") || self.export_nag_dismissed || self.store.tasks.len() < 5 {
            return;
        }
        let stale = self.store.settings.last_export.is_none_or(|t| now - t > Duration::days(30));
        if !stale {
            return;
        }
        let mut export = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label("Your tasks are stored in this browser only. Keep a backup in case browser data is cleared.");
                if ui.button("Export backup").clicked() {
                    export = true;
                }
                if ui.button("Not now").clicked() {
                    self.export_nag_dismissed = true;
                }
            });
        });
        ui.add_space(6.0);
        if export {
            let ctx = ui.ctx().clone();
            self.export_backup(&ctx, now);
        }
    }

    fn delete_task(&mut self, id: Uuid) {
        if let Some((idx, task)) = self.store.remove_task(id) {
            if self.selected == Some(id) {
                self.selected = None;
            }
            self.alerts.retain(|a| *a != id);
            self.toast = Some(Toast {
                text: format!("Deleted “{}”", task.title),
                undo: Some((idx, task)),
                at: Instant::now(),
            });
            self.dirty = true;
        }
    }

    fn apply(&mut self, action: Action, now: NaiveDateTime) {
        match action {
            Action::ToggleDone(id) => {
                if let Some(done) = self.store.task(id).map(|t| !t.done) {
                    if self.store.set_done(id, done, now).is_some() {
                        self.notify("Nice! The next occurrence has been scheduled.");
                    }
                    self.dirty = true;
                }
            }
            Action::ToggleImportant(id) => {
                if let Some(t) = self.store.task_mut(id) {
                    t.important = !t.important;
                    self.dirty = true;
                }
            }
            Action::ToggleMyDay(id) => {
                let today = now.date();
                if let Some(t) = self.store.task_mut(id) {
                    t.my_day = if t.in_my_day(today) { None } else { Some(today) };
                    self.dirty = true;
                }
            }
            Action::Select(id) => {
                self.selected = if self.selected == Some(id) { None } else { Some(id) };
                self.new_step.clear();
            }
            Action::Delete(id) => self.delete_task(id),
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (new, find, esc, del) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::N),
                i.consume_key(Modifiers::COMMAND, Key::F),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Delete),
            )
        });
        if new {
            self.search.clear();
            if self.view == View::Completed {
                self.view = View::MyDay;
            }
            self.focus_add = true;
        }
        if find {
            self.focus_search = true;
        }
        if esc {
            if !self.search.is_empty() {
                self.search.clear();
            } else {
                self.selected = None;
            }
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        if del && !typing {
            if let Some(id) = self.selected {
                self.delete_task(id);
            }
        }
    }

    // ------------------------------------------------------------------ sidebar

    /// The list picker: a left side panel on wide screens, the whole screen on phones.
    fn sidebar(&mut self, ctx: &egui::Context, today: NaiveDate, narrow: bool) {
        let body = |ui: &mut egui::Ui| {
                ui.add_space(10.0);
                let touch = is_touch(ui);
                ui.horizontal(|ui| {
                    let back = egui::Button::new(RichText::new("<").size(if touch { 26.0 } else { 18.0 })).frame(false);
                    let back = if touch { back.min_size(Vec2::splat(44.0)) } else { back };
                    if narrow && ui.add(back).clicked() {
                        self.show_nav = false;
                    }
                    ui.label(RichText::new("✔ RustDo").size(20.0).strong().color(ACCENT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let theme = self.store.settings.theme;
                        let (icon, next, tip) = match theme {
                            ThemeChoice::System => ("🖥", ThemeChoice::Light, "Theme: system"),
                            ThemeChoice::Light => ("☀", ThemeChoice::Dark, "Theme: light"),
                            ThemeChoice::Dark => ("🌙", ThemeChoice::System, "Theme: dark"),
                        };
                        self.settings_menu(ui);
                        if ui.button(icon).on_hover_text(tip).clicked() {
                            self.store.settings.theme = next;
                            apply_theme(ui.ctx(), next);
                            self.dirty = true;
                        }
                    });
                });
                ui.add_space(6.0);

                let min = field_height(ui);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(if touch { "🔍 Search" } else { "🔍 Search  (Ctrl+F)" })
                        .min_size(min)
                        .vertical_align(Align::Center)
                        .desired_width(f32::INFINITY),
                );
                if self.focus_search {
                    r.request_focus();
                    self.focus_search = false;
                }
                // On a phone, pressing Enter in the search box shows the results.
                if narrow && r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.search.trim().is_empty() {
                    self.show_nav = false;
                }
                ui.add_space(6.0);

                let searching = !self.search.trim().is_empty();
                let smart = [
                    (View::MyDay, "☀", "My Day"),
                    (View::Important, "★", "Important"),
                    (View::Planned, "📅", "Planned"),
                    (View::All, "☰", "All"),
                    (View::Completed, "✔", "Completed"),
                ];
                for (view, icon, name) in smart {
                    let count = if view == View::Completed {
                        0
                    } else {
                        self.store.open_count(view, today)
                    };
                    let selected = self.view == view && !searching;
                    if nav_row(ui, selected, icon, name, count).clicked() {
                        self.view = view;
                        self.search.clear();
                        self.show_nav = false;
                    }
                }
                ui.separator();

                let lists: Vec<(Uuid, String)> =
                    self.store.lists.iter().map(|l| (l.id, l.name.clone())).collect();
                // Room below the lists for "+ New list" (and Import), sized for the layout.
                let reserved = match (CAN_IMPORT, touch) {
                    (true, true) => 140.0,
                    (false, true) => 80.0,
                    (true, false) => 86.0,
                    (false, false) => 52.0,
                };
                // Phone layout: the whole screen scrolls as one page, so show every list.
                let max_h = if narrow { f32::INFINITY } else { (ui.available_height() - reserved).max(60.0) };
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (id, name) in lists {
                            let is_default = id == self.store.default_list;
                            if self.renaming_list.as_ref().is_some_and(|(rid, _)| *rid == id) {
                                let (_, buf) = self.renaming_list.as_mut().unwrap();
                                let r = ui.add(
                                    egui::TextEdit::singleline(buf).desired_width(f32::INFINITY),
                                );
                                if self.rename_focus {
                                    r.request_focus();
                                    self.rename_focus = false;
                                }
                                if r.lost_focus() {
                                    let (rid, buf) = self.renaming_list.take().unwrap();
                                    self.dirty |= self.store.rename_list(rid, &buf);
                                }
                                continue;
                            }
                            let count = self.store.open_count(View::List(id), today);
                            let icon = if is_default { "🏠" } else { "☰" };
                            let selected = self.view == View::List(id) && !searching;
                            let r = nav_row(ui, selected, icon, &name, count);
                            if r.clicked() {
                                self.view = View::List(id);
                                self.search.clear();
                                self.show_nav = false;
                            }
                            if r.double_clicked() {
                                self.renaming_list = Some((id, name.clone()));
                                self.rename_focus = true;
                            }
                            r.context_menu(|ui| {
                                if ui.button("✏  Rename list").clicked() {
                                    self.renaming_list = Some((id, name.clone()));
                                    self.rename_focus = true;
                                    ui.close_menu();
                                }
                                if !is_default && ui.button("🗑  Delete list").clicked() {
                                    self.confirm_delete_list = Some(id);
                                    ui.close_menu();
                                }
                            });
                        }
                    });

                ui.separator();
                let min = field_height(ui);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.new_list)
                        .hint_text("+ New list")
                        .min_size(min)
                        .vertical_align(Align::Center)
                        .desired_width(f32::INFINITY),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    if let Some(id) = self.store.add_list(&self.new_list) {
                        self.new_list.clear();
                        self.view = View::List(id);
                        self.search.clear();
                        self.show_nav = false;
                        self.focus_add = !platform::IS_MOBILE_OR_WEB;
                        self.dirty = true;
                    }
                }
                ui.add_space(2.0);
                if CAN_IMPORT
                    && ui
                        .add_sized([ui.available_width(), 26.0], egui::Button::new("⬇  Import from Microsoft To Do…"))
                        .on_hover_text("Import a Microsoft To Do export (.json) or an Outlook tasks export (.csv).\nYou can also drop the file onto this window.")
                        .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.pick_import_file(&ctx);
                }
        };
        if narrow {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("lists_page")
                    .auto_shrink([false; 2])
                    .show(ui, body);
            });
        } else {
            egui::SidePanel::left("sidebar")
                .resizable(true)
                .default_width(250.0)
                .width_range(200.0..=400.0)
                .show(ctx, body);
        }
    }

    // ------------------------------------------------------------------ task list

    fn main_panel(&mut self, ctx: &egui::Context, now: NaiveDateTime, narrow: bool) {
        let today = now.date();
        let mut actions = Vec::new();
        egui::CentralPanel::default().show(ctx, |ui| {
            let query = self.search.trim().to_lowercase();
            let searching = !query.is_empty();
            let (icon, title) = if searching {
                ("🔍", format!("Searching for “{}”", self.search.trim()))
            } else {
                match self.view {
                    View::MyDay => ("☀", "My Day".to_string()),
                    View::Important => ("★", "Important".to_string()),
                    View::Planned => ("📅", "Planned".to_string()),
                    View::All => ("☰", "All".to_string()),
                    View::Completed => ("✔", "Completed".to_string()),
                    View::List(id) => ("☰", self.store.list_name(id).to_string()),
                }
            };

            ui.add_space(10.0);
            self.update_banner(ui);
            self.export_nag(ui, now);
            let touch = is_touch(ui);
            ui.horizontal(|ui| {
                let menu = egui::Button::new(RichText::new("☰").size(if touch { 26.0 } else { 20.0 })).frame(false);
                let menu = if touch { menu.min_size(Vec2::splat(44.0)) } else { menu };
                if narrow && ui.add(menu).on_hover_text("Lists").clicked() {
                    self.show_nav = true;
                }
                let size = if narrow { 20.0 } else { 24.0 };
                ui.label(RichText::new(format!("{icon}  {title}")).size(size).strong());
                // Planned is always grouped by date, so sorting doesn't apply there.
                if self.view == View::Planned && !searching {
                    return;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut sort = self.store.settings.sort;
                    egui::ComboBox::from_id_salt("sort")
                        .width(if narrow { 64.0 } else { 160.0 })
                        .selected_text(if narrow { "Sort".to_string() } else { format!("Sort: {}", sort.label()) })
                        .show_ui(ui, |ui| {
                            for m in SortMode::ALL {
                                ui.selectable_value(&mut sort, m, m.label());
                            }
                        });
                    if sort != self.store.settings.sort {
                        self.store.settings.sort = sort;
                        self.dirty = true;
                    }
                });
            });
            if self.view == View::MyDay && !searching {
                ui.label(RichText::new(today.format("%A, %B %-d").to_string()).weak());
            }
            ui.add_space(8.0);

            if !searching && self.view != View::Completed {
                let min = field_height(ui);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.new_task)
                        .hint_text(if platform::IS_MOBILE_OR_WEB || narrow || touch {
                            "+  Add a task"
                        } else {
                            "+  Add a task   (Enter to save, Ctrl+N)"
                        })
                        .font(FontId::proportional(if touch { 17.0 } else { 15.0 }))
                        .min_size(min)
                        .vertical_align(Align::Center)
                        .desired_width(f32::INFINITY),
                );
                if self.focus_add {
                    r.request_focus();
                    self.focus_add = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    if self.store.add_task(&self.new_task, self.view, now).is_some() {
                        self.new_task.clear();
                        self.dirty = true;
                    }
                    r.request_focus();
                }
                ui.add_space(8.0);
            }

            self.pull_to_refresh(ui);

            let store = &self.store;
            let view = self.view;
            let selected = self.selected;
            let sort = store.settings.sort;
            let show_list = searching || !matches!(view, View::List(_));
            let scroll = egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, t: &Task, actions: &mut Vec<Action>| {
                    task_row(ui, store, t, view, show_list, selected == Some(t.id), today, actions)
                };

                if searching {
                    let mut found: Vec<&Task> =
                        store.tasks.iter().filter(|t| t.matches_query(&query)).collect();
                    sort_tasks(&mut found, sort);
                    found.sort_by_key(|t| t.done);
                    if found.is_empty() {
                        empty_state(ui, "🔍", "No matching tasks.");
                    }
                    for t in found {
                        row(ui, t, &mut actions);
                    }
                    return;
                }

                if view == View::Completed {
                    let mut done: Vec<&Task> = store.tasks.iter().filter(|t| t.done).collect();
                    done.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
                    if done.is_empty() {
                        empty_state(ui, "✔", "Completed tasks will show up here.");
                    }
                    for t in done {
                        row(ui, t, &mut actions);
                    }
                    return;
                }

                let mut open: Vec<&Task> = store
                    .tasks
                    .iter()
                    .filter(|t| !t.done && store.view_contains(view, t, today))
                    .collect();
                let mut done: Vec<&Task> = store
                    .tasks
                    .iter()
                    .filter(|t| t.done && store.view_contains(view, t, today))
                    .collect();

                if open.is_empty() && done.is_empty() {
                    let (icon, msg) = match view {
                        View::MyDay if touch => ("☀", "Focus on your day. Add a task, or long-press any task and choose “Add to My Day”."),
                        View::MyDay => ("☀", "Focus on your day. Add a task, or right-click any task and choose “Add to My Day”."),
                        View::Important => ("★", "Star tasks to see them here."),
                        View::Planned => ("📅", "Tasks with due dates or reminders show up here."),
                        _ => ("✔", "Nothing here yet. Add a task above."),
                    };
                    empty_state(ui, icon, msg);
                }

                if view == View::Planned {
                    for bucket in PlannedBucket::ALL {
                        let mut group: Vec<&Task> = open
                            .iter()
                            .copied()
                            .filter(|t| {
                                t.planned_date().map(|d| PlannedBucket::of(d, today)) == Some(bucket)
                            })
                            .collect();
                        if group.is_empty() {
                            continue;
                        }
                        group.sort_by_key(|t| (t.planned_date(), !t.important));
                        section_header(ui, bucket.label(), group.len());
                        for t in group {
                            row(ui, t, &mut actions);
                        }
                    }
                } else {
                    sort_tasks(&mut open, sort);
                    for t in open {
                        row(ui, t, &mut actions);
                    }
                }

                if !done.is_empty() {
                    done.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
                    ui.add_space(6.0);
                    egui::CollapsingHeader::new(
                        RichText::new(format!("Completed  {}", done.len())).strong(),
                    )
                    .id_salt(("completed", view))
                    .default_open(false)
                    .show(ui, |ui| {
                        for t in done {
                            row(ui, t, &mut actions);
                        }
                    });
                }
            });
            self.list_scroll_y = scroll.state.offset.y;
            self.list_rect = Some(scroll.inner_rect);
        });
        for a in actions {
            self.apply(a, now);
        }
    }

    /// Android: keeps the app clear of the status bar, navigation bar, notch and on-screen
    /// keyboard by reserving that space with empty panels around the edges.
    fn apply_safe_insets(&mut self, ctx: &egui::Context) {
        if !cfg!(target_os = "android") {
            return;
        }
        if self.insets_checked.is_none_or(|t| t.elapsed() >= StdDuration::from_millis(250)) {
            let fresh = platform::safe_insets(ctx.pixels_per_point());
            if fresh != self.insets {
                self.insets = fresh;
                ctx.request_repaint();
            }
            self.insets_checked = Some(Instant::now());
        }
        // The keyboard slides in and out over a few frames: keep measuring meanwhile.
        if self.keyboard_shown || self.insets[3] > 0.0 {
            ctx.request_repaint_after(StdDuration::from_millis(300));
        }
        let [left, top, right, bottom] = self.insets;
        let frame = egui::Frame::default().fill(ctx.style().visuals.panel_fill);
        if top > 0.5 {
            egui::TopBottomPanel::top("safe_top")
                .exact_height(top)
                .frame(frame)
                .show_separator_line(false)
                .show(ctx, |_| {});
        }
        if bottom > 0.5 {
            egui::TopBottomPanel::bottom("safe_bottom")
                .exact_height(bottom)
                .frame(frame)
                .show_separator_line(false)
                .show(ctx, |_| {});
        }
        if left > 0.5 {
            egui::SidePanel::left("safe_left")
                .exact_width(left)
                .resizable(false)
                .frame(frame)
                .show_separator_line(false)
                .show(ctx, |_| {});
        }
        if right > 0.5 {
            egui::SidePanel::right("safe_right")
                .exact_width(right)
                .resizable(false)
                .frame(frame)
                .show_separator_line(false)
                .show(ctx, |_| {});
        }
    }

    /// Pull-to-refresh: dragging the task list down with a finger while it's scrolled to
    /// the top shows a hint, and letting go past the threshold refreshes.
    fn pull_to_refresh(&mut self, ui: &mut egui::Ui) {
        const THRESHOLD: f32 = 80.0;
        let (touching, down, delta, origin) = ui.input(|i| {
            (i.any_touches(), i.pointer.primary_down(), i.pointer.delta(), i.pointer.press_origin())
        });
        if !self.pull_active {
            let starts_in_list = origin.zip(self.list_rect).is_some_and(|(p, r)| r.contains(p));
            if touching && down && starts_in_list && self.list_scroll_y <= 0.5 && delta.y > 0.0 {
                self.pull_active = true;
                self.pull = 0.0;
            }
        }
        if !self.pull_active {
            return;
        }
        if down {
            self.pull = (self.pull + delta.y).max(0.0);
            ui.ctx().request_repaint();
        } else {
            if self.pull >= THRESHOLD {
                self.refresh_requested = true;
            }
            self.pull_active = false;
            self.pull = 0.0;
            return;
        }
        // The hint grows with the pull (with resistance), pushing the list down.
        let height = (self.pull * 0.5).min(56.0);
        if height < 2.0 {
            return;
        }
        let ready = self.pull >= THRESHOLD;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let progress = (self.pull / THRESHOLD).min(1.0);
        let color = if ready { ACCENT } else { ui.visuals().weak_text_color() };
        let center = rect.center();
        // A ring that fills as you pull, plus a label.
        let r = 9.0;
        let c = Pos2::new(center.x - 70.0, center.y);
        ui.painter().circle_stroke(c, r, Stroke::new(1.5_f32, ui.visuals().weak_text_color().gamma_multiply(0.4)));
        let steps = (progress * 32.0).ceil() as usize;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|k| {
                let a = -std::f32::consts::FRAC_PI_2 + (k as f32 / 32.0) * std::f32::consts::TAU;
                Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
            })
            .collect();
        if pts.len() > 1 {
            ui.painter().add(egui::Shape::line(pts, Stroke::new(2.0_f32, color)));
        }
        ui.painter().text(
            Pos2::new(c.x + 18.0, center.y),
            Align2::LEFT_CENTER,
            if ready { "Release to refresh" } else { "Pull to refresh" },
            FontId::proportional(13.0),
            color,
        );
    }

    /// Web: reload the page (which also picks up a newly published version).
    /// Desktop/Android: re-read the data file (e.g. after it synced) and check for updates.
    fn refresh(&mut self) {
        if self.dirty {
            self.persist();
        }
        #[cfg(target_arch = "wasm32")]
        {
            platform::reload_page();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let loaded = self.storage.load();
            match loaded.warning {
                Some(w) => self.notify(w),
                None => {
                    let settings = self.store.settings.clone();
                    self.store = loaded.store;
                    self.store.settings = settings;
                    if self.selected.is_some_and(|id| self.store.task(id).is_none()) {
                        self.selected = None;
                    }
                    if self.update_rx.is_none() {
                        // Reports "up to date" or offers the update.
                        self.update_manual = true;
                        self.update_available = None;
                    } else {
                        self.notify("Refreshed.");
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ details

    /// Task details: a right side panel on wide screens, the whole screen on phones.
    fn detail_panel(&mut self, ctx: &egui::Context, id: Uuid, now: NaiveDateTime, narrow: bool) {
        let today = now.date();
        let lists: Vec<(Uuid, String)> =
            self.store.lists.iter().map(|l| (l.id, l.name.clone())).collect();
        let mut toggle_done = false;
        let mut delete = false;
        let mut close = false;
        let mut changed = false;
        let new_step = &mut self.new_step;
        let Some(task) = self.store.tasks.iter_mut().find(|t| t.id == id) else {
            return;
        };

        let body = |ui: &mut egui::Ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if narrow {
                        let size = if is_touch(ui) { 20.0 } else { 16.0 };
                        let back = egui::Button::new(RichText::new("<  Back").size(size)).frame(false).min_size(Vec2::new(0.0, 44.0));
                        if ui.add(back).clicked() {
                            close = true;
                        }
                        return;
                    }
                    ui.label(RichText::new("Details").weak());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("✖").on_hover_text("Close (Esc)").clicked() {
                            close = true;
                        }
                    });
                });
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    // Title + steps
                    card(ui, |ui| {
                        ui.horizontal(|ui| {
                            if check_button(ui, task.done, 22.0).clicked() {
                                toggle_done = true;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if star_button(ui, task.important).clicked() {
                                    task.important = !task.important;
                                    changed = true;
                                }
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut task.title)
                                        .font(FontId::proportional(18.0))
                                        .frame(false)
                                        .desired_width(ui.available_width()),
                                );
                                changed |= r.changed();
                            });
                        });
                        ui.add_space(4.0);
                        let mut remove = None;
                        for (i, step) in task.steps.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                if check_button(ui, step.done, 16.0).clicked() {
                                    step.done = !step.done;
                                    changed = true;
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if small_btn(ui, "✖").on_hover_text("Delete step").clicked() {
                                        remove = Some(i);
                                    }
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut step.title)
                                            .frame(false)
                                            .desired_width(ui.available_width()),
                                    );
                                    changed |= r.changed();
                                });
                            });
                        }
                        if let Some(i) = remove {
                            task.steps.remove(i);
                            changed = true;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            ui.label(RichText::new("+").color(ACCENT).strong());
                            let r = ui.add(
                                egui::TextEdit::singleline(new_step)
                                    .hint_text(if task.steps.is_empty() { "Add step" } else { "Next step" })
                                    .frame(false)
                                    .desired_width(f32::INFINITY),
                            );
                            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                if !new_step.trim().is_empty() {
                                    task.steps.push(Step::new(new_step.trim()));
                                    new_step.clear();
                                    changed = true;
                                }
                                r.request_focus();
                            }
                        });
                    });

                    // My Day
                    card(ui, |ui| {
                        let in_day = task.in_my_day(today);
                        let (text, color) = if in_day && is_touch(ui) {
                            ("☀   Added to My Day  (tap to remove)", ACCENT)
                        } else if in_day {
                            ("☀   Added to My Day  (click to remove)", ACCENT)
                        } else {
                            ("☀   Add to My Day", ui.visuals().text_color())
                        };
                        if ui.add(egui::Button::new(RichText::new(text).color(color)).frame(false)).clicked() {
                            task.my_day = if in_day { None } else { Some(today) };
                            changed = true;
                        }
                    });

                    // Reminder, due date, repeat
                    card(ui, |ui| {
                        ui.label(RichText::new("🔔   Remind me").strong());
                        match task.reminder {
                            Some(r) => {
                                ui.horizontal(|ui| {
                                    let mut date = r.date();
                                    let mut h = r.hour();
                                    let mut m = r.minute();
                                    ui.add(egui_extras::DatePickerButton::new(&mut date).id_salt("reminder_date"));
                                    ui.add(egui::DragValue::new(&mut h).range(0..=23).custom_formatter(|v, _| format!("{:02}", v as u32)));
                                    ui.label(":");
                                    ui.add(egui::DragValue::new(&mut m).range(0..=59).speed(0.5).custom_formatter(|v, _| format!("{:02}", v as u32)));
                                    let new_r = date.and_hms_opt(h, m, 0).unwrap_or(r);
                                    if new_r != r {
                                        task.reminder = Some(new_r);
                                        task.reminder_fired = new_r <= now;
                                        changed = true;
                                    }
                                    if small_btn(ui, "✖").on_hover_text("Remove reminder").clicked() {
                                        task.reminder = None;
                                        task.reminder_fired = false;
                                        changed = true;
                                    }
                                });
                                if let Some(r) = task.reminder {
                                    ui.label(RichText::new(format!("{} at {}", relative_date(r.date(), today), r.format("%H:%M"))).weak().small());
                                }
                            }
                            None => {
                                ui.horizontal_wrapped(|ui| {
                                    for (label, when) in reminder_presets(now) {
                                        if ui.button(label).clicked() {
                                            task.reminder = Some(when);
                                            task.reminder_fired = false;
                                            changed = true;
                                        }
                                    }
                                });
                            }
                        }
                        ui.separator();

                        ui.label(RichText::new("📅   Due date").strong());
                        match task.due {
                            Some(due) => {
                                ui.horizontal(|ui| {
                                    let mut date = due;
                                    ui.add(egui_extras::DatePickerButton::new(&mut date).id_salt("due_date"));
                                    if date != due {
                                        task.due = Some(date);
                                        changed = true;
                                    }
                                    let color = if task.is_overdue(today) { DANGER } else { ui.visuals().weak_text_color() };
                                    ui.label(RichText::new(relative_date(date, today)).color(color));
                                    if small_btn(ui, "✖").on_hover_text("Remove due date").clicked() {
                                        task.due = None;
                                        changed = true;
                                    }
                                });
                            }
                            None => {
                                ui.horizontal_wrapped(|ui| {
                                    let presets = [
                                        ("Today", today),
                                        ("Tomorrow", today + Duration::days(1)),
                                        ("Next week", next_monday(today)),
                                    ];
                                    for (label, date) in presets {
                                        if ui.button(label).clicked() {
                                            task.due = Some(date);
                                            changed = true;
                                        }
                                    }
                                });
                            }
                        }
                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔁   Repeat").strong());
                            let mut rec = task.recurrence;
                            egui::ComboBox::from_id_salt("repeat")
                                .selected_text(rec.label())
                                .show_ui(ui, |ui| {
                                    for r in Recurrence::ALL {
                                        ui.selectable_value(&mut rec, r, r.label());
                                    }
                                });
                            if rec != task.recurrence {
                                task.recurrence = rec;
                                // A repeating task needs a date to repeat from.
                                if rec != Recurrence::Never && task.planned_date().is_none() {
                                    task.due = Some(today);
                                }
                                changed = true;
                            }
                        });
                    });

                    // List
                    card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("☰   List").strong());
                            let current = lists
                                .iter()
                                .find(|(lid, _)| *lid == task.list_id)
                                .map(|(_, n)| n.clone())
                                .unwrap_or_default();
                            let mut list_id = task.list_id;
                            egui::ComboBox::from_id_salt("move_list")
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    for (lid, name) in &lists {
                                        ui.selectable_value(&mut list_id, *lid, name);
                                    }
                                });
                            if list_id != task.list_id {
                                task.list_id = list_id;
                                changed = true;
                            }
                        });
                    });

                    // Notes
                    card(ui, |ui| {
                        let r = ui.add(
                            egui::TextEdit::multiline(&mut task.notes)
                                .hint_text("Add note")
                                .desired_rows(6)
                                .frame(false)
                                .desired_width(f32::INFINITY),
                        );
                        changed |= r.changed();
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let created = if task.created.date() == today {
                            "Created today".to_string()
                        } else {
                            format!("Created {}", relative_date(task.created.date(), today))
                        };
                        let info = match task.completed_at {
                            Some(c) => format!("Completed {}", relative_date(c.date(), today)),
                            None => created,
                        };
                        ui.label(RichText::new(info).weak().small());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(RichText::new("🗑 Delete").color(DANGER)))
                                .on_hover_text("Delete task (Del)")
                                .clicked()
                            {
                                delete = true;
                            }
                        });
                    });
                });
        };
        if narrow {
            egui::CentralPanel::default().show(ctx, body);
        } else {
            egui::SidePanel::right("details")
                .resizable(true)
                .default_width(350.0)
                .width_range(290.0..=560.0)
                .show(ctx, body);
        }

        if changed {
            self.dirty = true;
        }
        if toggle_done {
            self.apply(Action::ToggleDone(id), now);
        }
        if close {
            self.selected = None;
        }
        if delete {
            self.delete_task(id);
        }
    }

    // ------------------------------------------------------------------ overlays

    fn alert_windows(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let store = &self.store;
        self.alerts.retain(|id| store.task(*id).is_some_and(|t| !t.done));
        let mut acted = Vec::new();
        for (i, id) in self.alerts.iter().enumerate() {
            let Some(t) = self.store.task(*id) else { continue };
            egui::Window::new("🔔 Reminder")
                .id(Id::new(("alert", *id)))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -16.0 - i as f32 * 140.0))
                .show(ctx, |ui| {
                    ui.set_min_width(280.0);
                    ui.label(RichText::new(&t.title).strong().size(16.0));
                    if let Some(due) = t.due {
                        ui.label(RichText::new(format!("Due {}", relative_date(due, now.date()))).weak());
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("✔ Complete").clicked() {
                            acted.push((*id, AlertAction::Complete));
                        }
                        if ui.button("Open").clicked() {
                            acted.push((*id, AlertAction::Open));
                        }
                        if ui.button("Dismiss").clicked() {
                            acted.push((*id, AlertAction::Dismiss));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Snooze:").weak());
                        for (label, mins) in [("10 min", 10), ("1 hour", 60), ("Tomorrow", 24 * 60)] {
                            if small_btn(ui, label).clicked() {
                                acted.push((*id, AlertAction::Snooze(mins)));
                            }
                        }
                    });
                });
        }
        for (id, action) in acted {
            self.alerts.retain(|a| *a != id);
            match action {
                AlertAction::Open => {
                    self.selected = Some(id);
                    self.search.clear();
                }
                AlertAction::Complete => self.apply(Action::ToggleDone(id), now),
                AlertAction::Snooze(mins) => {
                    if let Some(t) = self.store.task_mut(id) {
                        t.reminder = Some(now + Duration::minutes(mins));
                        t.reminder_fired = false;
                        self.dirty = true;
                    }
                }
                AlertAction::Dismiss => {}
            }
        }
    }

    fn delete_list_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete_list else { return };
        let name = self.store.list_name(id).to_string();
        let count = self.store.tasks.iter().filter(|t| t.list_id == id).count();
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Delete list?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "“{name}” and its {count} task{} will be permanently deleted.",
                    if count == 1 { "" } else { "s" }
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Delete list").color(Color32::WHITE)).fill(DANGER)).clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            if self.store.delete_list(id) {
                if self.view == View::List(id) {
                    self.view = View::List(self.store.default_list);
                }
                if self.selected.is_some_and(|s| self.store.task(s).is_none()) {
                    self.selected = None;
                }
                self.dirty = true;
            }
            self.confirm_delete_list = None;
        } else if cancel {
            self.confirm_delete_list = None;
        }
    }

    fn toast_area(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else { return };
        if toast.at.elapsed() > StdDuration::from_secs(7) {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(StdDuration::from_millis(500));
        let mut undo = false;
        let mut close = false;
        egui::Area::new(Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width((ctx.screen_rect().width() - 80.0).clamp(200.0, 620.0));
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(&toast.text).wrap());
                        if toast.undo.is_some() && ui.button("Undo").clicked() {
                            undo = true;
                        }
                        if small_btn(ui, "✖").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if undo {
            if let Some((idx, task)) = self.toast.take().and_then(|t| t.undo) {
                let id = task.id;
                self.store.restore_task(idx, task);
                self.selected = Some(id);
                self.dirty = true;
            }
        } else if close {
            self.toast = None;
        }
    }
}

impl eframe::App for TodoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Local::now().naive_local();
        let today = now.date();

        let fired = self.store.fire_due_reminders(now);
        if !fired.is_empty() {
            for id in fired {
                // Linux and web: a system notification now. (Windows already got a
                // scheduled one; Android shows the in-app popup.)
                if self.store.settings.notifications {
                    if let Some(t) = self.store.task(id) {
                        notify::show_now(&t.title, &notify::body_text(t.due, today));
                    }
                }
                if !self.alerts.contains(&id) {
                    self.alerts.push(id);
                }
            }
            self.dirty = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
        // Wake up periodically so reminders fire and "today" rolls over at midnight.
        ctx.request_repaint_after(StdDuration::from_secs(5));

        self.maybe_check_updates(ctx, now);
        self.poll_updates(ctx);
        self.handle_dropped_files(ctx);
        #[cfg(target_arch = "wasm32")]
        {
            let picked = self.web_import.borrow_mut().take();
            if let Some((name, bytes)) = picked {
                self.load_import(&name, &bytes);
            }
        }

        self.shortcuts(ctx);
        if self.selected.is_some_and(|id| self.store.task(id).is_none()) {
            self.selected = None;
        }

        // The first touch anywhere switches to the finger-friendly layout.
        if !self.touch_mode && ctx.input(|i| i.any_touches()) {
            self.touch_mode = true;
            apply_style(ctx, true);
        }

        // Must come before the other panels so it sits at the very edges.
        self.apply_safe_insets(ctx);

        // Wide screens: lists | tasks | details side by side.
        // Phones: one screen at a time (lists, or tasks, or one task's details).
        let narrow = ctx.screen_rect().width() < NARROW_WIDTH;
        if narrow {
            if self.show_nav {
                self.sidebar(ctx, today, true);
            } else if let Some(id) = self.selected {
                self.detail_panel(ctx, id, now, true);
            } else {
                self.main_panel(ctx, now, true);
            }
        } else {
            self.show_nav = false;
            self.sidebar(ctx, today, false);
            if let Some(id) = self.selected {
                self.detail_panel(ctx, id, now, false);
            }
            self.main_panel(ctx, now, false);
        }

        // Android needs to be told to show the on-screen keyboard for text fields.
        let wants_keyboard = ctx.wants_keyboard_input();
        if wants_keyboard != self.keyboard_shown {
            platform::show_keyboard(wants_keyboard);
            self.keyboard_shown = wants_keyboard;
        }

        self.alert_windows(ctx, now);
        self.delete_list_dialog(ctx);
        self.import_dialog(ctx, now);
        self.restore_dialog(ctx);
        self.toast_area(ctx);

        if self.dirty {
            self.persist();
        }
        if std::mem::take(&mut self.refresh_requested) {
            self.refresh();
        }
        self.maybe_auto_backup(today);
        self.maybe_start_helper();
    }
}

// ---------------------------------------------------------------------- widgets

const TOUCH_ID: &str = "rustdo_touch_mode";

/// Whether the finger-friendly layout is on (readable from any drawing function).
fn is_touch(ui: &egui::Ui) -> bool {
    ui.ctx().data(|d| d.get_temp::<bool>(Id::new(TOUCH_ID))).unwrap_or(false)
}

/// Text sizes and spacing. Touch mode follows the ~48dp tap-target guideline: bigger
/// text, taller buttons and fields, more room between things.
fn apply_style(ctx: &egui::Context, touch: bool) {
    ctx.data_mut(|d| d.insert_temp(Id::new(TOUCH_ID), touch));
    ctx.all_styles_mut(|s| {
        let (body, small, heading) = if touch { (16.0, 13.0, 24.0) } else { (14.0, 11.5, 22.0) };
        s.text_styles.insert(TextStyle::Body, FontId::proportional(body));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(body));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(small));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(heading));
        if touch {
            s.spacing.item_spacing = Vec2::new(10.0, 10.0);
            s.spacing.button_padding = Vec2::new(14.0, 10.0);
            s.spacing.interact_size = Vec2::new(48.0, 44.0);
            s.spacing.icon_width = 22.0;
            s.spacing.icon_width_inner = 12.0;
            s.spacing.icon_spacing = 8.0;
            s.spacing.combo_width = 120.0;
            s.spacing.scroll.bar_width = 6.0;
        } else {
            let default = egui::style::Spacing::default();
            s.spacing = default;
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = Vec2::new(8.0, 4.0);
        }
    });
}

fn apply_theme(ctx: &egui::Context, theme: ThemeChoice) {
    ctx.set_theme(match theme {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}

fn next_monday(today: NaiveDate) -> NaiveDate {
    today + Duration::days(7 - today.weekday().num_days_from_monday() as i64)
}

fn reminder_presets(now: NaiveDateTime) -> Vec<(&'static str, NaiveDateTime)> {
    let today = now.date();
    let nine = |d: NaiveDate| d.and_hms_opt(9, 0, 0).unwrap();
    let mut out = Vec::new();
    // "Later today": three hours from now, rounded up to the hour.
    let later = (now + Duration::hours(3)).with_minute(0).and_then(|t| t.with_second(0));
    if let Some(later) = later.filter(|l| l.date() == today) {
        out.push(("Later today", later));
    }
    out.push(("Tomorrow 9:00", nine(today + Duration::days(1))));
    out.push(("Next week", nine(next_monday(today))));
    out
}

fn nav_row(ui: &mut egui::Ui, selected: bool, icon: &str, name: &str, count: usize) -> egui::Response {
    let touch = is_touch(ui);
    let (height, font, gap) = if touch { (48.0, 17.0, 40.0) } else { (30.0, 14.0, 34.0) };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let vis = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 6.0, vis.selection.bg_fill.gamma_multiply(0.45));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, vis.widgets.hovered.weak_bg_fill);
    }
    let color = if selected { vis.strong_text_color() } else { vis.text_color() };
    let icon_color = if selected { ACCENT } else { vis.weak_text_color() };
    let p = ui.painter();
    p.text(rect.left_center() + Vec2::new(12.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(font), icon_color);
    p.text(rect.left_center() + Vec2::new(gap, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(font), color);
    if count > 0 {
        p.text(rect.right_center() - Vec2::new(12.0, 0.0), Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(font - 2.0), vis.weak_text_color());
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn paint_check(painter: &egui::Painter, center: Pos2, radius: f32, done: bool, hovered: bool, vis: &egui::Visuals) {
    let check = |color: Color32| {
        let s = radius / 9.0;
        let a = center + Vec2::new(-4.0, 0.0) * s;
        let b = center + Vec2::new(-1.0, 3.5) * s;
        let c = center + Vec2::new(4.5, -3.5) * s;
        let stroke = Stroke::new(1.8 * s.max(0.8), color);
        painter.line_segment([a, b], stroke);
        painter.line_segment([b, c], stroke);
    };
    if done {
        painter.circle_filled(center, radius, ACCENT);
        check(Color32::WHITE);
    } else {
        let color = if hovered { ACCENT } else { vis.weak_text_color() };
        painter.circle_stroke(center, radius, Stroke::new(1.5_f32, color));
        if hovered {
            check(ACCENT.gamma_multiply(0.7));
        }
    }
}

fn check_button(ui: &mut egui::Ui, done: bool, size: f32) -> egui::Response {
    // In touch mode the tap area grows to 44pt; the circle grows a little.
    let (hit, radius) = if is_touch(ui) { (size.max(44.0), size * 0.5) } else { (size, size * 0.42) };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(hit), Sense::click());
    paint_check(ui.painter(), rect.center(), radius, done, resp.hovered(), ui.visuals());
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if done { "Mark as not completed" } else { "Mark as completed" })
}

fn paint_star(ui: &egui::Ui, center: Pos2, important: bool, hovered: bool) {
    let (glyph, color) = if important {
        ("★", STAR)
    } else if hovered {
        ("★", STAR.gamma_multiply(0.5))
    } else {
        ("☆", ui.visuals().weak_text_color())
    };
    let size = if is_touch(ui) { 24.0 } else { 18.0 };
    ui.painter().text(center, Align2::CENTER_CENTER, glyph, FontId::proportional(size), color);
}

fn star_button(ui: &mut egui::Ui, important: bool) -> egui::Response {
    let hit = if is_touch(ui) { 44.0 } else { 24.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(hit), Sense::click());
    paint_star(ui, rect.center(), important, resp.hovered());
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if important { "Remove importance" } else { "Mark as important" })
}

/// A compact button with a mouse; a full-size, finger-friendly one in touch mode.
fn small_btn(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    if is_touch(ui) {
        ui.button(text)
    } else {
        ui.small_button(text)
    }
}

/// Minimum height for text boxes: comfortable to tap in touch mode.
fn field_height(ui: &egui::Ui) -> Vec2 {
    Vec2::new(0.0, if is_touch(ui) { 44.0 } else { 0.0 })
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(6.0);
}

fn section_header(ui: &mut egui::Ui, title: &str, count: usize) {
    ui.add_space(6.0);
    ui.label(RichText::new(format!("{title}   {count}")).strong().color(ACCENT));
}

fn empty_state(ui: &mut egui::Ui, icon: &str, msg: &str) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(icon).size(48.0).color(ACCENT.gamma_multiply(0.6)));
        ui.add_space(8.0);
        ui.label(RichText::new(msg).weak());
    });
}

#[allow(clippy::too_many_arguments)]
fn task_row(
    ui: &mut egui::Ui,
    store: &Store,
    t: &Task,
    view: View,
    show_list: bool,
    selected: bool,
    today: NaiveDate,
    actions: &mut Vec<Action>,
) {
    let vis = ui.visuals().clone();
    let weak = vis.weak_text_color();

    // Second line: list, My Day, steps, due date, reminder, repeat, note.
    let mut meta: Vec<(String, Color32)> = Vec::new();
    if show_list {
        meta.push((store.list_name(t.list_id).to_string(), weak));
    }
    if view != View::MyDay && t.in_my_day(today) {
        meta.push(("☀ My Day".into(), weak));
    }
    if !t.steps.is_empty() {
        let n = t.steps.iter().filter(|s| s.done).count();
        meta.push((format!("{n} of {}", t.steps.len()), weak));
    }
    if let Some(due) = t.due {
        let color = if t.is_overdue(today) {
            DANGER
        } else if due == today && !t.done {
            ACCENT
        } else {
            weak
        };
        let prefix = if t.is_overdue(today) { "Overdue, " } else { "" };
        meta.push((format!("📅 {prefix}{}", relative_date(due, today)), color));
    }
    if let Some(r) = t.reminder {
        if !t.done && !t.reminder_fired {
            meta.push((format!("🔔 {}", r.format("%H:%M")), weak));
        }
    }
    if t.recurrence != Recurrence::Never {
        meta.push(("🔁".into(), weak));
    }
    if !t.notes.trim().is_empty() {
        meta.push(("📝 Note".into(), weak));
    }

    // Touch mode: taller rows, bigger text, and 48pt tap areas for the circle and star.
    let touch = is_touch(ui);
    let (h_one, h_two, title_size, meta_size, hit, radius, inset, text_gap) = if touch {
        (56.0, 66.0, 17.0, 13.5, 48.0, 12.0, 28.0, 56.0)
    } else {
        (42.0, 52.0, 14.5, 11.5, 24.0, 9.5, 22.0, 44.0)
    };
    let height = if meta.is_empty() { h_one } else { h_two };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let bg = if selected {
        vis.selection.bg_fill.gamma_multiply(0.35)
    } else if resp.hovered() {
        vis.widgets.hovered.weak_bg_fill
    } else {
        vis.faint_bg_color
    };
    ui.painter().rect_filled(rect, 6.0, bg);

    // Interactive sub-areas registered after the row so they take priority over it.
    let check_rect = Rect::from_center_size(Pos2::new(rect.left() + inset, rect.center().y), Vec2::splat(hit));
    let check = ui.interact(check_rect, Id::new(("check", t.id)), Sense::click());
    let star_rect = Rect::from_center_size(Pos2::new(rect.right() - inset, rect.center().y), Vec2::splat(hit));
    let star = ui.interact(star_rect, Id::new(("star", t.id)), Sense::click());

    paint_check(ui.painter(), check_rect.center(), radius, t.done, check.hovered(), &vis);
    paint_star(ui, star_rect.center(), t.important, star.hovered());

    let text_left = rect.left() + text_gap;
    let text_width = (star_rect.left() - 8.0 - text_left).max(10.0);

    let mut title_job = LayoutJob::single_section(
        t.title.clone(),
        TextFormat {
            font_id: FontId::proportional(title_size),
            color: if t.done { weak } else { vis.text_color() },
            strikethrough: if t.done { Stroke::new(1.0_f32, weak) } else { Stroke::NONE },
            ..Default::default()
        },
    );
    title_job.wrap = TextWrapping::truncate_at_width(text_width);
    let title = ui.fonts(|f| f.layout_job(title_job));

    if meta.is_empty() {
        let pos = Pos2::new(text_left, rect.center().y - title.size().y / 2.0);
        ui.painter().galley(pos, title, vis.text_color());
    } else {
        let mut job = LayoutJob::default();
        for (i, (text, color)) in meta.into_iter().enumerate() {
            if i > 0 {
                job.append("  ·  ", 0.0, TextFormat { font_id: FontId::proportional(meta_size), color: weak, ..Default::default() });
            }
            job.append(&text, 0.0, TextFormat { font_id: FontId::proportional(meta_size), color, ..Default::default() });
        }
        job.wrap = TextWrapping::truncate_at_width(text_width);
        let meta_galley = ui.fonts(|f| f.layout_job(job));
        let total = title.size().y + 3.0 + meta_galley.size().y;
        let top = rect.center().y - total / 2.0;
        let title_h = title.size().y;
        ui.painter().galley(Pos2::new(text_left, top), title, vis.text_color());
        ui.painter().galley(Pos2::new(text_left, top + title_h + 3.0), meta_galley, weak);
    }

    if check.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        actions.push(Action::ToggleDone(t.id));
    } else if star.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        actions.push(Action::ToggleImportant(t.id));
    } else if resp.clicked() {
        actions.push(Action::Select(t.id));
    }

    resp.context_menu(|ui| {
        let my_day = if t.in_my_day(today) { "☀  Remove from My Day" } else { "☀  Add to My Day" };
        if ui.button(my_day).clicked() {
            actions.push(Action::ToggleMyDay(t.id));
            ui.close_menu();
        }
        let imp = if t.important { "☆  Remove importance" } else { "★  Mark as important" };
        if ui.button(imp).clicked() {
            actions.push(Action::ToggleImportant(t.id));
            ui.close_menu();
        }
        let done = if t.done { "○  Mark as not completed" } else { "✔  Mark as completed" };
        if ui.button(done).clicked() {
            actions.push(Action::ToggleDone(t.id));
            ui.close_menu();
        }
        ui.separator();
        if ui.button(RichText::new("🗑  Delete task").color(DANGER)).clicked() {
            actions.push(Action::Delete(t.id));
            ui.close_menu();
        }
    });
}
