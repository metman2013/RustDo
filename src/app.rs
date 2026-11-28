//! The egui user interface.

use crate::account;
use crate::backgrounds::{self, Background};
use crate::mstodo;
use crate::showlist;
use crate::i18n::{self, plural, tr, trf};
use crate::model::{
    format_time, from_12h, relative_date, sort_tasks, to_12h, PlannedBucket, Recurrence, SortMode, Step, Store, Task,
    ThemeChoice, View,
};
use crate::import::{self, Import};
use crate::keep;
use crate::changelog;
use crate::companion;
use crate::store_package;
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use crate::diagnostics;
use crate::notes;
use crate::notes_ui;
use crate::notify;
use crate::platform;
use crate::storage::{self, Storage};
use crate::sync::{self, Done, Job};
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
/// Sidebar room for the "Desktop notes" section below the lists.
const NOTES_SECTION_H: f32 = 62.0;
/// Sidebar room for the footer: version, What's new, Discord.
const SIDEBAR_FOOTER_H: f32 = 74.0;
/// Icons for a user's list and for "All" (☰ is the menu button).
const LIST_ICON: &str = "📋";
const ALL_ICON: &str = "📚";
/// Discord's own colour, for the "Join the Discord" button.
const DISCORD_BLUE: Color32 = Color32::from_rgb(0x58, 0x65, 0xF2);

/// Import is offered where we can pick a file: desktop and web, not the Android build.
/// The sidebar's "Import from Microsoft To Do" (signing in works everywhere).
const CAN_IMPORT: bool = true;
/// Importing an export file (Android has no file picker here).
const CAN_IMPORT_FILE: bool = cfg!(not(target_os = "android"));

/// Built at runtime from `update::REPO` (like the other GitHub links) so the binary never
/// contains "/RustDo/", which tools/build-release.ps1 treats as a leaked local path.
fn hub_docs() -> String {
    format!("https://github.com/{}/blob/main/docs/hub.md", update::REPO)
}

const ACCENT: Color32 = Color32::from_rgb(70, 125, 230);
const DANGER: Color32 = Color32::from_rgb(215, 75, 75);
const STAR: Color32 = Color32::from_rgb(240, 170, 30);

enum Action {
    /// Show or hide a task's subtasks in the list.
    ToggleExpand(Uuid),
    /// Tick or untick a subtask (task, subtask).
    ToggleStep(Uuid, Uuid),
    /// Add a subtask from the list (task, title).
    AddStep(Uuid, String),
    ToggleDone(Uuid),
    ToggleImportant(Uuid),
    ToggleMyDay(Uuid),
    Select(Uuid),
    Delete(Uuid),
    /// Move a task in "My order": (the open tasks as shown, the task, its new place).
    MoveTask(Vec<Uuid>, Uuid, usize),
    /// Make a sticky note from a task (and remove the task).
    ToNote(Uuid),
    /// Swiped right: tick it off (with Undo).
    SwipeDone(Uuid),
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

/// "Import from Microsoft To Do": sign in with Microsoft, or pick an export file.
#[derive(Default)]
struct MsImportDialog {
    session: Option<mstodo::Session>,
    /// Desktop and Android: the code to enter, and where.
    code: Option<(String, String)>,
    status: Option<String>,
    error: Option<String>,
}

#[cfg(target_arch = "wasm32")]
fn resume_ms_import(ctx: &egui::Context) -> Option<MsImportDialog> {
    mstodo::resume_web(ctx).map(|session| MsImportDialog { session: Some(session), ..Default::default() })
}

#[cfg(not(target_arch = "wasm32"))]
fn resume_ms_import(_ctx: &egui::Context) -> Option<MsImportDialog> {
    None
}

/// "Sync my devices": turning sync on, or showing the sync key.
struct AccountDialog {
    hub: String,
    /// Joining an account another device started (else: start a new one).
    join: bool,
    key: String,
    busy: bool,
    error: Option<String>,
    /// The key to show (just created, or "Sync key…" in Settings).
    show_key: Option<String>,
}

/// The "Share list" / "Join a shared list" dialog.
struct ShareDialog {
    /// The list being shared; `None` when joining someone else's list.
    list: Option<Uuid>,
    hub: String,
    code: String,
    busy: bool,
    error: Option<String>,
}

struct Toast {
    text: String,
    undo: Option<UndoOp>,
    at: Instant,
}

/// What a toast's Undo puts back.
enum UndoOp {
    /// A deleted task, at its old place.
    Deleted(usize, Task),
    /// A task ticked off (by a swipe), and the next repeat that made, if any.
    Completed { id: Uuid, next: Option<Uuid> },
    /// A sticky note put in the Archive.
    Archived(Uuid),
}

pub struct TodoApp {
    /// Bigger, finger-friendly layout (phones, tablets, touch screens).
    touch_mode: bool,
    store: Store,
    storage: Storage,
    can_save: bool,
    /// Phone layout: showing the list picker instead of the task list.
    show_nav: bool,
    /// Phones: settings fill the screen as their own page instead of a pop-up menu.
    show_settings: bool,
    /// Showing the Sticky notes page instead of a task list (desktop).
    notes_page: bool,
    /// Sticky notes (desktop): shared with the note windows, which draw themselves.
    notes: notes_ui::SharedNotes,
    /// The background companion is running (it then owns the note windows).
    companion_running: bool,
    companion_checked: Option<Instant>,
    /// Held while this window is open, so the tray's "Open RustDo" brings it to the front.
    _app_lock: Option<std::fs::File>,
    keyboard_shown: bool,
    /// Whether the window had the focus last frame (to bring the keyboard back on return).
    window_focused: Option<bool>,
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
    /// A full backup to restore: file name, tasks, and the sticky notes it carries (if any).
    pending_restore: Option<(String, Store, Option<notes::Notes>)>,
    /// A sticky notes backup to merge in (desktop).
    pending_notes_restore: Option<(String, notes::Notes)>,
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
    /// Android: a new inset reading waiting for a second one that agrees (see
    /// `apply_safe_insets`).
    insets_pending: Option<[f32; 4]>,
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
    /// Tasks whose subtasks are shown in the list.
    expanded: std::collections::HashSet<Uuid>,
    /// What's typed in each list "Add a subtask" box (by task).
    step_inputs: std::collections::HashMap<Uuid, String>,
    /// The task just added from "Add a task": its "Add a subtask" box is open under it.
    last_added: Option<(Uuid, View)>,
    renaming_list: Option<(Uuid, String)>,
    /// "Show list": the view whose open tasks are shown full screen, to read or hand over.
    show_list: Option<View>,
    /// Desktop: the list being dragged to a new place in the menu.
    list_drag: Option<Uuid>,
    rename_focus: bool,
    confirm_delete_list: Option<Uuid>,
    alerts: Vec<Uuid>,
    /// How many reminders the reminders box showed last frame (a new one re-opens it).
    alerts_shown: usize,
    /// Whether the reminders box is expanded (false: just its header).
    alerts_open: bool,
    toast: Option<Toast>,
    focus_add: bool,
    focus_search: bool,
    pending_import: Option<PendingImport>,
    // Family sharing (RustDo Hub)
    sync_rx: Option<Receiver<Vec<Done>>>,
    sync_jobs: Vec<Job>,
    last_sync_at: Option<Instant>,
    /// Something changed: sync shortly instead of waiting for the regular round.
    sync_soon: bool,
    sync_error: Option<String>,
    share_dialog: Option<ShareDialog>,
    /// The "How family sharing works" guide is open.
    hub_guide: bool,
    // "Sync my devices"
    account_rx: Option<Receiver<account::Done>>,
    account_last: Option<Instant>,
    account_soon: bool,
    account_error: Option<String>,
    account_dialog: Option<AccountDialog>,
    ms_import: Option<MsImportDialog>,
    // Backgrounds
    /// Your own background pictures on this device (id → JPEG).
    bg_images: backgrounds::Images,
    /// Drawn backgrounds: "builtin:<id>" or "custom:<id>" → texture.
    bg_textures: std::collections::HashMap<String, egui::TextureHandle>,
    /// "Background…" for one list (its id), from the list's menu.
    bg_dialog: Option<Uuid>,
    /// Web: a picture picked in the browser, waiting to be stored: (list or app-wide, bytes).
    bg_web_pick: WebPick,
    /// The look last applied: (dark theme, a background showing).
    bg_style: Option<(bool, bool)>,
    /// When the pictures folder was last checked for pictures added elsewhere (a sticky
    /// note's picture added in the companion), so they sync too.
    bg_scanned: Option<Instant>,
    /// Pictures removed by sync whose note textures should be dropped.
    bg_forget: Vec<String>,
    /// Android: RustDo's own photo picker, while it's open.
    #[cfg(target_os = "android")]
    photo_picker: Option<crate::photos::PhotoPicker>,
    /// Notes changes up to here are already known to the account sync.
    account_notes_seen: Option<Instant>,
    /// The "What's new" dialog: the releases to describe, newest first.
    whats_new: Option<Vec<changelog::Release>>,
    /// "Get RustDo for your other devices" is open.
    get_apps: bool,
    /// The Settings section showing.
    settings_tab: SettingsTab,
    /// Desktop: the one-time "RustDo keeps running in the tray" message (shown instead of
    /// closing the first time).
    close_hint: bool,
    /// Closing for real (quit completely, or after the message): don't ask again.
    closing: bool,
}

impl TodoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (storage, moved_notice) = Storage::for_platform();
        let bg_images = backgrounds::load(storage.folder().as_deref());
        let mut loaded = storage.load();
        // A load problem is more important to show than the "moved to Documents" notice.
        if loaded.warning.is_none() {
            loaded.warning = moved_notice;
        }
        if let Some(w) = &loaded.warning {
            log::warn!(target: "rustdo", "loading tasks: {w}");
        }
        log::info!(
            target: "rustdo",
            "{} lists, {} tasks; can save: {}",
            loaded.store.lists.len(),
            loaded.store.tasks.len(),
            loaded.can_save
        );
        apply_theme(&cc.egui_ctx, loaded.store.settings.theme);
        i18n::set(loaded.store.settings.language);
        let touch_mode = platform::prefers_touch();
        apply_style(&cc.egui_ctx, touch_mode);
        let notes = notes_ui::Shared::load(if notes::AVAILABLE { storage.folder() } else { None });

        // An automatic update that did finish: forget the attempt.
        if loaded.store.settings.update_attempt.as_deref().is_some_and(|v| {
            changelog::version_key(v) <= changelog::version_key(update::CURRENT_VERSION)
        }) {
            loaded.store.settings.update_attempt = None;
        }

        // "What's new" after an update. A brand-new install (no tasks yet) just records
        // the version: there's nothing "new" to someone who has never used RustDo.
        let mut first_run_dirty = false;
        let settings = &mut loaded.store.settings;
        let whats_new = if settings.last_seen_version.is_none() && loaded.store.tasks.is_empty() {
            settings.last_seen_version = Some(update::CURRENT_VERSION.to_string());
            first_run_dirty = loaded.can_save;
            None
        } else {
            let unseen = changelog::unseen(
                &changelog::releases(),
                settings.last_seen_version.as_deref(),
                update::CURRENT_VERSION,
            );
            (!unseen.is_empty()).then_some(unseen)
        };
        Self {
            touch_mode,
            store: loaded.store,
            storage,
            can_save: loaded.can_save,
            show_nav: false,
            show_settings: false,
            notes_page: false,
            notes,
            // Known from the first frame, so notes the companion shows don't flash up twice.
            companion_running: companion::SUPPORTED && companion::is_running(),
            companion_checked: None,
            _app_lock: if companion::SUPPORTED { companion::lock_app() } else { None },
            keyboard_shown: false,
            window_focused: None,
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
            pending_notes_restore: None,
            paste_restore: None,
            backup_day: None,
            last_backup: None,
            backup_pending: false,
            backup_error_shown: false,
            export_nag_dismissed: false,
            insets: [0.0; 4],
            insets_pending: None,
            insets_checked: None,
            pull: 0.0,
            pull_active: false,
            refresh_requested: false,
            list_scroll_y: 0.0,
            list_rect: None,
            #[cfg(target_arch = "wasm32")]
            web_import: Default::default(),
            dirty: first_run_dirty,
            view: View::MyDay,
            selected: None,
            search: String::new(),
            new_task: String::new(),
            new_list: String::new(),
            new_step: String::new(),
            expanded: Default::default(),
            step_inputs: Default::default(),
            last_added: None,
            renaming_list: None,
            list_drag: None,
            show_list: None,
            rename_focus: false,
            confirm_delete_list: None,
            alerts: Vec::new(),
            alerts_shown: 0,
            alerts_open: true,
            toast: loaded.warning.map(|text| Toast { text, undo: None, at: Instant::now() }),
            // Don't pop up the on-screen keyboard as soon as a phone opens the app.
            focus_add: !platform::IS_MOBILE_OR_WEB,
            focus_search: false,
            pending_import: None,
            sync_rx: None,
            sync_jobs: Vec::new(),
            last_sync_at: None,
            sync_soon: false,
            sync_error: None,
            share_dialog: None,
            hub_guide: false,
            account_rx: None,
            account_last: None,
            account_soon: false,
            account_error: None,
            account_dialog: None,
            // Web: back from Microsoft's sign-in page, carry on with the import.
            ms_import: resume_ms_import(&cc.egui_ctx),
            bg_images,
            bg_textures: Default::default(),
            bg_dialog: None,
            bg_web_pick: Default::default(),
            bg_scanned: None,
            bg_forget: Vec::new(),
            #[cfg(target_os = "android")]
            photo_picker: None,
            bg_style: None,
            account_notes_seen: None,
            whats_new,
            get_apps: false,
            settings_tab: SettingsTab::default(),
            close_hint: false,
            closing: false,
        }
    }

    /// Desktop: choose a RustDo backup to restore, starting in the Backups folder.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn pick_restore_file(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new()
            .set_title(tr("Restore RustDo backup"))
            .add_filter(tr("RustDo backup"), &["json"]);
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
            Err(e) => self.notify(trf("Couldn't read {path}: {e}", &[("path", &path.display()), ("e", &e)])),
        }
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn pick_import_file(&mut self, _ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .set_title(tr("Import from Microsoft To Do"))
            .add_filter(tr("Microsoft To Do / Outlook export"), &["json", "csv"])
            .add_filter(tr("All files"), &["*"])
            .pick_file();
        if let Some(path) = picked {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.load_import(&name, &bytes);
                }
                Err(e) => self.notify(trf("Couldn't read {path}: {e}", &[("path", &path.display()), ("e", &e)])),
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_import_file(&mut self, ctx: &egui::Context) {
        let slot = self.web_import.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let file = rfd::AsyncFileDialog::new()
                .add_filter(tr("Microsoft To Do / Outlook export"), &["json", "csv"])
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

    /// Google Keep: a Takeout zip (or one note's .json).
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn pick_keep_file(&mut self, _ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .set_title(tr("Import from Google Keep"))
            .add_filter(tr("Google Takeout export"), &["zip", "json"])
            .add_filter(tr("All files"), &["*"])
            .pick_file();
        if let Some(path) = picked {
            self.read_picked_file(&path);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_keep_file(&mut self, ctx: &egui::Context) {
        let slot = self.web_import.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let file = rfd::AsyncFileDialog::new().add_filter(tr("Google Takeout export"), &["zip", "json"]).pick_file().await;
            if let Some(file) = file {
                let bytes = file.read().await;
                *slot.borrow_mut() = Some((file.file_name(), bytes));
                ctx.request_repaint();
            }
        });
    }

    #[cfg(target_os = "android")]
    fn pick_keep_file(&mut self, _ctx: &egui::Context) {}

    /// Whether a list has a card showing (on the desktop, or on the wall).
    fn list_card_showing(&self, list: Uuid) -> bool {
        notes_ui::lock(&self.notes).notes.card_for(list).is_some_and(|n| !n.archived && (n.open || notes::WALL))
    }

    /// 📌: pins a list (or shows its hidden card), or unpins it.
    fn toggle_pin(&mut self, list: Uuid) {
        let name = self.store.list_name(list).to_string();
        if self.list_card_showing(list) {
            notes_ui::unpin_list(&self.notes, list);
            self.notify(trf("Unpinned “{list}”.", &[("list", &name)]));
        } else {
            notes_ui::pin_list(&self.notes, list, &name);
            self.notify(if notes::WALL {
                trf("Pinned “{list}” to your sticky notes.", &[("list", &name)])
            } else {
                trf("Pinned “{list}” to your desktop.", &[("list", &name)])
            });
        }
    }

    /// Pinned lists: carries out what was done on their cards (here, or in the background
    /// companion's windows), and gives the cards the lists' current tasks.
    fn pinned_tick(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        if !notes::AVAILABLE {
            return;
        }
        let (lists, actions) = {
            let mut s = notes_ui::lock(&self.notes);
            (s.notes.pinned_lists(), std::mem::take(&mut s.card_actions))
        };
        // Desktop: also what was done on the background companion's cards.
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        let actions = [actions, crate::pinned::handoff::take()].concat();
        if !actions.is_empty() && crate::pinned::apply(&mut self.store, &actions, now) {
            self.dirty = true;
            // This frame's lists were drawn already: show the change now.
            ctx.request_repaint();
        }
        let cards = crate::pinned::cards_from_store(&self.store, lists, now.date());
        notes_ui::set_cards(ctx, &self.notes, cards);
    }

    /// Windows: copies the notes from Microsoft Sticky Notes on this PC (or a `plum.sqlite`
    /// picked by hand, e.g. from another PC's backup).
    #[cfg(windows)]
    fn import_ms_sticky_notes(&mut self) {
        let path = match crate::msnotes::default_path() {
            Some(p) => p,
            None => {
                self.notify(tr("Sticky Notes wasn't found on this PC (Sticky Notes in OneNote keeps its notes in OneNote). You can also pick a plum.sqlite file."));
                let Some(p) = rfd::FileDialog::new()
                    .set_title(tr("Import from Microsoft Sticky Notes"))
                    .add_filter(tr("Sticky Notes database"), &["sqlite"])
                    .pick_file()
                else {
                    return;
                };
                p
            }
        };
        match crate::msnotes::read(&path, Local::now().naive_local()) {
            Ok(found) => {
                let total = found.len();
                let added = notes_ui::lock(&self.notes).import(found);
                self.notes_page = true;
                self.notify(if total == 0 {
                    tr("Sticky Notes has no notes to import.").to_string()
                } else if added == 0 {
                    tr("Nothing new to import: you already have these Sticky Notes.").to_string()
                } else {
                    format!(
                        "{} {}",
                        plural(added, "Imported {n} note from Microsoft Sticky Notes.", "Imported {n} notes from Microsoft Sticky Notes."),
                        tr("Show them from this page.")
                    )
                });
            }
            Err(e) => self.notify(trf("Couldn't read Sticky Notes: {e}", &[("e", &e)])),
        }
    }

    #[cfg(not(windows))]
    fn import_ms_sticky_notes(&mut self) {}

    /// Adds the notes from a Google Keep export to the sticky notes.
    fn import_keep(&mut self, bytes: &[u8]) {
        match keep::parse(bytes, Local::now().naive_local()) {
            Ok(found) => {
                let total = found.notes.len();
                let added = notes_ui::lock(&self.notes).import(found.notes);
                self.notes_page = true;
                self.notify(if total == 0 {
                    tr("Nothing to import: these Keep notes are all in Keep's trash.").to_string()
                } else if added == 0 {
                    tr("Nothing new to import: you already have these Keep notes.").to_string()
                } else {
                    let mut msg = plural(added, "Imported {n} note from Google Keep.", "Imported {n} notes from Google Keep.");
                    if notes::SUPPORTED {
                        msg = format!("{msg} {}", tr("Show them from this page."));
                    }
                    msg
                });
            }
            Err(e) => self.notify(e),
        }
    }

    fn load_import(&mut self, file_name: &str, bytes: &[u8]) {
        // A Google Takeout zip, or one Keep note: into the sticky notes.
        if notes::AVAILABLE && (bytes.starts_with(b"PK") || keep::looks_like_note(bytes)) {
            self.import_keep(bytes);
            return;
        }
        // A RustDo backup is restored (after confirmation) rather than merged.
        if let Ok(store) = storage::parse_backup(bytes) {
            let carried = notes::parse_backup(bytes).ok().filter(|_| notes::AVAILABLE);
            self.pending_restore = Some((file_name.to_string(), store, carried));
            return;
        }
        // A sticky notes backup (desktop) is merged in (after confirmation).
        if notes::AVAILABLE {
            if let Ok(found) = notes::parse_backup(bytes) {
                self.pending_notes_restore = Some((file_name.to_string(), found));
                return;
            }
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
        // The web app updates itself, and the Store updates a Store copy.
        if cfg!(target_arch = "wasm32")
            || store_package::is_packaged()
            || self.update_rx.is_some()
            || self.update_available.is_some()
        {
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
                        let version = release.version.clone();
                        // An earlier automatic update to this version didn't finish (we're
                        // still running the old one): don't close RustDo for it again.
                        let failed_before = self.store.settings.update_attempt.as_deref() == Some(version.as_str());
                        let auto = self.can_auto_update()
                            && self.started.elapsed() < AUTO_UPDATE_WINDOW
                            && release.download_url.is_some()
                            && !failed_before;
                        if failed_before {
                            self.notify(trf(
                                "The last automatic update to {version} didn't finish. Use “Update & restart” to try again.",
                                &[("version", &version)],
                            ));
                        }
                        self.update_available = Some(release);
                        if auto {
                            self.auto_installing = true;
                            self.install_update(ctx);
                            self.notify(trf("Updating RustDo to {version}… it will reopen in a moment.", &[("version", &version)]));
                        }
                    }
                    CheckResult::UpToDate if manual => {
                        self.notify(trf("You're up to date (RustDo {version}).", &[("version", &update::CURRENT_VERSION)]))
                    }
                    CheckResult::Failed(e) if manual => self.notify(trf("Couldn't check for updates: {e}", &[("e", &e)])),
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
                        self.notify(trf("Couldn't download the update: {e}", &[("e", &e)]));
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
                || self.pending_notes_restore.is_some()
                || self.confirm_delete_list.is_some()
                || self.share_dialog.is_some())
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
        // The reminder helper runs from this same exe: stop it so the installer can replace it.
        notify::stop_helper();
        // Remember the attempt: if RustDo starts again still on the old version, the update
        // failed and we won't retry it automatically at every start.
        if self.auto_installing {
            self.store.settings.update_attempt = self.update_available.as_ref().map(|r| r.version.clone());
        }
        let mut args = vec![
            "/VERYSILENT".to_string(),
            "/SUPPRESSMSGBOXES".into(),
            "/NORESTART".into(),
            "/CLOSEAPPLICATIONS".into(),
        ];
        // A log next to the tasks, for when an update goes wrong.
        if let Some(dir) = self.storage.folder() {
            args.push(format!("/LOG={}", dir.join("update-install.log").display()));
        }
        log::info!(target: "rustdo", "running the installer {} {args:?}", installer.display());
        match std::process::Command::new(&installer).args(&args).spawn() {
            Ok(_) => {
                // The installer replaces this program, so get out of its way.
                self.persist();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => {
                self.auto_installing = false;
                self.notify(trf("Couldn't start the installer: {e}", &[("e", &e)]));
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
            self.notify(tr("Downloading the update…"));
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
                    ui.label(RichText::new(trf("RustDo {version} is available.", &[("version", &release.version)])).strong());
                    let busy = self.install_rx.is_some() || (self.auto_installing && self.ready_installer.is_some());
                    let label = if self.auto_installing && self.ready_installer.is_some() {
                        tr("Updating when you finish typing…")
                    } else if busy {
                        tr("Downloading…")
                    } else if self.ready_installer.is_some() {
                        tr("Restart to update")
                    } else if cfg!(windows) {
                        tr("Update & restart")
                    } else {
                        tr("Update now")
                    };
                    if ui
                        .add_enabled(!busy, egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(ACCENT))
                        .clicked()
                    {
                        install = true;
                    }
                    ui.hyperlink_to(tr("What's new"), &release.notes_url);
                    if ui.button(tr("Later")).clicked() {
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

    /// The ⚙ button: Settings as a pane on the right on wide screens, a full page on
    /// phones. Pressing it again (or ✖, or Escape) closes it.
    fn settings_button(&mut self, ui: &mut egui::Ui, _narrow: bool) {
        if ui.add(egui::Button::new("⚙").selected(self.show_settings)).on_hover_text(tr("Settings")).clicked() {
            self.show_settings = !self.show_settings;
        }
    }

    /// Wide screens: Settings as a full-height pane on the right (where a task's details
    /// go), with room for each section.
    fn settings_pane(&mut self, ctx: &egui::Context) {
        let side = self.side_frame(ctx);
        let mut close = false;
        egui::SidePanel::right("settings_pane")
            .frame(side)
            .resizable(true)
            .default_width(420.0)
            .width_range(340.0..=600.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Settings")).size(20.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        close = small_btn(ui, "✖").on_hover_text(tr("Close")).clicked();
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    self.settings_contents(ui);
                    ui.add_space(24.0);
                });
            });
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.show_settings = false;
        }
    }

    /// Phones: settings as their own scrolling page with a Back button.
    fn settings_page(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if back_button(ui, tr("Back")).clicked() {
                    self.show_settings = false;
                }
                ui.label(RichText::new(tr("Settings")).size(20.0).strong());
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                self.settings_contents(ui);
                ui.add_space(24.0);
            });
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.show_settings = false;
        }
    }

    /// Done with settings: closes the pop-up menu, or leaves the phone settings page.
    fn close_settings(&mut self, ui: &mut egui::Ui) {
        ui.close_menu();
        self.show_settings = false;
    }

    fn settings_contents(&mut self, ui: &mut egui::Ui) {
        // Sections as tabs: one at a time, so it's short and easy to scan.
        ui.horizontal_wrapped(|ui| {
            for tab in SettingsTab::ALL {
                if ui.selectable_label(self.settings_tab == tab, tab.label()).clicked() {
                    self.settings_tab = tab;
                }
            }
        });
        ui.add_space(6.0);
        match self.settings_tab {
            SettingsTab::Appearance => card(ui, |ui| self.settings_appearance(ui)),
            SettingsTab::Reminders => {
                card(ui, |ui| self.notification_menu_items(ui));
                if companion::SUPPORTED {
                    card(ui, |ui| self.companion_menu_items(ui));
                }
            }
            SettingsTab::Sync => {
                card(ui, |ui| self.account_menu_items(ui));
                card(ui, |ui| self.sharing_menu_items(ui));
            }
            SettingsTab::Backups => card(ui, |ui| self.backup_menu_items(ui)),
            SettingsTab::About => card(ui, |ui| self.settings_about(ui)),
        }
    }

    /// Settings › Appearance: language, background, clock, sharp text.
    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(tr("Language")).strong());
        let mut language = self.store.settings.language;
        egui::ComboBox::from_id_salt("language")
            .selected_text(language.label())
            .show_ui(ui, |ui| {
                for l in i18n::Language::ALL {
                    ui.selectable_value(&mut language, l, l.label());
                }
            });
        if language != self.store.settings.language {
            self.store.settings.language = language;
            i18n::set(language);
            // The default list follows the language while it still has its default name.
            let default = self.store.default_list;
            if let Some(list) = self.store.lists.iter_mut().find(|l| l.id == default) {
                if i18n::default_list_names().contains(&list.name.as_str()) {
                    list.name = tr("Tasks").to_string();
                }
            }
            self.dirty = true;
        }
        ui.separator();
        ui.label(RichText::new(tr("Background")).strong());
        self.background_picker(ui, None);
        let any = self.store.settings.background != Background::None || !self.store.settings.list_backgrounds.is_empty();
        if any {
            ui.horizontal(|ui| {
                ui.label(tr("Dim background"));
                let mut dim = self.store.settings.background_dim;
                if ui.add(egui::Slider::new(&mut dim, 0.0..=0.9).show_value(false)).changed() {
                    self.store.settings.background_dim = (dim * 20.0).round() / 20.0;
                    self.dirty = true;
                }
            });
        }
        ui.separator();
        ui.label(RichText::new(tr("Time")).strong());
        let s = &mut self.store.settings;
        if ui.radio_value(&mut s.clock_24h, false, tr("12-hour clock (9:30 PM)")).changed()
            | ui.radio_value(&mut s.clock_24h, true, tr("24-hour clock (21:30)")).changed()
        {
            self.dirty = true;
        }
        if cfg!(target_os = "android") || platform::web_touch_screen() {
            if ui.checkbox(&mut self.store.settings.crisp_text, tr("Sharp text")).changed() {
                self.dirty = true;
            }
            let native = ui.ctx().native_pixels_per_point().unwrap_or(1.0);
            ui.label(
                RichText::new(trf(
                    "Rounds the screen scale to whole pixels so letters render evenly (screen {native}x, drawn at {drawn}x). Turn off if things look too big.",
                    &[("native", &format!("{native:.3}")), ("drawn", &format!("{:.3}", ui.ctx().pixels_per_point()))],
                ))
                .weak()
                .small(),
            );
        }
    }

    /// Settings › About: version and updates, help, other devices and links.
    fn settings_about(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("RustDo {}", update::CURRENT_VERSION)).strong());
            if ui.button(tr("What's new")).clicked() {
                self.whats_new = Some(changelog::releases());
                self.close_settings(ui);
            }
        });
        ui.separator();
        if cfg!(target_arch = "wasm32") {
            ui.label(RichText::new(tr("The web app updates itself automatically.")).weak());
        } else if store_package::is_packaged() {
            ui.label(RichText::new(tr("Updates come from the Microsoft Store.")).weak());
        } else {
            if ui.checkbox(&mut self.store.settings.check_updates, tr("Check for updates daily")).changed() {
                self.dirty = true;
            }
            if cfg!(windows) {
                let installed = update::is_installed_copy();
                let r = ui.add_enabled(
                    installed,
                    egui::Checkbox::new(&mut self.store.settings.auto_update, tr("Install updates automatically")),
                );
                if r.changed() {
                    self.dirty = true;
                }
                let note = if installed {
                    tr("Checks each time RustDo opens and updates itself silently, then reopens.")
                } else {
                    tr("Only for the installed version (this is the portable exe).")
                };
                ui.label(RichText::new(note).weak().small());
            }
            ui.label(
                RichText::new(tr("Asks GitHub for the latest version. No task data is sent."))
                    .weak()
                    .small(),
            );
            let checking = self.update_rx.is_some();
            if ui.add_enabled(!checking, egui::Button::new(tr("Check now"))).clicked() {
                self.update_manual = true;
                self.update_available = None;
                self.close_settings(ui);
            }
        }
        self.help_menu_items(ui);
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
        if ui.button(tr("⬇ Get RustDo for your other devices…")).on_hover_text(tr("Windows, Android, Linux, the web app and the Hub")).clicked() {
            self.get_apps = true;
            ui.close_menu();
        }
        if !cfg!(target_arch = "wasm32") {
            ui.hyperlink_to(tr("Downloads & release notes"), releases);
        }
        ui.hyperlink_to(tr("Join the RustDo Discord"), update::DISCORD)
            .on_hover_text(tr("Chat, ideas and help: opens in your browser"));
        ui.hyperlink_to(tr("Project page"), format!("https://github.com/{}", update::REPO));
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
        // Start (or wake, or stop) the background companion and keep its sign-in entry current.
        if companion::SUPPORTED {
            let settings = self.store.settings.clone();
            std::thread::spawn(move || companion::ensure(&settings));
        } else if cfg!(windows) {
            notify::register_app();
        }
    }

    /// Settings › Help (desktop): the log, and a summary to paste into a problem report.
    fn help_menu_items(&mut self, ui: &mut egui::Ui) {
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        {
            ui.separator();
            ui.label(RichText::new(tr("Help")).strong());
            ui.add(
                egui::Label::new(
                    RichText::new(tr("Something not working? Copy the diagnostic info and send it with your report."))
                        .weak()
                        .small(),
                )
                .wrap(),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("Copy diagnostic info")).on_hover_text(tr("Version, system and the last lines of the log")).clicked() {
                    platform::copy_text(ui.ctx(), diagnostic_report());
                    self.close_settings(ui);
                    self.notify(tr("Diagnostic info copied. Paste it into your message or the Discord."));
                }
                if let Some(dir) = diagnostics::log_dir() {
                    if ui.button(tr("Open logs folder")).on_hover_text(dir.display().to_string()).clicked() {
                        let _ = std::fs::create_dir_all(&dir);
                        self.close_settings(ui);
                        platform::open_folder(&dir);
                    }
                }
            });
        }
        let _ = ui;
    }

    /// Desktop: once a second, see whether the companion runs, pick up notes it saved, and
    /// come to the front if its tray icon's "Open RustDo" asked.
    fn companion_tick(&mut self, ctx: &egui::Context) {
        if !companion::SUPPORTED {
            return;
        }
        if self.companion_checked.is_some_and(|t| t.elapsed() < StdDuration::from_secs(1)) {
            return;
        }
        self.companion_checked = Some(Instant::now());
        self.companion_running = companion::is_running();
        notes_ui::lock(&self.notes).reload_if_changed();
        if companion::take_quit_app_request() {
            // The tray's "Quit RustDo completely".
            self.closing = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if companion::take_show_app_request() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        ctx.request_repaint_after(StdDuration::from_secs(1));
    }

    /// Desktop: the first time the window is closed while the background helper keeps
    /// running, say where RustDo went (the tray) and offer to quit completely.
    fn close_hint_check(&mut self, ctx: &egui::Context) {
        if !companion::SUPPORTED || self.closing {
            return;
        }
        let s = &self.store.settings;
        if ctx.input(|i| i.viewport().close_requested()) && companion::wanted(s) && !s.close_hint_seen {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_hint = true;
        }
        if !self.close_hint {
            return;
        }
        let what = if s.companion && companion::reminders_when_closed(s) {
            tr("Your sticky notes stay on the desktop and reminders keep coming.")
        } else if s.companion {
            tr("Your sticky notes stay on the desktop.")
        } else {
            tr("Reminders keep coming.")
        };
        let mut choice = None;
        dialog_window(ctx, tr("RustDo keeps running in the tray"), self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 420.0));
            ui.add(egui::Label::new(what).wrap());
            ui.add(egui::Label::new(tr("Find it by the RustDo icon near the clock: it opens RustDo again or quits it completely. You can change this in Settings › Reminders.")).wrap());
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add(egui::Button::new(RichText::new(tr("OK, close the window")).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                    choice = Some(false);
                }
                if ui.button(tr("Quit RustDo completely")).clicked() {
                    choice = Some(true);
                }
            });
        });
        if let Some(quit) = choice {
            self.close_hint = false;
            self.closing = true;
            self.store.settings.close_hint_seen = true;
            self.persist();
            if quit {
                companion::request_quit();
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Settings › Reminders › When RustDo is closed (desktop): the companion and its tray icon.
    fn companion_menu_items(&mut self, ui: &mut egui::Ui) {
        if !companion::SUPPORTED {
            return;
        }
        ui.label(RichText::new(tr("When RustDo is closed")).strong());
        let s = &mut self.store.settings;
        let mut changed = ui
            .checkbox(&mut s.companion, tr("Keep sticky notes running in the background"))
            .on_hover_text(tr("A tray icon keeps your sticky notes on the desktop after you close RustDo."))
            .changed();
        let sign_in = if cfg!(windows) { tr("Start with Windows") } else { tr("Start when I sign in") };
        changed |= ui
            .add_enabled(s.companion, egui::Checkbox::new(&mut s.companion_autostart, sign_in))
            .changed();
        if cfg!(windows) {
            changed |= ui
                .add_enabled(s.notifications, egui::Checkbox::new(&mut s.background_reminders, tr("Remind me even when RustDo is closed")))
                .on_hover_text(tr("RustDo keeps running in the tray to show reminders."))
                .changed();
        }
        let note = if !s.companion {
            tr("Sticky notes show only while RustDo is open.")
        } else if cfg!(windows) {
            tr("The RustDo tray icon (by the clock) opens RustDo, adds a note, or shows and hides notes. Reminders keep arriving after you close RustDo.")
        } else {
            tr("The RustDo tray icon opens RustDo, adds a note, or shows and hides notes. GNOME needs the AppIndicator extension to show tray icons.")
        };
        ui.add(egui::Label::new(RichText::new(note).weak().small()).wrap());
        if changed {
            self.dirty = true;
            self.helper_handled = false; // start, wake or stop the companion
        }
        if self.companion_running && ui.button(tr("Quit RustDo completely")).on_hover_text(tr("Closes this window and the tray icon. RustDo starts again when you open it.")).clicked() {
            companion::request_quit();
            self.closing = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ------------------------------------------------------------------ backgrounds

    /// The background for what's on screen: the open list's own, else the app's. Pages
    /// meant for reading (Settings on a phone, Show list) stay plain.
    fn current_background(&self) -> Background {
        if self.show_list.is_some() || self.show_settings {
            return Background::None;
        }
        if let (View::List(id), false) = (self.view, self.notes_page) {
            if let Some(b) = self.store.settings.list_backgrounds.get(&id) {
                return b.clone();
            }
        }
        self.store.settings.background.clone()
    }

    /// The texture for a background (made once, then kept).
    fn bg_texture(&mut self, ctx: &egui::Context, bg: &Background) -> Option<egui::TextureHandle> {
        let (key, image) = match bg {
            Background::None => return None,
            Background::Builtin(id) => {
                let key = format!("builtin:{id}");
                if let Some(t) = self.bg_textures.get(&key) {
                    return Some(t.clone());
                }
                let b = backgrounds::builtin(id)?;
                let (w, h) = (480, 300);
                (key, egui::ColorImage::from_rgba_unmultiplied([w, h], &backgrounds::render_builtin(b, w, h)))
            }
            Background::Custom(id) => {
                let key = format!("custom:{id}");
                if let Some(t) = self.bg_textures.get(&key) {
                    return Some(t.clone());
                }
                let (w, h, px) = backgrounds::decode(self.bg_images.get(id)?)?;
                (key, egui::ColorImage::from_rgba_unmultiplied([w, h], &px))
            }
        };
        let texture = ctx.load_texture(&key, image, egui::TextureOptions::LINEAR);
        self.bg_textures.insert(key, texture.clone());
        Some(texture)
    }

    /// Draws the background behind everything, toned down by the Dim setting. Returns
    /// whether one is showing.
    fn paint_background(&mut self, ctx: &egui::Context) -> bool {
        let bg = self.current_background();
        let Some(texture) = self.bg_texture(ctx, &bg) else { return false };
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.image(texture.id(), screen, cover_uv(texture.size_vec2(), screen.size()), Color32::WHITE);
        let base = if ctx.style().visuals.dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() }.panel_fill;
        let dim = (self.store.settings.background_dim.clamp(0.0, 0.9) * 255.0) as u8;
        painter.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), dim));
        true
    }

    /// Over a background: panels let it show through and task rows become see-through
    /// cards; without one, the normal look.
    fn apply_background_style(&mut self, ctx: &egui::Context, showing: bool) {
        let dark = ctx.style().visuals.dark_mode;
        if self.bg_style == Some((dark, showing)) {
            return;
        }
        self.bg_style = Some((dark, showing));
        let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        if showing {
            let p = v.panel_fill;
            v.panel_fill = Color32::TRANSPARENT;
            v.faint_bg_color = Color32::from_rgba_unmultiplied(p.r(), p.g(), p.b(), 220);
            // Text stays readable over any picture: normal text gets full contrast, and
            // faint text (dates, hints), which egui fades towards this colour, stays
            // clearly lighter (dark theme) or darker (light theme) than the picture.
            let w = &mut v.widgets.noninteractive;
            if dark {
                w.fg_stroke.color = Color32::from_gray(235);
                w.weak_bg_fill = Color32::from_gray(150);
            } else {
                w.fg_stroke.color = Color32::from_gray(15);
                w.weak_bg_fill = Color32::from_gray(110);
            }
        }
        ctx.set_visuals(v);
    }

    /// The frame for the side panels (menu, details): a little see-through over a background.
    fn side_frame(&self, ctx: &egui::Context) -> egui::Frame {
        let frame = egui::Frame::side_top_panel(&ctx.style());
        if self.bg_style.is_some_and(|(_, showing)| showing) {
            let p = if ctx.style().visuals.dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() }.panel_fill;
            frame.fill(Color32::from_rgba_unmultiplied(p.r(), p.g(), p.b(), 200))
        } else {
            frame
        }
    }

    /// Sets the app-wide background (`target` None) or a list's (`Some(list)`; `None` value =
    /// use the app background).
    fn set_background(&mut self, target: Option<Uuid>, value: Option<Background>) {
        match (target, value) {
            (None, value) => self.store.settings.background = value.unwrap_or_default(),
            (Some(list), Some(value)) => {
                self.store.settings.list_backgrounds.insert(list, value);
            }
            (Some(list), None) => {
                self.store.settings.list_backgrounds.remove(&list);
            }
        }
        self.dirty = true;
    }

    /// Stores a picked picture (shrunk to a JPEG copy) and uses it.
    fn add_picture(&mut self, target: PicFor, bytes: &[u8]) {
        match backgrounds::prepare(bytes) {
            Ok(jpeg) => {
                let id = backgrounds::image_id(&jpeg);
                if let Err(e) = backgrounds::save(self.storage.folder().as_deref(), &id, &jpeg) {
                    self.notify(e);
                    return;
                }
                self.bg_images.insert(id.clone(), jpeg);
                let picture = Background::Custom(id);
                match target {
                    PicFor::App => self.set_background(None, Some(picture)),
                    PicFor::List(list) => self.set_background(Some(list), Some(picture)),
                    PicFor::Note(note) => {
                        let mut s = notes_ui::lock(&self.notes);
                        if let Some(n) = s.notes.get_mut(note) {
                            n.picture = Some(picture);
                            s.changed();
                        }
                    }
                }
            }
            Err(e) => self.notify(e),
        }
    }

    /// Picks up pictures stored by another RustDo process (a sticky note's picture added
    /// in the companion), so they're shown and synced too. Checked every few seconds.
    fn scan_pictures(&mut self) {
        if self.bg_scanned.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(4)) {
            return;
        }
        self.bg_scanned = Some(Instant::now());
        let folder = self.storage.folder();
        for id in backgrounds::list_ids(folder.as_deref()) {
            if let std::collections::btree_map::Entry::Vacant(slot) = self.bg_images.entry(id) {
                if let Some(jpeg) = backgrounds::load_one(folder.as_deref(), slot.key()) {
                    slot.insert(jpeg);
                }
            }
        }
    }

    /// Removes a picture from this device (and, through sync, your others).
    fn remove_picture(&mut self, id: &str) {
        self.bg_images.remove(id);
        backgrounds::delete(self.storage.folder().as_deref(), id);
        self.bg_textures.retain(|k, _| !k.ends_with(id));
        let gone = Background::Custom(id.to_string());
        if self.store.settings.background == gone {
            self.store.settings.background = Background::None;
        }
        self.store.settings.list_backgrounds.retain(|_, b| *b != gone);
        self.dirty = true;
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn pick_picture(&mut self, _ctx: &egui::Context, target: PicFor) {
        let picked = rfd::FileDialog::new()
            .set_title(tr("Pick a picture"))
            .add_filter(tr("Pictures"), &["jpg", "jpeg", "png"])
            .pick_file();
        if let Some(path) = picked {
            match std::fs::read(&path) {
                Ok(bytes) => self.add_picture(target, &bytes),
                Err(e) => self.notify(trf("Couldn't read {path}: {e}", &[("path", &path.display()), ("e", &e)])),
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_picture(&mut self, ctx: &egui::Context, target: PicFor) {
        let slot = self.bg_web_pick.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let file = rfd::AsyncFileDialog::new().add_filter(tr("Pictures"), &["jpg", "jpeg", "png"]).pick_file().await;
            if let Some(file) = file {
                *slot.borrow_mut() = Some((target, file.read().await));
                ctx.request_repaint();
            }
        });
    }

    #[cfg(target_os = "android")]
    fn pick_picture(&mut self, _ctx: &egui::Context, target: PicFor) {
        self.photo_picker = Some(crate::photos::PhotoPicker::new(target));
    }

    /// A list's icon, the same in the menu and on its page: 👥 shared, 🏠 the default list,
    /// 📋 any other (not ☰, which is the menu button beside it on phones).
    fn list_icon(&self, id: Uuid) -> &'static str {
        if self.store.shares.iter().any(|s| s.list_id == id) {
            "👥"
        } else if id == self.store.default_list {
            "🏠"
        } else {
            LIST_ICON
        }
    }

    /// Android: RustDo's own photo picker, while it's open.
    #[cfg(target_os = "android")]
    fn photo_picker_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.photo_picker.take() else { return };
        let mut outcome = crate::photos::Outcome::Open;
        dialog_window(ctx, tr("Pick a picture"), self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 480.0));
            outcome = picker.ui(ui);
        });
        match outcome {
            crate::photos::Outcome::Open => self.photo_picker = Some(picker),
            crate::photos::Outcome::Cancelled => {}
            crate::photos::Outcome::Picked(bytes) => self.add_picture(picker.target, &bytes),
            crate::photos::Outcome::Failed(e) => self.notify(e),
        }
    }

    /// The background choices as swatches: for the app (`target` None) or one list.
    fn background_picker(&mut self, ui: &mut egui::Ui, target: Option<Uuid>) {
        let current: Option<Background> = match target {
            None => Some(self.store.settings.background.clone()),
            Some(id) => self.store.settings.list_backgrounds.get(&id).cloned(),
        };
        // (value, name, texture): None value = "use the app background".
        let mut choices: Vec<(Option<Background>, String, Option<egui::TextureHandle>)> = Vec::new();
        if target.is_some() {
            let app = self.store.settings.background.clone();
            let tex = self.bg_texture(ui.ctx(), &app);
            choices.push((None, tr("App background").to_string(), tex));
        }
        choices.push((Some(Background::None), tr("Plain").to_string(), None));
        for b in &backgrounds::BUILTINS {
            let bg = Background::Builtin(b.id.to_string());
            let tex = self.bg_texture(ui.ctx(), &bg);
            choices.push((Some(bg), backgrounds::builtin_name(b.id).to_string(), tex));
        }
        let custom: Vec<String> = self.bg_images.keys().cloned().collect();
        for id in custom {
            let bg = Background::Custom(id);
            let tex = self.bg_texture(ui.ctx(), &bg);
            choices.push((Some(bg), tr("Your picture").to_string(), tex));
        }
        let touch = is_touch(ui);
        let size = if touch { Vec2::new(84.0, 56.0) } else { Vec2::new(68.0, 44.0) };
        let mut chosen: Option<Option<Background>> = None;
        let mut remove: Option<String> = None;
        let mut pick = false;
        ui.horizontal_wrapped(|ui| {
            for (value, name, tex) in &choices {
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                let p = ui.painter();
                let corner = egui::CornerRadius::same(6);
                match tex {
                    Some(t) => {
                        p.add(egui::epaint::RectShape::filled(rect, corner, Color32::WHITE).with_texture(t.id(), cover_uv(t.size_vec2(), rect.size())));
                    }
                    None => {
                        p.rect_filled(rect, corner, ui.visuals().extreme_bg_color);
                    }
                }
                if value.is_none() {
                    p.text(rect.center(), Align2::CENTER_CENTER, tr("App"), FontId::proportional(12.0), ui.visuals().strong_text_color());
                } else if *value == Some(Background::None) {
                    p.text(rect.center(), Align2::CENTER_CENTER, tr("Plain"), FontId::proportional(12.0), ui.visuals().text_color());
                }
                let selected = *value == current;
                let stroke = if selected { Stroke::new(3.0_f32, ACCENT) } else { Stroke::new(1.0_f32, ui.visuals().weak_text_color()) };
                p.rect_stroke(rect, corner, stroke, egui::StrokeKind::Inside);
                let resp = resp.on_hover_text(name.as_str()).on_hover_cursor(CursorIcon::PointingHand);
                if resp.clicked() {
                    chosen = Some(value.clone());
                }
                if let Some(Background::Custom(id)) = value {
                    resp.context_menu(|ui| {
                        if ui.button(tr("🗑  Remove picture")).clicked() {
                            remove = Some(id.clone());
                            ui.close_menu();
                        }
                    });
                }
            }
            if (CAN_IMPORT_FILE || cfg!(target_os = "android")) && ui.add(egui::Button::new(tr("+ Picture…")).min_size(size)).on_hover_text(tr("Use your own picture (JPG or PNG)")).clicked() {
                pick = true;
            }
        });
        if let Some(value) = chosen {
            self.set_background(target, value);
        }
        if let Some(id) = remove {
            self.remove_picture(&id);
        }
        if pick {
            let ctx = ui.ctx().clone();
            self.pick_picture(&ctx, target.map_or(PicFor::App, PicFor::List));
        }
    }

    /// "Background…" for one list.
    fn background_dialog(&mut self, ctx: &egui::Context) {
        let Some(list) = self.bg_dialog else { return };
        if !self.store.lists.iter().any(|l| l.id == list) {
            self.bg_dialog = None;
            return;
        }
        let name = self.store.list_name(list).to_string();
        let mut close = false;
        dialog_window(ctx, trf("Background for “{name}”", &[("name", &name)]), self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 480.0));
            self.background_picker(ui, Some(list));
            ui.add_space(6.0);
            if ui.button(tr("Done")).clicked() {
                close = true;
            }
        });
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.bg_dialog = None;
        }
    }

    // ------------------------------------------------------------------ show list

    /// The view's open tasks, large and plain, to read or hand over; copy as text or print.
    fn show_list_page(&mut self, ctx: &egui::Context, view: View, today: NaiveDate) {
        let title = match view {
            View::MyDay => tr("My Day").to_string(),
            View::Important => tr("Important").to_string(),
            View::Planned => tr("Planned").to_string(),
            View::All => tr("All").to_string(),
            View::Completed => tr("Completed").to_string(),
            View::List(id) => self.store.list_name(id).to_string(),
        };
        let page = showlist::build(&self.store, view, &title, today);
        let mut close = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if back_button(ui, tr("Back")).clicked() {
                    close = true;
                }
                if ui.button(tr("📋 Copy as text")).clicked() {
                    platform::copy_text(ui.ctx(), showlist::to_text(&page));
                    self.notify(tr("Copied. Paste it into an email or a message."));
                }
                if !cfg!(target_os = "android") && ui.button(tr("Print or save as PDF")).clicked() {
                    match platform::open_printable(self.storage.folder(), &showlist::file_name(&title), &showlist::to_html(&page)) {
                        Ok(()) => self.notify(tr("Opened in your browser: use its Print button (it can also save a PDF).")),
                        Err(e) => self.notify(e),
                    }
                }
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(760.0));
                ui.label(RichText::new(&page.title).size(30.0).strong());
                ui.label(RichText::new(&page.date).size(16.0).weak());
                ui.add_space(10.0);
                if page.items.is_empty() {
                    ui.label(RichText::new(tr("Nothing in this list.")).size(18.0).weak());
                }
                for item in &page.items {
                    ui.separator();
                    ui.add(egui::Label::new(RichText::new(&item.title).size(22.0).strong()).wrap());
                    if !item.details.is_empty() {
                        ui.label(RichText::new(item.details.join(" · ")).size(16.0).weak());
                    }
                    if !item.notes.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&item.notes).size(18.0)).wrap());
                    }
                    for (step, done) in &item.steps {
                        ui.label(RichText::new(format!("{} {step}", if *done { "☑" } else { "☐" })).size(18.0));
                    }
                    ui.add_space(4.0);
                }
                ui.add_space(24.0);
            });
        });
        if close {
            self.show_list = None;
        }
    }

    // ------------------------------------------------------------------ Microsoft To Do import

    fn ms_import_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.ms_import.as_mut() else { return };
        // What the sign-in sent.
        let mut download = None;
        if let Some(session) = &dialog.session {
            while let Ok(event) = session.rx.try_recv() {
                match event {
                    mstodo::Event::Code { user_code, url } => {
                        dialog.code = Some((user_code, url));
                        dialog.status = None;
                    }
                    mstodo::Event::Status(s) => dialog.status = Some(s),
                    mstodo::Event::Done(result) => download = Some(result),
                }
            }
        }
        match download {
            Some(Ok(json)) => {
                self.ms_import = None;
                match import::parse_download(&json) {
                    Ok(import) => {
                        self.pending_import = Some(PendingImport { file_name: "Microsoft To Do".into(), import, include_completed: false })
                    }
                    Err(e) => self.notify(e),
                }
                return;
            }
            Some(Err(e)) => {
                dialog.session = None;
                dialog.code = None;
                dialog.status = None;
                // An empty error: cancelled here.
                dialog.error = (!e.is_empty()).then_some(e);
            }
            None => {}
        }

        let mut close = false;
        let mut sign_in = false;
        let mut from_file = false;
        dialog_window(ctx, tr("Import from Microsoft To Do"), self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 440.0));
            if let Some((code, url)) = &dialog.code {
                ui.label(tr("1. Open the Microsoft sign-in page:"));
                if ui.link(url.as_str()).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
                ui.add_space(4.0);
                ui.label(tr("2. Enter this code and sign in with the account you use for To Do:"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(code).monospace().size(24.0).strong());
                    copy_button(ui, tr("📋 Copy code"), || code.clone());
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(dialog.status.as_deref().unwrap_or(tr("Waiting for you to sign in…"))).weak());
                });
            } else if dialog.session.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(dialog.status.as_deref().unwrap_or(tr("Connecting to Microsoft…")));
                });
            } else {
                ui.add(
                    egui::Label::new(tr(
                        "Bring your lists, tasks, steps, notes, due dates and reminders over from Microsoft To Do. RustDo only reads them: nothing in To Do is changed.",
                    ))
                    .wrap(),
                );
                ui.add_space(6.0);
                if let Some(e) = &dialog.error {
                    ui.label(RichText::new(e).color(DANGER).small());
                    ui.add_space(4.0);
                }
                if ui.add(egui::Button::new(RichText::new(tr("Sign in with Microsoft")).strong())).clicked() {
                    sign_in = true;
                }
                ui.label(
                    RichText::new(tr("Works with personal Microsoft accounts (Outlook.com) and work or school accounts."))
                        .weak()
                        .small(),
                );
                if CAN_IMPORT_FILE {
                    ui.add_space(6.0);
                    if ui
                        .button(tr("From an export file…"))
                        .on_hover_text(tr("Import a Microsoft To Do export (.json) or an Outlook tasks export (.csv).\nYou can also drop the file onto this window."))
                        .clicked()
                    {
                        from_file = true;
                    }
                }
            }
            ui.add_space(6.0);
            if ui.button(tr("Cancel")).clicked() {
                close = true;
            }
        });
        if sign_in {
            dialog.error = None;
            dialog.session = Some(mstodo::start(ctx));
        }
        if close || from_file {
            if let Some(s) = &dialog.session {
                s.cancel();
            }
            self.ms_import = None;
        }
        if from_file {
            self.pick_import_file(ctx);
        }
    }

    // ------------------------------------------------------------------ sync my devices

    fn account_menu_items(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(tr("Sync my devices")).strong());
        match &self.store.account {
            None => {
                ui.label(
                    RichText::new(tr("Keep your lists, tasks and sticky notes the same on all your devices, through your own RustDo Hub."))
                        .weak()
                        .small(),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("Turn on sync…")).clicked() {
                        self.account_dialog = Some(AccountDialog {
                            hub: self.store.settings.hub.clone().unwrap_or_default(),
                            join: false,
                            key: String::new(),
                            busy: false,
                            error: None,
                            show_key: None,
                        });
                        self.close_settings(ui);
                    }
                });
            }
            Some(acc) => {
                let (text, color) = match &self.account_error {
                    Some(err) => (trf("Can't sync right now: {err}", &[("err", &err)]), DANGER),
                    None => (
                        format!(
                            "{} · {} · {}",
                            tr("On"),
                            acc.hub.trim_start_matches("https://").trim_start_matches("http://"),
                            acc.last_sync.map_or(tr("not synced yet").into(), |t| trf("last synced {time}", &[("time", &format_time(t, self.store.settings.clock_24h))]))
                        ),
                        ui.visuals().weak_text_color(),
                    ),
                };
                ui.label(RichText::new(text).color(color).small());
                let (hub, key) = (acc.hub.clone(), acc.key.clone());
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("Sync now")).clicked() {
                        self.account_last = None;
                    }
                    if ui.button(tr("Sync key…")).on_hover_text(tr("Show the key to turn on sync on another device")).clicked() {
                        self.account_dialog = Some(AccountDialog { hub, join: true, key: String::new(), busy: false, error: None, show_key: Some(key) });
                        self.close_settings(ui);
                    }
                    if ui.button(tr("Turn off")).on_hover_text(tr("Stop syncing this device. Your tasks and notes stay here.")).clicked() {
                        self.store.account = None;
                        self.account_error = None;
                        self.dirty = true;
                        self.notify(tr("Sync is off on this device. Your tasks and notes stay here."));
                    }
                });
            }
        }
        if ui.button(tr("❓ How syncing and sharing work")).clicked() {
            self.hub_guide = true;
            self.close_settings(ui);
        }
    }

    /// Collects a finished sync round and starts the next: every 30 seconds, or a couple
    /// of seconds after a change here.
    fn account_tick(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.account_rx {
            match rx.try_recv() {
                Ok(done) => {
                    self.account_rx = None;
                    let theme = self.store.settings.theme;
                    self.account_done(done);
                    if self.store.settings.theme != theme {
                        apply_theme(ctx, self.store.settings.theme);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.account_rx = None,
            }
        }
        if self.account_rx.is_some() {
            return;
        }
        let Some(acc) = &self.store.account else { return };
        // Notes edited since the last round count as a change too.
        if notes::AVAILABLE {
            let changed = notes_ui::lock(&self.notes).last_change();
            if self.account_notes_seen.is_none_or(|seen| changed > seen) {
                self.account_notes_seen = Some(changed);
                self.account_soon = true;
            }
        }
        let since = self.account_last.map(|t| t.elapsed());
        let due = since.is_none_or(|e| e >= StdDuration::from_secs(30) || (self.account_soon && e >= StdDuration::from_secs(2)));
        if !due {
            return;
        }
        self.account_soon = false;
        self.account_last = Some(Instant::now());
        let now_ms = chrono::Utc::now().timestamp_millis();
        let out = if notes::AVAILABLE {
            let s = notes_ui::lock(&self.notes);
            account::outgoing(&self.store, Some(&s.notes), Some(&self.bg_images), acc, now_ms)
        } else {
            account::outgoing(&self.store, None, Some(&self.bg_images), acc, now_ms)
        };
        self.account_rx = Some(account::sync(&acc.hub, out, ctx));
    }

    fn account_done(&mut self, done: account::Done) {
        match done {
            account::Done::Created { hub, result } => {
                let Some(dialog) = self.account_dialog.as_mut() else { return };
                dialog.busy = false;
                match result {
                    Ok(key) => {
                        dialog.show_key = Some(key.clone());
                        self.store.account = Some(account::Account::new(hub.clone(), key));
                        self.store.settings.hub = Some(hub);
                        self.account_error = None;
                        self.account_last = None;
                        self.dirty = true;
                    }
                    Err(e) => dialog.error = Some(e),
                }
            }
            account::Done::Synced(out, result) => {
                let Some(mut acc) = self.store.account.take().filter(|a| a.key == out.request.key) else { return };
                let joining = self.account_dialog.as_ref().is_some_and(|d| d.busy);
                match result {
                    Ok(resp) => {
                        if acc.pull_only {
                            let fresh = account::is_fresh(&self.store, (notes::AVAILABLE).then(|| notes_ui::lock(&self.notes).notes.clone()).as_ref());
                            if fresh {
                                account::clear_for_pull(&mut self.store);
                            } else {
                                acc.pull_only = false;
                            }
                        }
                        let applied = if notes::AVAILABLE {
                            let mut s = notes_ui::lock(&self.notes);
                            let applied = account::apply(&mut self.store, Some(&mut s.notes), Some(&mut self.bg_images), &mut acc, &out, &resp);
                            if applied.notes > 0 {
                                s.changed();
                                s.save_if_due(true);
                            }
                            self.account_notes_seen = Some(s.last_change());
                            applied
                        } else {
                            account::apply(&mut self.store, None, Some(&mut self.bg_images), &mut acc, &out, &resp)
                        };
                        // Background pictures that came or went: keep the device's copies in step.
                        let folder = self.storage.folder();
                        for id in &applied.images_saved {
                            if let Some(jpeg) = self.bg_images.get(id) {
                                if let Err(e) = backgrounds::save(folder.as_deref(), id, jpeg) {
                                    log::warn!(target: "rustdo", "couldn't store a synced background: {e}");
                                }
                            }
                            self.bg_textures.retain(|k, _| !k.ends_with(id.as_str()));
                        }
                        for id in &applied.images_removed {
                            backgrounds::delete(folder.as_deref(), id);
                            self.bg_textures.retain(|k, _| !k.ends_with(id.as_str()));
                            self.bg_forget.push(id.clone());
                        }
                        acc.last_sync = Some(Local::now().naive_local());
                        self.store.account = Some(acc);
                        self.account_error = None;
                        if self.selected.is_some_and(|id| self.store.task(id).is_none()) {
                            self.selected = None;
                        }
                        if let View::List(id) = self.view {
                            if !self.store.lists.iter().any(|l| l.id == id) {
                                self.view = View::MyDay;
                            }
                        }
                        if applied.joined > 0 {
                            // Fetch the shared lists' tasks straight away.
                            self.last_sync_at = None;
                        }
                        self.dirty = true;
                        if joining {
                            self.account_dialog = None;
                            self.notify(tr("Sync is on. This device now has your lists and notes."));
                        }
                    }
                    Err(e) => {
                        if joining {
                            // The key or hub didn't work: this device isn't linked after all.
                            if let Some(d) = self.account_dialog.as_mut() {
                                d.busy = false;
                                d.error = Some(e);
                            }
                            self.dirty = true;
                        } else {
                            self.account_error = Some(e);
                            self.store.account = Some(acc);
                        }
                    }
                }
            }
        }
    }

    fn account_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.account_dialog.as_mut() else { return };
        let mut close = false;
        let mut submit = false;
        let mut guide = false;
        let title = if dialog.show_key.is_some() { tr("Your sync key") } else { tr("Turn on sync") };
        dialog_window(ctx, title, self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 440.0));
            if let Some(key) = &dialog.show_key {
                ui.add(egui::Label::new(tr("To sync another device, open Settings there, choose Turn on sync, pick \"I already have a sync key\", and enter the hub address and this key.")).wrap());
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Hub address")).small());
                ui.label(RichText::new(&dialog.hub).monospace());
                ui.label(RichText::new(tr("Sync key")).small());
                ui.label(RichText::new(key).monospace().size(20.0).strong());
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(tr("Keep it private: anyone who has this key and can reach your hub can see and change your tasks and notes."))
                            .weak()
                            .small(),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    copy_button(ui, tr("📋 Copy key"), || key.clone());
                    if ui.button(tr("Done")).clicked() {
                        close = true;
                    }
                });
                return;
            }
            ui.add(egui::Label::new(tr("Your lists, tasks, sticky notes and settings stay the same on every device that uses the same RustDo Hub and sync key. Lists you share with family keep their own codes.")).wrap());
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Hub address")).small());
            field_with_paste(ui, &mut dialog.hub, tr("e.g. https://todo.example.com or 192.168.1.20"), "sync_hub");
            ui.add_space(4.0);
            ui.radio_value(&mut dialog.join, false, tr("This is my first device: start syncing"));
            ui.radio_value(&mut dialog.join, true, tr("I already have a sync key"));
            if dialog.join {
                field_with_paste(ui, &mut dialog.key, tr("ABCDE-FGHJK-MNPQR-STUVW"), "sync_key");
            }
            if let Some(err) = &dialog.error {
                ui.label(RichText::new(err).color(DANGER).small());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!dialog.busy, egui::Button::new(tr("Turn on sync"))).clicked() {
                    submit = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
                if dialog.busy {
                    ui.spinner();
                }
            });
            if ui.add(egui::Button::new(RichText::new(tr("No hub yet? How syncing works")).small()).frame(false)).clicked() {
                guide = true;
            }
        });
        if guide {
            self.hub_guide = true;
        }
        if close {
            // Cancelled while joining: don't stay half-linked.
            if dialog.busy && dialog.show_key.is_none() {
                self.store.account = None;
                self.account_rx = None;
            }
            self.account_dialog = None;
            return;
        }
        if !submit || dialog.busy {
            return;
        }
        dialog.error = None;
        let Some(hub) = sync::normalize_hub(&dialog.hub) else {
            dialog.error = Some(tr("Enter the hub's address, e.g. https://todo.example.com or 192.168.1.20").into());
            return;
        };
        dialog.hub = hub.clone();
        if dialog.join {
            let Some(key) = account::normalize_key(&dialog.key) else {
                dialog.error = Some(tr("A sync key has 20 letters and numbers, like ABCDE-FGHJK-MNPQR-STUVW.").into());
                return;
            };
            let fresh = account::is_fresh(&self.store, (notes::AVAILABLE).then(|| notes_ui::lock(&self.notes).notes.clone()).as_ref());
            let mut acc = account::Account::new(hub.clone(), key);
            // An empty device takes the account as it is; otherwise both are combined.
            acc.pull_only = fresh;
            self.store.account = Some(acc);
            self.store.settings.hub = Some(hub);
            self.account_rx = None;
            self.account_last = None;
            self.account_error = None;
            dialog.busy = true;
        } else {
            dialog.busy = true;
            self.account_rx = Some(account::create(hub, ctx));
        }
    }

    // ------------------------------------------------------------------ family sharing

    fn sharing_menu_items(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(tr("Family sharing")).strong());
        if !sync::SUPPORTED {
            ui.label(
                RichText::new(tr("Sharing lists through a RustDo Hub works in the Windows, Linux and Android apps."))
                    .weak()
                    .small(),
            );
            ui.separator();
            return;
        }
        let shared = self.store.shares.len();
        let status = if shared == 0 {
            // No arrows here: egui's built-in fonts have no "→" and draw a box instead.
            let how = if is_touch(ui) { tr("long-press it and choose Share list") } else { tr("right-click it and choose Share list") };
            trf("Share a list with people at home: {how}. Needs a RustDo Hub on your network.", &[("how", &how)])
        } else if let Some(err) = &self.sync_error {
            trf("Can't sync right now: {err}", &[("err", &err)])
        } else {
            let last = self.store.shares.iter().filter_map(|s| s.last_sync).max();
            format!(
                "{} · {}",
                plural(shared, "{n} shared list", "{n} shared lists"),
                last.map_or(tr("not synced yet").into(), |t| trf("last synced {time}", &[("time", &format_time(t, self.store.settings.clock_24h))]))
            )
        };
        let color = if self.sync_error.is_some() && shared > 0 { DANGER } else { ui.visuals().weak_text_color() };
        ui.label(RichText::new(status).color(color).small());
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Join a shared list…")).clicked() {
                self.open_share_dialog(None);
                self.close_settings(ui);
            }
            if shared > 0 && ui.button(tr("Sync now")).clicked() {
                self.last_sync_at = None;
            }
        });
        if ui.button(tr("❓ How to share a list")).clicked() {
            self.hub_guide = true;
            self.close_settings(ui);
        }
    }

    fn open_share_dialog(&mut self, list: Option<Uuid>) {
        self.share_dialog = Some(ShareDialog {
            list,
            hub: self.store.settings.hub.clone().unwrap_or_default(),
            code: String::new(),
            busy: false,
            error: None,
        });
    }

    /// Collects finished hub requests and starts the next round: every 30 seconds, or a
    /// couple of seconds after a change.
    fn sync_tick(&mut self, ctx: &egui::Context) {
        if !sync::SUPPORTED {
            return;
        }
        if let Some(rx) = &self.sync_rx {
            match rx.try_recv() {
                Ok(done) => {
                    self.sync_rx = None;
                    for d in done {
                        self.sync_done(d);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.sync_rx = None,
            }
        }
        if self.sync_rx.is_some() {
            return;
        }
        let since = self.last_sync_at.map(|t| t.elapsed());
        let due = !self.sync_jobs.is_empty()
            || (!self.store.shares.is_empty()
                && since.is_none_or(|e| e >= StdDuration::from_secs(30) || (self.sync_soon && e >= StdDuration::from_secs(2))));
        if !due {
            return;
        }
        self.sync_soon = false;
        self.last_sync_at = Some(Instant::now());
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut jobs = std::mem::take(&mut self.sync_jobs);
        for share in &self.store.shares {
            jobs.push(Job::Sync { hub: share.hub.clone(), out: sync::outgoing(&self.store, share, now_ms) });
        }
        self.sync_rx = Some(sync::run(jobs, ctx.clone()));
    }

    fn sync_done(&mut self, done: Done) {
        match done {
            Done::Synced(out, Ok(resp)) => {
                let Some(idx) = self.store.shares.iter().position(|s| s.list_id == out.list_id) else { return };
                if resp.list_id != out.list_id {
                    self.sync_error = Some(tr("the hub has a different list under this code").into());
                    return;
                }
                // Taken out while applying: `apply` changes the store it lives in.
                let mut share = self.store.shares.remove(idx);
                let before = share.rev;
                sync::apply(&mut self.store, &mut share, &out, &resp);
                share.last_sync = Some(Local::now().naive_local());
                let changed = share.rev != before || !out.sent.is_empty();
                self.store.shares.insert(idx, share);
                if self.selected.is_some_and(|id| self.store.task(id).is_none()) {
                    self.selected = None;
                }
                self.sync_error = None;
                if changed {
                    self.dirty = true;
                }
            }
            Done::Synced(_, Err(e)) => self.sync_error = Some(e),
            Done::Shared { list_id, hub, result } => {
                let dialog = self.share_dialog.as_mut().filter(|d| d.list == Some(list_id));
                match result {
                    Ok(code) => {
                        if self.store.lists.iter().any(|l| l.id == list_id) && !self.store.shares.iter().any(|s| s.list_id == list_id) {
                            self.store.shares.push(sync::Share {
                                list_id,
                                hub,
                                code,
                                rev: 0,
                                known: Default::default(),
                                last_sync: None,
                            });
                            self.dirty = true;
                            self.last_sync_at = None; // upload it right away
                        }
                        if let Some(d) = dialog {
                            d.busy = false;
                        }
                    }
                    Err(e) => {
                        if let Some(d) = dialog {
                            d.busy = false;
                            d.error = Some(e);
                        }
                    }
                }
            }
            Done::Joined { hub, code, result } => {
                let dialog = self.share_dialog.as_mut().filter(|d| d.list.is_none());
                match result {
                    Ok(resp) => match sync::join(&mut self.store, &hub, &code, &resp) {
                        Some(mut share) => {
                            share.last_sync = Some(Local::now().naive_local());
                            let id = share.list_id;
                            self.store.shares.push(share);
                            self.dirty = true;
                            self.view = View::List(id);
                            self.notes_page = false;
                            self.show_nav = false;
                            self.share_dialog = None;
                            self.notify(trf("Joined “{name}”", &[("name", &self.store.list_name(id))]));
                        }
                        None => {
                            if let Some(d) = dialog {
                                d.busy = false;
                                d.error = Some(tr("You already have this list.").into());
                            }
                        }
                    },
                    Err(e) => {
                        if let Some(d) = dialog {
                            d.busy = false;
                            d.error = Some(e);
                        }
                    }
                }
            }
        }
    }

    fn share_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.share_dialog.as_mut() else { return };
        let mut open_guide = false;
        let list = dialog.list;
        let existing = list.and_then(|id| self.store.shares.iter().find(|s| s.list_id == id)).cloned();
        let title = match (list, &existing) {
            (None, _) => tr("Join a shared list"),
            (Some(_), Some(_)) => tr("Shared list"),
            (Some(_), None) => tr("Share list"),
        };
        let list_name = list.map(|id| self.store.list_name(id).to_string()).unwrap_or_default();
        let mut close = false;
        let mut stop_sharing = false;
        let mut submit = false;
        dialog_window(ctx, title, self.insets)
            .show(ctx, |ui| {
                ui.set_max_width(dialog_width(ui, 380.0));
                if let Some(share) = &existing {
                    ui.label(trf("“{list_name}” is shared. Anyone with this code can join it from their RustDo:", &[("list_name", &list_name)]));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&share.code).monospace().size(26.0).strong()).selectable(true));
                        copy_button(ui, tr("Copy"), || share.code.clone());
                    });
                    ui.label(RichText::new(trf("Hub: {hub}", &[("hub", &share.hub)])).weak().small());
                    let status = match (&self.sync_error, share.last_sync) {
                        (Some(e), _) => RichText::new(trf("Can't sync right now: {e}", &[("e", &e)])).color(DANGER),
                        (None, Some(t)) => RichText::new(trf("Last synced {time}", &[("time", &format_time(t, self.store.settings.clock_24h))])).weak(),
                        (None, None) => RichText::new(tr("Syncing…")).weak(),
                    };
                    ui.label(status.small());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Close")).clicked() {
                            close = true;
                        }
                        if ui.button(tr("Stop syncing on this device")).clicked() {
                            stop_sharing = true;
                        }
                    });
                    return;
                }
                if list.is_some() {
                    ui.label(trf(
                        "Share “{list_name}” through your RustDo Hub. You'll get a code; anyone who has it and can reach the hub can see and change this list.",
                        &[("list_name", &list_name)],
                    ));
                } else {
                    ui.label(tr("Enter your RustDo Hub's address and the code someone gave you."));
                }
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Hub address")).small());
                let r = field_with_paste(ui, &mut dialog.hub, "e.g. 192.168.1.20:7979", "share_hub");
                submit |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && list.is_some();
                if list.is_none() {
                    ui.label(RichText::new(tr("Share code")).small());
                    let r = field_with_paste(ui, &mut dialog.code, tr("e.g. K7QM-2XPD"), "share_code");
                    submit |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                }
                if let Some(e) = &dialog.error {
                    ui.label(RichText::new(e).color(DANGER).small());
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let label = if list.is_some() { tr("Share") } else { tr("Join") };
                    if ui.add_enabled(!dialog.busy, egui::Button::new(label)).clicked() {
                        submit = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                    if dialog.busy {
                        ui.spinner();
                    }
                });
                if ui.add(egui::Button::new(RichText::new(tr("No hub yet? How syncing and sharing work")).small()).frame(false)).clicked() {
                    open_guide = true;
                }
            });

        if submit && !dialog.busy {
            dialog.error = None;
            match sync::normalize_hub(&dialog.hub) {
                None => dialog.error = Some(tr("Enter the hub's address, e.g. 192.168.1.20:7979").into()),
                Some(hub) => match list {
                    Some(list_id) => {
                        dialog.busy = true;
                        self.sync_jobs.push(Job::Share { hub: hub.clone(), list_id });
                        self.store.settings.hub = Some(hub);
                        self.dirty = true;
                    }
                    None => match sync::normalize_code(&dialog.code) {
                        None => dialog.error = Some(tr("A share code has 8 letters and numbers, like K7QM-2XPD.").into()),
                        Some(code) => {
                            dialog.busy = true;
                            self.sync_jobs.push(Job::Join { hub: hub.clone(), code });
                            self.store.settings.hub = Some(hub);
                            self.dirty = true;
                        }
                    },
                },
            }
        }
        if stop_sharing {
            if let Some(id) = list {
                self.store.shares.retain(|s| s.list_id != id);
                self.dirty = true;
                self.notify(tr("This list no longer syncs on this device. It stays here as a normal list."));
            }
            close = true;
        }
        if open_guide {
            self.hub_guide = true;
        }
        if close {
            self.share_dialog = None;
        }
    }

    fn notification_menu_items(&mut self, ui: &mut egui::Ui) {
        if cfg!(target_os = "android") {
            return;
        }
        ui.label(RichText::new(tr("Notifications")).strong());
        if ui
            .checkbox(&mut self.store.settings.notifications, tr("Notify me about reminders"))
            .changed()
        {
            self.dirty = true;
            self.helper_handled = false; // start or stop the Windows reminder helper
        }
        let note = if cfg!(windows) {
            tr("Windows shows them at the reminder time, even after you close RustDo (a small background helper starts with Windows).")
        } else if cfg!(target_arch = "wasm32") {
            tr("Shown while this app is open (browsers can't notify a closed web page).")
        } else {
            tr("Shown while RustDo is running (minimized is fine).")
        };
        ui.label(RichText::new(note).weak().small());
        if cfg!(target_arch = "wasm32") && self.store.settings.notifications {
            match notify::web_permission() {
                "default" => {
                    if ui.button(tr("Allow notifications")).clicked() {
                        notify::web_request_permission();
                    }
                }
                "denied" => {
                    ui.label(RichText::new(tr("Notifications are blocked in this browser's site settings.")).color(DANGER).small());
                }
                "unsupported" => {
                    ui.label(RichText::new(tr("This browser doesn't support notifications.")).weak().small());
                }
                _ => {}
            }
        }
    }

    fn backup_menu_items(&mut self, ui: &mut egui::Ui) {
        let now = Local::now().naive_local();
        ui.label(RichText::new(tr("Backups")).strong());
        let where_text = if cfg!(target_arch = "wasm32") {
            tr("A backup is saved automatically each day in this browser (last 7 days).").to_string()
        } else if cfg!(target_os = "android") {
            tr("A backup is saved automatically each day on this phone (last 14 days). Uninstalling the app removes them.").to_string()
        } else {
            trf(
                "A backup of your tasks and sticky notes is saved automatically each day in the Backups folder (last {days} days).",
                &[("days", &storage::KEEP_BACKUPS)],
            )
        };
        ui.add(egui::Label::new(RichText::new(where_text).weak().small()).wrap())
            .on_hover_text(self.storage.backups_location());
        if let Some(last) = self.store.settings.last_export {
            let when = relative_date(last.date(), now.date()).to_lowercase();
            ui.label(RichText::new(trf("Last exported: {when}", &[("when", &when)])).weak().small());
        }

        let export_label = if cfg!(target_os = "android") { tr("Copy backup to clipboard") } else { tr("Export backup…") };
        if ui.button(export_label).clicked() {
            self.close_settings(ui);
            let ctx = ui.ctx().clone();
            self.export_backup(&ctx, now);
        }
        if cfg!(target_os = "android") {
            if ui.button(tr("Restore from pasted backup…")).clicked() {
                self.close_settings(ui);
                self.paste_restore = Some(String::new());
            }
        } else if ui.button(tr("Restore from backup…")).clicked() {
            self.close_settings(ui);
            let ctx = ui.ctx().clone();
            self.pick_restore_file(&ctx);
        }

        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        {
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("Back up sticky notes…")).on_hover_text(tr("Save just your sticky notes to a file")).clicked() {
                    self.close_settings(ui);
                    let ctx = ui.ctx().clone();
                    self.backup_notes(&ctx, now.date());
                }
                if ui.button(tr("Restore sticky notes…")).on_hover_text(tr("Bring notes back from a backup (your current notes are kept)")).clicked() {
                    self.close_settings(ui);
                    let ctx = ui.ctx().clone();
                    self.pick_notes_restore_file(&ctx);
                }
            });
            if let Some(dir) = self.storage.folder() {
                if ui
                    .button(tr("Open RustDo folder"))
                    .on_hover_text(format!("{}\n{}", dir.display(), tr("Your tasks (tasks.json) and the Backups folder")))
                    .clicked()
                {
                    self.close_settings(ui);
                    platform::open_folder(&dir);
                }
            }
            match self.store.settings.backup_dir.clone() {
                Some(extra) => {
                    ui.add(egui::Label::new(RichText::new(trf("Also backing up to {extra}", &[("extra", &extra)])).small()).wrap());
                    ui.horizontal(|ui| {
                        if small_btn(ui, tr("Change…")).clicked() {
                            self.close_settings(ui);
                            self.choose_backup_folder(now.date());
                        }
                        if small_btn(ui, tr("Stop")).clicked() {
                            self.store.settings.backup_dir = None;
                            self.dirty = true;
                            self.close_settings(ui);
                        }
                    });
                }
                None => {
                    if ui
                        .button(tr("Also back up to a folder…"))
                        .on_hover_text(tr("For example a OneDrive or USB folder, so a copy lives off this PC."))
                        .clicked()
                    {
                        self.close_settings(ui);
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
                Err(e) => self.notify(trf("Couldn't read {path}: {e}", &[("path", &path.display()), ("e", &e)])),
            }
        }
    }

    fn import_dialog(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let Some(pending) = &mut self.pending_import else { return };
        let mut confirm = false;
        let mut cancel = false;
        let existing: Vec<String> = self.store.lists.iter().map(|l| l.name.to_lowercase()).collect();
        dialog_window(ctx, tr("Import tasks"), self.insets)
            .show(ctx, |ui| {
                ui.set_max_width(dialog_width(ui, 480.0));
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
                        ui.label(RichText::new(tr("List")).strong());
                        ui.label(RichText::new(tr("Open")).strong());
                        ui.label(RichText::new("").strong());
                        ui.end_row();
                        for l in &imp.lists {
                            let open = l.tasks.iter().filter(|t| !t.done).count();
                            ui.label(&l.name);
                            ui.label(open.to_string());
                            let note = if l.is_default {
                                tr("into your Tasks list")
                            } else if existing.contains(&l.name.trim().to_lowercase()) {
                                tr("merged with existing list")
                            } else {
                                tr("new list")
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
                ui.checkbox(&mut pending.include_completed, trf("Also import {n} completed tasks", &[("n", &imp.completed_count())]));
                ui.label(
                    RichText::new(tr("Tasks you've already imported are skipped. A backup of your current tasks is saved first."))
                        .weak()
                        .small(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(tr("Import")).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                        confirm = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
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
                    self.notify(trf("Import cancelled: couldn't save a backup first ({e}).", &[("e", &e)]));
                    return;
                }
            }
            let report = import::apply(&mut self.store, &pending.import, pending.include_completed, now);
            self.dirty = true;
            self.view = View::All;
            self.notes_page = false;
            self.search.clear();
            self.notify(report.summary());
        }
    }

    /// A swipe ticked a task off: done, with Undo in the toast.
    fn complete_with_undo(&mut self, id: Uuid) {
        let Some(title) = self.store.task(id).filter(|t| !t.done).map(|t| t.title.clone()) else { return };
        let next = self.store.set_done(id, true, Local::now().naive_local());
        self.dirty = true;
        let text = if next.is_some() {
            trf("Completed “{title}”. The next one is scheduled.", &[("title", &title)])
        } else {
            trf("Completed “{title}”", &[("title", &title)])
        };
        self.toast = Some(Toast { text, undo: Some(UndoOp::Completed { id, next }), at: Instant::now() });
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast { text: text.into(), undo: None, at: Instant::now() });
    }

    fn persist(&mut self) {
        self.dirty = false;
        if !self.can_save {
            return;
        }
        self.sync_soon = true;
        self.account_soon = true;
        match self.storage.save(&self.store) {
            Ok(()) => self.backup_pending = true,
            Err(e) => {
                log::error!(target: "rustdo", "saving tasks failed: {e}");
                self.notify(trf("Couldn't save: {e}", &[("e", &e)]));
            }
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
        let mut result = self.storage.auto_backup(&self.store, today);
        if notes::AVAILABLE && result.is_ok() {
            let snapshot = notes_ui::lock(&self.notes).notes.clone();
            result = self.storage.auto_backup_notes(&snapshot, &self.store.settings, today);
        }
        if let Err(e) = result {
            if !self.backup_error_shown {
                self.backup_error_shown = true;
                log::warn!(target: "rustdo", "automatic backup failed: {e}");
                self.notify(trf("Automatic backup failed: {e}", &[("e", &e)]));
            }
        }
    }

    fn export_backup(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        // Desktop: the backup carries the sticky notes too.
        let carried = notes::AVAILABLE.then(|| notes_ui::lock(&self.notes).notes.clone());
        let json = storage::export_json_with_notes(&self.store, carried.as_ref());
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
                .set_title(tr("Export RustDo backup"))
                .set_file_name(&name)
                .add_filter(tr("RustDo backup"), &["json"]);
            if let Some(dir) = start_dir {
                dialog = dialog.set_directory(dir);
            }
            let Some(path) = dialog.save_file() else {
                return;
            };
            match std::fs::write(&path, json) {
                Ok(()) => {
                    self.store.settings.last_export_dir = path.parent().map(|d| d.display().to_string());
                    self.notify(trf("Backup saved to {path}", &[("path", &path.display())]));
                }
                Err(e) => {
                    self.notify(trf("Couldn't save the backup: {e}", &[("e", &e)]));
                    return;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            if let Err(e) = platform::download_text(&name, &json) {
                self.notify(trf("Couldn't export: {e}", &[("e", &e)]));
                return;
            }
            self.notify(trf("Downloaded {name}. Keep it somewhere safe.", &[("name", &name)]));
        }
        #[cfg(target_os = "android")]
        {
            let _ = name;
            platform::copy_text(ctx, json);
            self.notify(tr("Backup copied. Paste it into a note or email to keep it safe."));
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
                self.notify(trf("Daily backups will also go to {path}", &[("path", &dir.display())]));
            }
        }
    }

    /// "What's new": the changelog for the releases since the user last looked.
    /// "How family sharing works": setting up a RustDo Hub and sharing a list, in the app.
    fn hub_guide_dialog(&mut self, ctx: &egui::Context) {
        if !self.hub_guide {
            return;
        }
        let mut close = false;
        let max_h = (ctx.screen_rect().height() - self.insets[1] - self.insets[3] - 170.0).clamp(160.0, 520.0);
        let releases = format!("https://github.com/{}/releases", update::REPO);
        dialog_window(ctx, tr("Syncing and sharing with RustDo Hub"), self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 500.0));
            egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                let para = |ui: &mut egui::Ui, text: &str| {
                    ui.add(egui::Label::new(text).wrap());
                };
                let step = |ui: &mut egui::Ui, n: &str, title: &str| {
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("{n}. {title}")).strong().color(ACCENT));
                };
                para(ui, tr("A RustDo Hub is a small program you run yourself, on a PC, a NAS or a home server. RustDo uses it to keep your own devices in sync, and to share lists with the people you live with. Nothing goes through anyone's cloud."));

                step(ui, "1", tr("Start a RustDo Hub (once, on one machine)"));
                para(ui, tr("Pick a PC, NAS or home server that's usually on. One hub is enough for the whole family."));
                ui.label(RichText::new(tr("On Windows")).strong());
                para(ui, tr("Download RustDo-Hub-<version>-windows.exe from the RustDo releases page and run it. It shows the hub's address, e.g. http://192.168.1.20:7979. When Windows Firewall asks, allow it on Private networks."));
                ui.hyperlink_to(tr("RustDo releases"), releases);
                ui.label(RichText::new(tr("With Docker (NAS, Raspberry Pi, Linux)")).strong());
                let cmd = "docker run -d --name rustdo-hub --init --restart unless-stopped -p 7979:7979 -v rustdo-hub-data:/data ghcr.io/metman2013/rustdo-hub:latest";
                ui.add(egui::Label::new(RichText::new(cmd).monospace().small()).wrap());
                if ui.small_button(tr("📋 Copy command")).clicked() {
                    platform::copy_text(ui.ctx(), cmd.to_string());
                }
                para(ui, tr("The hub's address is then the machine's address with port 7979, e.g. http://192.168.1.20:7979."));
                ui.label(RichText::new(tr("On a Synology NAS")).strong());
                para(ui, tr("In Container Manager, create a Project with the RustDo Hub compose file. The full guide has the steps, the firewall setting and how to update it."));
                ui.hyperlink_to(tr("Synology steps"), format!("{}#synology-nas", hub_docs()));
                ui.label(RichText::new(tr("On Linux without Docker")).strong());
                para(ui, tr("Download RustDo-Hub-<version>-linux-x86_64 (or -linux-arm64 for a Raspberry Pi), rustdo-hub.service and install-linux.sh from the releases page into one folder, then run: sudo sh install-linux.sh. The hub then starts with the computer."));

                step(ui, "2", tr("Sync your own devices"));
                para(ui, tr("On your first device: Settings, Sync my devices, Turn on sync. Enter the hub's address and choose \"This is my first device\". RustDo shows a sync key."));
                para(ui, tr("On each other device: Turn on sync, choose \"I already have a sync key\", and enter the same hub address and the key. Your lists, tasks, sticky notes and a few settings (theme, clock, sort order) then stay the same everywhere, within about 30 seconds."));
                para(ui, tr("Each device keeps its own reminder pop-ups and where its sticky notes sit on screen. The web app needs the hub on an https:// address (see below)."));

                step(ui, "3", tr("Share a list with family"));
                let how = if is_touch(ui) { tr("Long-press") } else { tr("Right-click") };
                para(ui, &trf("{how} the list in the menu, choose Share list, enter the hub's address (just the IP is fine, e.g. 192.168.1.20) and press Share. You get a code like K7QM-2XPD: send it to the people you're sharing with. They choose Join a shared list in Settings, under Family sharing, and enter the address and the code.", &[("how", &how)]));
                para(ui, tr("A shared list uses its own code, separate from your sync key: they see only that list, never your other lists. Your other devices join it by themselves when sync is on."));

                step(ui, "?", tr("Good to know"));
                for item in [
                    tr("If the same task is changed in two places at the same time, the later change wins. Shared lists show a 👥 icon."),
                    tr("Away from home or the hub is off? RustDo keeps working and catches up when it can reach the hub again."),
                    tr("To sync away from home, use a VPN such as Tailscale, or put the hub behind an HTTPS reverse proxy on your own subdomain (e.g. https://todo.example.com) and enter that address instead. Don't open port 7979 on your router directly."),
                    tr("Sync my devices works everywhere, including the web app. Sharing a list works in the Windows, Linux and Android apps."),
                    tr("Keep your sync key private, like a password: anyone with it who can reach your hub can see and change your tasks."),
                ] {
                    ui.horizontal_top(|ui| {
                        ui.label("•");
                        ui.add(egui::Label::new(item).wrap());
                    });
                }
                ui.add_space(4.0);
                ui.hyperlink_to(tr("Full guide, troubleshooting and reverse proxy setup"), hub_docs());
            });
            ui.add_space(6.0);
            if ui.button(tr("Got it")).clicked() {
                close = true;
            }
        });
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.hub_guide = false;
        }
    }

    /// Where to get RustDo on every kind of device.
    fn get_apps_dialog(&mut self, ctx: &egui::Context) {
        if !self.get_apps {
            return;
        }
        let v = update::CURRENT_VERSION;
        let repo = update::REPO;
        let release = format!("https://github.com/{repo}/releases/tag/v{v}");
        let apk = format!("https://github.com/{repo}/releases/download/v{v}/RustDo-{v}.apk");
        let hub = format!("https://github.com/{repo}/blob/main/docs/hub.md");
        let mut open = true;
        let mut close = false;
        dialog_window(ctx, tr("Get RustDo"), self.insets).open(&mut open).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 460.0));
            ui.label(tr("RustDo is free on all of these. Sync my devices (with your own Hub) keeps them in step."));
            ui.add_space(6.0);
            let row = |ui: &mut egui::Ui, name: &str, about: &str, links: &[(String, String)]| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(name).strong().size(16.0));
                    ui.add(egui::Label::new(RichText::new(about).weak()).wrap());
                    ui.horizontal_wrapped(|ui| {
                        for (label, url) in links {
                            if ui.add(egui::Button::new(RichText::new(label.as_str()).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            }
                        }
                    });
                });
                ui.add_space(4.0);
            };
            egui::ScrollArea::vertical().max_height(ctx.screen_rect().height() * 0.7).show(ui, |ui| {
                row(
                    ui,
                    "Windows",
                    tr("From the Microsoft Store (updates itself), or as an installer or a portable .exe."),
                    &[
                        (tr("Microsoft Store").to_string(), "https://apps.microsoft.com/detail/9PK0DF0QBWWM".to_string()),
                        (tr("Installer & portable").to_string(), release.clone()),
                    ],
                );
                row(
                    ui,
                    "Android",
                    tr("For phones and tablets: download the APK and open it. Android asks once to allow installing apps from your browser."),
                    &[(tr("Download the APK").to_string(), apk)],
                );
                row(ui, "Linux", tr("A .deb (Debian, Ubuntu, Mint), an .rpm (Fedora, openSUSE), or for any distribution an AppImage or a .tar.gz with an install script."), &[(tr("Downloads").to_string(), release.clone())]);
                row(
                    ui,
                    tr("Web app"),
                    tr("Runs in any browser at rustdo.app, also on iPhone, iPad and Chromebook. Install it from the browser's menu: “Install app” or “Add to Home screen”."),
                    &[(tr("Open rustdo.app").to_string(), "https://rustdo.app".to_string())],
                );
                row(
                    ui,
                    "RustDo Hub",
                    tr("Your own small server for sharing lists and syncing your devices: Docker, Synology, Linux, Windows or a Home Assistant app."),
                    &[(tr("How to set it up").to_string(), hub)],
                );
            });
            if ui.button(tr("Close")).clicked() {
                close = true;
            }
        });
        if !open || close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.get_apps = false;
        }
    }

    fn whats_new_dialog(&mut self, ctx: &egui::Context) {
        let Some(releases) = &self.whats_new else { return };
        let mut close = false;
        let max_h = (ctx.screen_rect().height() - self.insets[1] - self.insets[3] - 170.0).clamp(160.0, 460.0);
        let title = match releases.as_slice() {
            [only] => trf("What's new in RustDo {version}", &[("version", &only.version)]),
            _ => tr("What's new in RustDo").to_string(),
        };
        dialog_window(ctx, title, self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 460.0));
            egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                for (i, r) in releases.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(6.0);
                        ui.separator();
                    }
                    if releases.len() > 1 {
                        ui.label(RichText::new(trf("Version {version}", &[("version", &r.version)])).strong().size(17.0));
                    }
                    if !r.date.is_empty() {
                        ui.label(RichText::new(&r.date).weak().small());
                    }
                    for (section, items) in &r.sections {
                        ui.add_space(4.0);
                        ui.label(RichText::new(section).strong().color(ACCENT));
                        for item in items {
                            ui.horizontal_top(|ui| {
                                ui.label("•");
                                ui.add(egui::Label::new(item.as_str()).wrap());
                            });
                        }
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Got it")).clicked() {
                    close = true;
                }
                ui.hyperlink_to(tr("All releases"), format!("https://github.com/{}/releases", update::REPO));
            });
        });
        if close {
            self.whats_new = None;
            let current = Some(update::CURRENT_VERSION.to_string());
            if self.store.settings.last_seen_version != current {
                self.store.settings.last_seen_version = current;
                self.dirty = true;
            }
        }
    }

    /// "Restore sticky notes?": merges a notes backup into the current notes.
    fn notes_restore_dialog(&mut self, ctx: &egui::Context) {
        let Some((name, found)) = &self.pending_notes_restore else { return };
        let mut confirm = false;
        let mut cancel = false;
        let count = found.notes.len();
        dialog_window(ctx, tr("Restore sticky notes?"), self.insets).show(ctx, |ui| {
            ui.set_max_width(dialog_width(ui, 420.0));
            ui.label(format!("{name}: {}.", plural(count, "{n} sticky note", "{n} sticky notes")));
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    "Your current notes are kept. Notes from the backup are added (hidden: show them \
                     from the Sticky notes page), and a note in both keeps whichever was edited last.",
                )
                .wrap(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(tr("Restore notes")).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                    confirm = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.pending_notes_restore = None;
        } else if confirm {
            let (_, found) = self.pending_notes_restore.take().unwrap();
            let (added, updated) = notes_ui::lock(&self.notes).merge_in(&found);
            self.notes_page = true;
            self.notify(match (added, updated) {
                (0, 0) => tr("Nothing to restore: you already have these notes.").to_string(),
                (a, 0) => format!("{} {}", plural(a, "Restored {n} sticky note.", "Restored {n} sticky notes."), tr("Show them from this page.")),
                (a, u) => trf("Restored {a} sticky notes and updated {u}.", &[("a", &a), ("u", &u)]),
            });
        }
    }

    /// Desktop: saves every sticky note to a file of the user's choosing.
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    fn backup_notes(&mut self, _ctx: &egui::Context, today: NaiveDate) {
        let snapshot = notes_ui::lock(&self.notes).notes.clone();
        if snapshot.notes.is_empty() {
            self.notify(tr("There are no sticky notes to back up yet."));
            return;
        }
        let start_dir = self
            .store
            .settings
            .last_export_dir
            .clone()
            .map(std::path::PathBuf::from)
            .filter(|d| d.is_dir())
            .or_else(|| self.storage.folder());
        let mut dialog = rfd::FileDialog::new()
            .set_title(tr("Back up sticky notes"))
            .set_file_name(notes::backup_file_name(today))
            .add_filter(tr("Sticky notes backup"), &["json"]);
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else { return };
        match notes::file::save(&path, &snapshot) {
            Ok(()) => {
                self.store.settings.last_export_dir = path.parent().map(|d| d.display().to_string());
                self.dirty = true;
                let n = snapshot.notes.len();
                self.notify(format!("{} {}", plural(n, "{n} sticky note saved to", "{n} sticky notes saved to"), path.display()));
            }
            Err(e) => self.notify(trf("Couldn't save the notes backup: {e}", &[("e", &e)])),
        }
    }

    /// Desktop: choose a sticky notes backup (or a full RustDo backup) to restore notes from.
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    fn pick_notes_restore_file(&mut self, _ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new()
            .set_title(tr("Restore sticky notes"))
            .add_filter(tr("Sticky notes or RustDo backup"), &["json"]);
        if let Some(dir) = self.storage.folder() {
            let backups = dir.join("Backups");
            dialog = dialog.set_directory(if backups.is_dir() { backups } else { dir });
        }
        let Some(path) = dialog.pick_file() else { return };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| notes::parse_backup(&b)) {
            Ok(found) => self.pending_notes_restore = Some((name, found)),
            Err(e) => self.notify(format!("{name}: {e}")),
        }
    }

    /// Web: downloads every sticky note as a file.
    #[cfg(target_arch = "wasm32")]
    fn backup_notes(&mut self, _ctx: &egui::Context, today: NaiveDate) {
        let snapshot = notes_ui::lock(&self.notes).notes.clone();
        if snapshot.notes.is_empty() {
            self.notify(tr("There are no sticky notes to back up yet."));
            return;
        }
        let name = notes::backup_file_name(today);
        let json = serde_json::to_string_pretty(&snapshot).unwrap_or_default();
        match platform::download_text(&name, &json) {
            Ok(()) => self.notify(trf("Downloaded {name}. Keep it somewhere safe.", &[("name", &name)])),
            Err(e) => self.notify(trf("Couldn't download the notes backup: {e}", &[("e", &e)])),
        }
    }

    /// Web: the browser's file picker; a notes backup it returns goes to the restore dialog.
    #[cfg(target_arch = "wasm32")]
    fn pick_notes_restore_file(&mut self, ctx: &egui::Context) {
        self.pick_import_file(ctx);
    }

    /// Android: copies every sticky note to the clipboard (paste it somewhere safe; paste it
    /// back with "Restore from pasted backup").
    #[cfg(target_os = "android")]
    fn backup_notes(&mut self, ctx: &egui::Context, _today: NaiveDate) {
        let snapshot = notes_ui::lock(&self.notes).notes.clone();
        if snapshot.notes.is_empty() {
            self.notify(tr("There are no sticky notes to back up yet."));
            return;
        }
        platform::copy_text(ctx, serde_json::to_string_pretty(&snapshot).unwrap_or_default());
        let n = snapshot.notes.len();
        self.notify(format!(
            "{} {}",
            plural(n, "{n} sticky note copied.", "{n} sticky notes copied."),
            tr("Paste them into a note or email to keep them safe.")
        ));
    }

    /// Android: paste a notes backup (or a full backup) into the restore box.
    #[cfg(target_os = "android")]
    fn pick_notes_restore_file(&mut self, _ctx: &egui::Context) {
        self.paste_restore = Some(String::new());
    }

    fn restore_dialog(&mut self, ctx: &egui::Context) {
        // Android: paste a backup that was exported to the clipboard.
        if let Some(text) = &mut self.paste_restore {
            let mut restore = false;
            let mut cancel = false;
            dialog_window(ctx, tr("Restore from a backup"), self.insets)
                .show(ctx, |ui| {
                    ui.set_max_width(dialog_width(ui, 420.0));
                    ui.label(tr("Paste the backup text you saved earlier:"));
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(text).desired_rows(6).desired_width(f32::INFINITY));
                    });
                    ui.horizontal(|ui| {
                        if ui.button(tr("Continue")).clicked() {
                            restore = true;
                        }
                        if ui.button(tr("Cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
            if restore {
                let text = self.paste_restore.take().unwrap_or_default();
                match storage::parse_backup(text.as_bytes()) {
                    Ok(store) => {
                        let carried = notes::parse_backup(text.as_bytes()).ok().filter(|_| notes::AVAILABLE);
                        self.pending_restore = Some((tr("pasted backup").into(), store, carried));
                    }
                    // A sticky notes backup: merged in, after confirmation.
                    Err(_) if notes::AVAILABLE && notes::parse_backup(text.as_bytes()).is_ok() => {
                        if let Ok(found) = notes::parse_backup(text.as_bytes()) {
                            self.pending_notes_restore = Some((tr("pasted notes").into(), found));
                        }
                    }
                    Err(e) => self.notify(e),
                }
            } else if cancel {
                self.paste_restore = None;
            }
        }

        self.notes_restore_dialog(ctx);
        let Some((name, backup, carried)) = &self.pending_restore else { return };
        let note_count = carried.as_ref().map_or(0, |n| n.notes.len());
        let mut confirm = false;
        let mut cancel = false;
        let open = backup.tasks.iter().filter(|t| !t.done).count();
        let newest = backup.tasks.iter().map(|t| t.created).max();
        dialog_window(ctx, tr("Restore backup?"), self.insets)
            .show(ctx, |ui| {
                ui.set_max_width(dialog_width(ui, 420.0));
                let notes_part = match note_count {
                    0 => String::new(),
                    1 => tr(", 1 sticky note").to_string(),
                    n => trf(", {n} sticky notes", &[("n", &n)]),
                };
                ui.label(trf(
                    "{name}: {lists} lists, {tasks} tasks ({open} open){notes}.",
                    &[("name", &name), ("lists", &backup.lists.len()), ("tasks", &backup.tasks.len()), ("open", &open), ("notes", &notes_part)],
                ));
                if let Some(newest) = newest {
                    ui.label(RichText::new(trf("Newest task created {date}.", &[("date", &i18n::date_medium(newest.date()))])).weak());
                }
                ui.add_space(4.0);
                let what = if note_count > 0 {
                    "This replaces all your current tasks, lists and sticky notes with the backup. \
                     Your current tasks and notes are saved first, so this can be undone."
                } else {
                    "This replaces all your current tasks and lists with the backup. \
                     Your current tasks are saved first, so this can be undone."
                };
                ui.add(egui::Label::new(what).wrap());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new(tr("Replace my tasks")).color(Color32::WHITE)).fill(DANGER))
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if cancel {
            self.pending_restore = None;
        } else if confirm {
            let (name, mut backup, carried) = self.pending_restore.take().unwrap();
            if self.can_save {
                if let Err(e) = self.storage.snapshot(&self.store, "before-restore") {
                    self.notify(trf("Restore cancelled: couldn't save your current tasks first ({e}).", &[("e", &e)]));
                    return;
                }
            }
            // Sticky notes first: if their safety copy fails, nothing has changed yet.
            if let Some(restored) = carried {
                let safety = self.storage.backup_dirs(&self.store.settings).into_iter().next();
                let replaced = notes_ui::lock(&self.notes).replace_all(restored, safety.as_deref());
                if let Err(e) = replaced {
                    self.notify(trf("Restore cancelled: {e}.", &[("e", &e)]));
                    return;
                }
            }
            // Keep this device's preferences (theme, update checks, backup folder).
            backup.settings = self.store.settings.clone();
            self.store = backup;
            self.selected = None;
            self.alerts.clear();
            self.view = View::MyDay;
            self.notes_page = false;
            self.search.clear();
            self.dirty = true;
            self.notify(trf("Restored from {name}.", &[("name", &name)]));
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
                ui.label(tr("Your tasks are stored in this browser only. Keep a backup in case browser data is cleared."));
                if ui.button(tr("Export backup")).clicked() {
                    export = true;
                }
                if ui.button(tr("Not now")).clicked() {
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
                text: trf("Deleted “{title}”", &[("title", &task.title)]),
                undo: Some(UndoOp::Deleted(idx, task)),
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
                        self.notify(tr("Nice! The next occurrence has been scheduled."));
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
                // Settings give the right-hand pane back to the task.
                let reopen = self.show_settings && self.selected == Some(id);
                self.selected = if self.selected == Some(id) && !reopen { None } else { Some(id) };
                self.show_settings = false;
                self.new_step.clear();
            }
            Action::Delete(id) => self.delete_task(id),
            Action::ToNote(id) => self.task_to_note(id),
            Action::SwipeDone(id) => self.complete_with_undo(id),
            Action::MoveTask(shown, id, to) => {
                self.dirty |= self.store.move_task(&shown, id, to);
            }
            Action::ToggleExpand(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                } else if self.last_added.is_some_and(|(last, _)| last == id) {
                    self.last_added = None;
                }
            }
            Action::ToggleStep(task, step) => {
                if let Some(s) = self.store.task_mut(task).and_then(|t| t.steps.iter_mut().find(|s| s.id == step)) {
                    s.done = !s.done;
                    self.dirty = true;
                }
            }
            Action::AddStep(task, title) => {
                if let Some(t) = self.store.task_mut(task) {
                    t.steps.push(Step::new(title.trim()));
                    self.expanded.insert(task);
                    self.dirty = true;
                }
            }
        }
    }

    /// Back on a phone: closes what's on top, one step at a time; at the menu, leaves the app.
    fn phone_back(&mut self, ctx: &egui::Context) {
        // A dialog first.
        #[cfg(target_os = "android")]
        if self.photo_picker.take().is_some() {
            return;
        }
        if self.whats_new.is_some() {
            self.whats_new = None;
            let current = Some(update::CURRENT_VERSION.to_string());
            if self.store.settings.last_seen_version != current {
                self.store.settings.last_seen_version = current;
                self.dirty = true;
            }
            return;
        }
        if let Some(d) = self.ms_import.take() {
            if let Some(s) = &d.session {
                s.cancel();
            }
            return;
        }
        if self.account_dialog.as_ref().is_some_and(|d| !d.busy) {
            self.account_dialog = None;
            return;
        }
        if self.hub_guide {
            self.hub_guide = false;
            return;
        }
        if self.share_dialog.as_ref().is_some_and(|d| !d.busy) {
            self.share_dialog = None;
            return;
        }
        if self.pending_import.take().is_some()
            || self.pending_restore.take().is_some()
            || self.pending_notes_restore.take().is_some()
            || self.paste_restore.take().is_some()
            || self.confirm_delete_list.take().is_some()
            || self.renaming_list.take().is_some()
        {
            return;
        }
        // Then the on-screen keyboard.
        if ctx.wants_keyboard_input() {
            ctx.memory_mut(|m| m.stop_text_input());
            return;
        }
        // Then one screen back.
        if self.show_list.is_some() {
            self.show_list = None;
        } else if self.show_settings {
            self.show_settings = false;
        } else if !self.search.is_empty() {
            self.search.clear();
        } else if self.selected.is_some() {
            self.selected = None;
        } else if self.show_nav {
            // The menu slides over the list: back closes it.
            self.show_nav = false;
        } else if self.notes_page {
            self.notes_page = false;
            self.view = View::MyDay;
        } else if self.view != View::MyDay {
            self.view = View::MyDay;
        } else {
            platform::leave_app();
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
                self.notes_page = false;
            }
            self.focus_add = true;
        }
        if find {
            self.focus_search = true;
        }
        if esc {
            if self.show_list.is_some() {
                self.show_list = None;
            } else if !self.search.is_empty() {
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
        let side = self.side_frame(ctx);
        // Wide but short screens (a tablet in landscape): everything doesn't fit with the
        // footer pinned to the bottom, so the whole sidebar scrolls as one, like on a phone.
        let flow = narrow || {
            let touch = ctx.data(|d| d.get_temp::<bool>(Id::new(TOUCH_ID))).unwrap_or(false);
            let row = if touch { 48.0 } else { 30.0 } + 4.0;
            let rows = 5.0 + self.store.lists.len().min(4) as f32 + if notes::AVAILABLE { 2.0 } else { 0.0 };
            let needed = 120.0 + rows * row + if touch { 140.0 } else { 86.0 } + SIDEBAR_FOOTER_H;
            // Not counting the on-screen keyboard: the layout mustn't change when it opens
            // (that would make the selected "+ New list" field a new widget and close it).
            let screen_h = ctx.screen_rect().height();
            let bottom = if self.insets[3] > screen_h * 0.2 { 0.0 } else { self.insets[3] };
            screen_h - self.insets[1] - bottom < needed
        };
        let mut body = |ui: &mut egui::Ui| {
                ui.add_space(10.0);
                let touch = is_touch(ui);
                ui.horizontal(|ui| {
                    if narrow && back_button(ui, "").on_hover_text(tr("Back")).clicked() {
                        self.show_nav = false;
                    }
                    let _ = touch;
                    ui.label(RichText::new(tr("✔ RustDo")).size(20.0).strong().color(ACCENT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let theme = self.store.settings.theme;
                        let (icon, next, tip) = match theme {
                            ThemeChoice::System => ("🖥", ThemeChoice::Light, tr("Theme: system")),
                            ThemeChoice::Light => ("☀", ThemeChoice::Dark, tr("Theme: light")),
                            ThemeChoice::Dark => ("🌙", ThemeChoice::System, tr("Theme: dark")),
                        };
                        self.settings_button(ui, narrow);
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
                        .hint_text(if touch { tr("🔍 Search") } else { tr("🔍 Search  (Ctrl+F)") })
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
                    (View::MyDay, "☀", tr("My Day")),
                    (View::Important, "★", tr("Important")),
                    (View::Planned, "📅", tr("Planned")),
                    (View::All, ALL_ICON, tr("All")),
                    (View::Completed, "✔", tr("Completed")),
                ];
                for (view, icon, name) in smart {
                    let count = if view == View::Completed {
                        0
                    } else {
                        self.store.open_count(view, today)
                    };
                    let selected = self.view == view && !searching;
                    if nav_row(ui, selected && !self.notes_page, icon, name, count).clicked() {
                        self.view = view;
                        self.notes_page = false;
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
                } + if notes::AVAILABLE { NOTES_SECTION_H } else { 0.0 }
                    + SIDEBAR_FOOTER_H;
                // Phone layout (and short screens): the whole sidebar scrolls as one page, so
                // show every list.
                let max_h = if flow { f32::INFINITY } else { (ui.available_height() - reserved).max(60.0) };
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let list_count = lists.len();
                        let mut rows: Vec<(Uuid, egui::Rect)> = Vec::new();
                        for (index, (id, name)) in lists.into_iter().enumerate() {
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
                            let shared = self.store.shares.iter().any(|s| s.list_id == id);
                            let icon = self.list_icon(id);
                            let selected = self.view == View::List(id) && !searching && !self.notes_page;
                            let r = nav_row(ui, selected, icon, &name, count);
                            rows.push((id, r.rect));
                            // Desktop: drag a list to move it. Touch: dragging the row scrolls,
                            // so a ≡ handle (left of the count) moves it instead.
                            if !is_touch(ui) && r.drag_started() {
                                self.list_drag = Some(id);
                            }
                            if is_touch(ui) && list_count > 1 {
                                let h = Rect::from_center_size(Pos2::new(r.rect.right() - 62.0, r.rect.center().y), Vec2::new(40.0, r.rect.height()));
                                let handle = ui.interact(h, Id::new(("list_handle", id)), Sense::drag());
                                let color = if handle.dragged() { ACCENT } else { ui.visuals().weak_text_color() };
                                for dy in [-5.0, 0.0, 5.0] {
                                    ui.painter().hline((h.center().x - 8.0)..=(h.center().x + 8.0), h.center().y + dy, Stroke::new(2.0_f32, color));
                                }
                                if handle.drag_started() {
                                    self.list_drag = Some(id);
                                }
                            }
                            if r.clicked() {
                                self.view = View::List(id);
                                self.notes_page = false;
                                self.search.clear();
                                self.show_nav = false;
                            }
                            if r.double_clicked() {
                                self.renaming_list = Some((id, name.clone()));
                                self.rename_focus = true;
                            }
                            r.context_menu(|ui| {
                                if ui.button(tr("✏  Rename list")).clicked() {
                                    self.renaming_list = Some((id, name.clone()));
                                    self.rename_focus = true;
                                    ui.close_menu();
                                }
                                if sync::SUPPORTED {
                                    let label = if shared { tr("👥  Sharing…") } else { tr("👥  Share list…") };
                                    if ui.button(label).clicked() {
                                        self.open_share_dialog(Some(id));
                                        ui.close_menu();
                                    }
                                }
                                if ui.button(tr("🖼  Background…")).clicked() {
                                    self.bg_dialog = Some(id);
                                    ui.close_menu();
                                }
                                if ui.button(tr("📄  Show list")).clicked() {
                                    self.show_list = Some(View::List(id));
                                    self.show_nav = false;
                                    ui.close_menu();
                                }
                                if notes::AVAILABLE {
                                    let label = if self.list_card_showing(id) { tr("📌  Unpin list") } else { tr("📌  Pin list") };
                                    if ui.button(label).clicked() {
                                        self.toggle_pin(id);
                                        ui.close_menu();
                                    }
                                }
                                if index > 0 && ui.button(tr("⬆  Move up")).clicked() {
                                    self.dirty |= self.store.move_list(id, index - 1);
                                    ui.close_menu();
                                }
                                if index + 1 < list_count && ui.button(tr("⬇  Move down")).clicked() {
                                    self.dirty |= self.store.move_list(id, index + 1);
                                    ui.close_menu();
                                }
                                if !is_default && ui.button(tr("🗑  Delete list")).clicked() {
                                    self.confirm_delete_list = Some(id);
                                    ui.close_menu();
                                }
                            });
                        }
                        // A list being dragged: show where it will go, move it on release.
                        if let Some(dragged) = self.list_drag {
                            let (pointer, released) = ui.input(|i| (i.pointer.interact_pos(), !i.pointer.primary_down()));
                            if let Some(pos) = pointer {
                                let others: Vec<&(Uuid, egui::Rect)> = rows.iter().filter(|(id, _)| *id != dragged).collect();
                                let to = others.iter().filter(|(_, r)| r.center().y < pos.y).count();
                                let y = match others.get(to) {
                                    Some((_, r)) => r.top(),
                                    None => others.last().map_or(pos.y, |(_, r)| r.bottom()),
                                };
                                let x = ui.min_rect().x_range();
                                ui.painter().hline(x, y, Stroke::new(2.0_f32, ACCENT));
                                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                                if released {
                                    self.dirty |= self.store.move_list(dragged, to);
                                    self.list_drag = None;
                                }
                            } else if released {
                                self.list_drag = None;
                            }
                        }
                    });

                ui.separator();
                let min = field_height(ui);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.new_list)
                        .hint_text(tr("+ New list"))
                        .min_size(min)
                        .vertical_align(Align::Center)
                        .desired_width(f32::INFINITY),
                );
                keep_above_keyboard(&r);
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    if let Some(id) = self.store.add_list(&self.new_list) {
                        self.new_list.clear();
                        self.view = View::List(id);
                        self.notes_page = false;
                        self.search.clear();
                        self.show_nav = false;
                        self.focus_add = !platform::IS_MOBILE_OR_WEB;
                        self.dirty = true;
                    }
                }
                ui.add_space(2.0);
                if CAN_IMPORT
                    && ui
                        .add_sized([ui.available_width(), 26.0], egui::Button::new(tr("⬇  Import from Microsoft To Do…")))
                        .on_hover_text(tr("Sign in with Microsoft to bring over your lists, or import an export file."))
                        .clicked()
                {
                    self.ms_import = Some(MsImportDialog::default());
                    self.show_nav = false;
                }

                // Sticky notes: their own section, apart from the to-do lists.
                if notes::AVAILABLE {
                    ui.add_space(4.0);
                    ui.separator();
                    let (heading, tip) = if cfg!(target_os = "android") {
                        (tr("STICKY NOTES"), tr("Sticky notes, kept on this phone"))
                    } else if notes::WALL {
                        (tr("STICKY NOTES"), tr("A wall of sticky notes, kept in this browser"))
                    } else {
                        (tr("DESKTOP NOTES"), tr("Notes on your desktop, kept on this computer"))
                    };
                    ui.label(RichText::new(heading).small().weak());
                    // Desktop: notes showing; web: all of them (they're all on the wall).
                    let shown = notes_ui::lock(&self.notes).notes.notes.iter().filter(|n| !n.archived && (n.open || notes::WALL)).count();
                    if nav_row(ui, self.notes_page, "📝", tr("Sticky notes"), shown)
                        .on_hover_text(tip)
                        .clicked()
                    {
                        self.notes_page = true;
                        self.search.clear();
                        self.show_nav = false;
                    }
                }

                // Footer: version, What's new and the Discord (also in Settings). Wide
                // screens pin it to the bottom of the sidebar; on a phone it ends the page.
                let footer = |ui: &mut egui::Ui, whats_new: &mut Option<Vec<changelog::Release>>| {
                    discord_button(ui);
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("RustDo {}", update::CURRENT_VERSION)).small().weak());
                        if ui.add(egui::Label::new(RichText::new(tr("What's new")).small().color(ACCENT)).sense(Sense::click()))
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .clicked()
                        {
                            *whats_new = Some(changelog::releases());
                        }
                    });
                };
                if flow {
                    ui.add_space(8.0);
                    ui.separator();
                    footer(ui, &mut self.whats_new);
                    ui.add_space(8.0);
                } else {
                    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                        ui.add_space(4.0);
                        footer(ui, &mut self.whats_new);
                        ui.separator();
                    });
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
                .frame(side)
                .resizable(true)
                .default_width(250.0)
                .width_range(200.0..=400.0)
                .show(ctx, |ui| {
                    if flow {
                        egui::ScrollArea::vertical().id_salt("sidebar_page").auto_shrink([false; 2]).show(ui, |ui| body(ui));
                    } else {
                        body(ui);
                    }
                });
        }
    }

    /// A sticky note becomes a task in the default list: its first line the title, a
    /// checklist's items the subtasks (ticks kept), other text the task's note. The note
    /// goes away.
    fn note_to_task(&mut self, id: Uuid) {
        let Some(note) = notes_ui::lock(&self.notes).notes.get(id).cloned() else { return };
        let now = Local::now().naive_local();
        let title = note.title().to_string();
        let list = self.store.default_list;
        let Some(task_id) = self.store.add_task(&title, View::List(list), now) else { return };
        if let Some(t) = self.store.task_mut(task_id) {
            if note.is_checklist() {
                t.steps = note
                    .items()
                    .into_iter()
                    .filter(|i| !i.text.trim().is_empty())
                    .map(|i| {
                        let mut step = Step::new(i.text.trim());
                        step.done = i.done;
                        step
                    })
                    .collect();
            } else {
                // Everything after the first line (which is the title).
                let mut lines = note.text.lines().skip_while(|l| l.trim().is_empty());
                lines.next();
                t.notes = lines.collect::<Vec<_>>().join("\n").trim().to_string();
            }
        }
        notes_ui::lock(&self.notes).delete(id);
        self.dirty = true;
        self.notify(trf("“{title}” is now a task in “{list}”.", &[("title", &title), ("list", &self.store.list_name(list))]));
    }

    /// A task becomes a sticky note: its title the first line, subtasks a checklist below
    /// it (ticks kept), the task's note after. The task goes away.
    fn task_to_note(&mut self, id: Uuid) {
        let Some(task) = self.store.task(id).cloned() else { return };
        let mut lines = vec![task.title.clone()];
        for st in &task.steps {
            lines.push(format!("{}{}", if st.done { notes::CHECKED } else { notes::UNCHECKED }, st.title));
        }
        if !task.notes.trim().is_empty() {
            if !task.steps.is_empty() {
                lines.push(String::new());
            }
            lines.push(task.notes.trim().to_string());
        }
        {
            let mut s = notes_ui::lock(&self.notes);
            let note = s.new_note(None);
            if let Some(n) = s.notes.get_mut(note) {
                n.text = lines.join("\n");
            }
            s.changed();
        }
        self.delete_task(id);
        self.notify(trf("“{title}” is now a sticky note.", &[("title", &task.title)]));
    }

    /// The Sticky notes page (desktop), with a Back button on narrow windows.
    fn notes_panel(&mut self, ctx: &egui::Context) {
        let narrow = ctx.screen_rect().width() < NARROW_WIDTH;
        egui::CentralPanel::default().show(ctx, |ui| {
            // Same top as the task pages (☰ on phones, then the page's title and buttons).
            ui.add_space(10.0);
            match notes_ui::notes_page(ui, &self.notes, narrow) {
                notes_ui::PageAction::Backup => self.backup_notes(ctx, Local::now().date_naive()),
                notes_ui::PageAction::Restore => self.pick_notes_restore_file(ctx),
                notes_ui::PageAction::ShowLists => self.show_nav = true,
                notes_ui::PageAction::NoteToTask(id) => self.note_to_task(id),
                notes_ui::PageAction::ImportKeep => self.pick_keep_file(ctx),
                notes_ui::PageAction::ImportMsNotes => self.import_ms_sticky_notes(),
                notes_ui::PageAction::Archived(id) => {
                    let title = notes_ui::lock(&self.notes).notes.get(id).map(|n| n.title().to_string()).unwrap_or_default();
                    self.toast = Some(Toast {
                        text: trf("“{title}” moved to the Archive", &[("title", &title)]),
                        undo: Some(UndoOp::Archived(id)),
                        at: Instant::now(),
                    });
                }
                notes_ui::PageAction::None => {}
            }
        });
    }

    // ------------------------------------------------------------------ task list

    fn main_panel(&mut self, ctx: &egui::Context, now: NaiveDateTime, narrow: bool) {
        let today = now.date();
        let mut actions = Vec::new();
        egui::CentralPanel::default().show(ctx, |ui| {
            let query = self.search.trim().to_lowercase();
            let searching = !query.is_empty();
            let (icon, title) = if searching {
                ("🔍", trf("Searching for “{query}”", &[("query", &self.search.trim())]))
            } else {
                match self.view {
                    View::MyDay => ("☀", tr("My Day").to_string()),
                    View::Important => ("★", tr("Important").to_string()),
                    View::Planned => ("📅", tr("Planned").to_string()),
                    View::All => (ALL_ICON, tr("All").to_string()),
                    View::Completed => ("✔", tr("Completed").to_string()),
                    View::List(id) => (self.list_icon(id), self.store.list_name(id).to_string()),
                }
            };

            ui.add_space(10.0);
            self.update_banner(ui);
            self.export_nag(ui, now);
            let touch = is_touch(ui);
            ui.horizontal(|ui| {
                let menu = egui::Button::new(RichText::new("☰").size(if touch { 26.0 } else { 20.0 })).frame(false);
                let menu = if touch { menu.min_size(Vec2::splat(44.0)) } else { menu };
                if narrow && ui.add(menu).on_hover_text(tr("Lists")).clicked() {
                    self.show_nav = true;
                }
                let size = if narrow { 20.0 } else { 24.0 };
                // Long list names are shortened (…) so the buttons beside them always fit
                // without running into each other.
                let buttons = match (narrow, touch) {
                    (true, true) => 170.0,
                    (true, false) => 140.0,
                    (false, _) => 280.0,
                };
                let title_w = (ui.available_width() - buttons).max(60.0);
                let label = egui::Label::new(RichText::new(format!("{icon}  {title}")).size(size).strong()).truncate();
                let galley_w = {
                    let text = egui::WidgetText::from(RichText::new(format!("{icon}  {title}")).size(size).strong());
                    text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x
                };
                if galley_w <= title_w {
                    ui.label(RichText::new(format!("{icon}  {title}")).size(size).strong());
                } else {
                    ui.add_sized(Vec2::new(title_w, size * 1.4), label);
                }
                if !searching && self.view != View::Completed {
                    // Phones: just the icon (the header is narrow); wider screens: say what it does.
                    let show = if narrow {
                        egui::Button::new(RichText::new("📄").size(20.0)).min_size(Vec2::splat(if touch { 44.0 } else { 32.0 }))
                    } else {
                        egui::Button::new(tr("📄  Show list"))
                    };
                    if ui.add(show).on_hover_text(tr("Show list: to read, print or share")).clicked() {
                        self.show_list = Some(self.view);
                    }
                }
                // 📌 Pin: the list as a card on the desktop (or the sticky notes wall).
                if let (View::List(list), false, true) = (self.view, searching, notes::AVAILABLE) {
                    let pinned = self.list_card_showing(list);
                    let pin = egui::Button::new(RichText::new("📌").size(if narrow { 20.0 } else { 16.0 }))
                        .selected(pinned)
                        .min_size(Vec2::splat(if touch { 44.0 } else if narrow { 32.0 } else { 0.0 }));
                    let tip = match (pinned, notes::WALL) {
                        (true, _) => tr("Unpin this list"),
                        (false, true) => tr("Pin this list to your sticky notes"),
                        (false, false) => tr("Pin this list to your desktop"),
                    };
                    if ui.add(pin).on_hover_text(tip).clicked() {
                        self.toggle_pin(list);
                    }
                }
                // Planned is always grouped by date, so sorting doesn't apply there.
                if self.view == View::Planned && !searching {
                    return;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut sort = self.store.settings.sort;
                    egui::ComboBox::from_id_salt("sort")
                        .width(if narrow { 64.0 } else { 160.0 })
                        .selected_text(if narrow { tr("Sort").to_string() } else { trf("Sort: {order}", &[("order", &sort.label())]) })
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
                ui.label(RichText::new(i18n::date_long(today)).weak());
            }
            ui.add_space(8.0);

            if !searching && self.view != View::Completed {
                let min = field_height(ui);
                // An Add button appears once there's text, so a task can be saved without
                // the keyboard's Enter key (e.g. after the on-screen keyboard was hidden).
                let has_text = !self.new_task.trim().is_empty();
                let (r, add_clicked) = ui
                    .horizontal(|ui| ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let add = has_text
                            && ui.add(egui::Button::new(RichText::new(tr("Add")).size(if touch { 17.0 } else { 15.0 })).min_size(Vec2::new(min.y * 1.6, min.y))).clicked();
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.new_task)
                                // A fixed id: the Add button appearing must not make it a new field.
                                .id(Id::new("add_task_field"))
                                .hint_text(if platform::IS_MOBILE_OR_WEB || narrow || touch {
                                    tr("+  Add a task")
                                } else {
                                    tr("+  Add a task   (Enter to save, Ctrl+N)")
                                })
                                .font(FontId::proportional(if touch { 17.0 } else { 15.0 }))
                                .min_size(min)
                                .vertical_align(Align::Center)
                                .desired_width(ui.available_width()),
                        );
                        (r, add)
                    }).inner)
                    .inner;
                if self.focus_add {
                    r.request_focus();
                    self.focus_add = false;
                }
                if add_clicked || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                    if let Some(id) = self.store.add_task(&self.new_task, self.view, now) {
                        self.new_task.clear();
                        // Its "Add a subtask" box opens right under it (the previous new
                        // task's box closes if nothing was added to it).
                        if let Some((prev, _)) = self.last_added.take() {
                            if self.store.task(prev).is_some_and(|t| t.steps.is_empty()) {
                                self.expanded.remove(&prev);
                            }
                        }
                        self.expanded.insert(id);
                        self.last_added = Some((id, self.view));
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
                let expanded = &self.expanded;
                let step_inputs = &mut self.step_inputs;
                let mut row = |ui: &mut egui::Ui, t: &Task, actions: &mut Vec<Action>, order: Option<&[Uuid]>| {
                    let open = expanded.contains(&t.id);
                    let r = task_row(ui, store, t, view, show_list, selected == Some(t.id), today, open, actions, order);
                    if open {
                        subtask_rows(ui, t, step_inputs.entry(t.id).or_default(), actions);
                    }
                    r
                };

                if searching {
                    let mut found: Vec<&Task> =
                        store.tasks.iter().filter(|t| t.matches_query(&query)).collect();
                    sort_tasks(&mut found, sort);
                    found.sort_by_key(|t| t.done);
                    if found.is_empty() {
                        empty_state(ui, "🔍", tr("No matching tasks."));
                    }
                    for t in found {
                        row(ui, t, &mut actions, None);
                    }
                    return;
                }

                if view == View::Completed {
                    let mut done: Vec<&Task> = store.tasks.iter().filter(|t| t.done).collect();
                    done.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
                    if done.is_empty() {
                        empty_state(ui, "✔", tr("Completed tasks will show up here."));
                    }
                    for t in done {
                        row(ui, t, &mut actions, None);
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
                        View::MyDay if touch => ("☀", tr("Focus on your day. Add a task, or long-press any task and choose “Add to My Day”.")),
                        View::MyDay => ("☀", tr("Focus on your day. Add a task, or right-click any task and choose “Add to My Day”.")),
                        View::Important => ("★", tr("Star tasks to see them here.")),
                        View::Planned => ("📅", tr("Tasks with due dates or reminders show up here.")),
                        _ => ("✔", tr("Nothing here yet. Add a task above.")),
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
                            row(ui, t, &mut actions, None);
                        }
                    }
                } else {
                    sort_tasks(&mut open, sort);
                    // "My order": drag a task (desktop) or use Move up / down in its menu.
                    let ids: Vec<Uuid> = open.iter().map(|t| t.id).collect();
                    let drag_id = Id::new(("task_drag", view));
                    let mut rects: Vec<(Uuid, Rect)> = Vec::new();
                    for t in open {
                        let r = row(ui, t, &mut actions, Some(&ids));
                        if r.drag_started() && !touch {
                            ui.ctx().data_mut(|d| d.insert_temp(drag_id, t.id));
                        }
                        rects.push((t.id, r.rect));
                    }
                    let dragged: Option<Uuid> = ui.ctx().data(|d| d.get_temp(drag_id));
                    if let Some(dragged) = dragged {
                        let (pointer, released) = ui.input(|i| (i.pointer.interact_pos(), !i.pointer.primary_down()));
                        match pointer {
                            Some(pos) if ids.contains(&dragged) => {
                                let others: Vec<&(Uuid, Rect)> = rects.iter().filter(|(id, _)| *id != dragged).collect();
                                let to = others.iter().filter(|(_, r)| r.center().y < pos.y).count();
                                let y = match others.get(to) {
                                    Some((_, r)) => r.top() - 1.0,
                                    None => others.last().map_or(pos.y, |(_, r)| r.bottom() + 1.0),
                                };
                                ui.painter().hline(ui.min_rect().x_range(), y, Stroke::new(2.0_f32, ACCENT));
                                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                                notes_ui::edge_scroll(ui, pos);
                                if released {
                                    actions.push(Action::MoveTask(ids.clone(), dragged, to));
                                    ui.ctx().data_mut(|d| d.remove::<Uuid>(drag_id));
                                }
                            }
                            _ if released => ui.ctx().data_mut(|d| d.remove::<Uuid>(drag_id)),
                            _ => {}
                        }
                    }
                }

                if !done.is_empty() {
                    done.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
                    ui.add_space(6.0);
                    egui::CollapsingHeader::new(
                        RichText::new(format!("{}  {}", tr("Completed"), done.len())).strong(),
                    )
                    .id_salt(("completed", view))
                    .default_open(false)
                    .show(ui, |ui| {
                        for t in done {
                            row(ui, t, &mut actions, None);
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
            // Take a new reading only once the next one agrees. While the keyboard slides
            // in or out, Android can report two different answers in turn, and following
            // each made the page jump up and down (most visibly at "+ New list", low on
            // the lists page). The very first reading is taken straight away.
            let first = self.insets == [0.0; 4];
            if fresh == self.insets {
                self.insets_pending = None;
            } else if first || self.insets_pending == Some(fresh) {
                self.insets = fresh;
                self.insets_pending = None;
                ctx.request_repaint();
            } else {
                self.insets_pending = Some(fresh);
                ctx.request_repaint_after(StdDuration::from_millis(260));
            }
            self.insets_checked = Some(Instant::now());
        }
        // Where the on-screen keyboard starts (screen y), for fields to scroll above it. Only
        // a tall bottom inset is the keyboard (the navigation bar is always there).
        let screen = ctx.screen_rect();
        let keyboard_top = (self.insets[3] > screen.height() * 0.2).then(|| screen.bottom() - self.insets[3]);
        ctx.data_mut(|d| d.insert_temp(Id::new(KEYBOARD_TOP_ID), keyboard_top));
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
        // A task being moved by its ≡ handle isn't a pull (dragging it down would refresh).
        let moving_task = ui.ctx().data(|d| d.get_temp::<Uuid>(Id::new(("task_drag", self.view)))).is_some();
        if moving_task {
            self.pull_active = false;
            self.pull = 0.0;
            return;
        }
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
            if ready { tr("Release to refresh") } else { tr("Pull to refresh") },
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
                        self.notify(tr("Refreshed."));
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ details

    /// Task details: a right side panel on wide screens, the whole screen on phones.
    fn detail_panel(&mut self, ctx: &egui::Context, id: Uuid, now: NaiveDateTime, narrow: bool) {
        let side = self.side_frame(ctx);
        let today = now.date();
        let clock_24h = self.store.settings.clock_24h;
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
                        if back_button(ui, tr("Back")).clicked() {
                            close = true;
                        }
                        return;
                    }
                    ui.label(RichText::new(tr("Details")).weak());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("✖").on_hover_text(tr("Close (Esc)")).clicked() {
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
                                    if small_btn(ui, "✖").on_hover_text(tr("Delete subtask")).clicked() {
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
                        // New subtask: a visible field, a clickable "+", and an Add button once
                        // there's text (so nobody has to know that Enter saves). Laid out from
                        // the right so the field takes exactly the room left: a fixed guess for
                        // the button's width made the row overflow, and the details panel then
                        // widened itself a little more every frame.
                        ui.horizontal(|ui| {
                            let field_id = Id::new(("new_step", id));
                            let touch = is_touch(ui);
                            let plus = egui::Button::new(RichText::new("+").color(ACCENT).strong().size(if touch { 22.0 } else { 18.0 }))
                                .frame(false)
                                .min_size(Vec2::splat(if touch { 40.0 } else { 24.0 }));
                            let plus = ui.add(plus).on_hover_text(tr("Add a subtask"));
                            let has_text = !new_step.trim().is_empty();
                            let mut add = false;
                            let mut enter = false;
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                add = has_text && ui.button(tr("Add")).clicked();
                                let r = ui.add(
                                    egui::TextEdit::singleline(new_step)
                                        .id(field_id)
                                        .hint_text(if task.steps.is_empty() { tr("Add a subtask") } else { tr("Add another subtask") })
                                        .min_size(field_height(ui))
                                        .vertical_align(Align::Center)
                                        .desired_width(ui.available_width()),
                                );
                                enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                            });
                            if (plus.clicked() || add || enter) && !new_step.trim().is_empty() {
                                task.steps.push(Step::new(new_step.trim()));
                                new_step.clear();
                                changed = true;
                            }
                            // Keep typing the next one straight away.
                            if plus.clicked() || add || enter {
                                ui.memory_mut(|m| m.request_focus(field_id));
                            }
                        });
                    });

                    // My Day
                    card(ui, |ui| {
                        let in_day = task.in_my_day(today);
                        let (text, color) = if in_day && is_touch(ui) {
                            (tr("☀   Added to My Day  (tap to remove)"), ACCENT)
                        } else if in_day {
                            (tr("☀   Added to My Day  (click to remove)"), ACCENT)
                        } else {
                            (tr("☀   Add to My Day"), ui.visuals().text_color())
                        };
                        if ui.add(egui::Button::new(RichText::new(text).color(color)).frame(false)).clicked() {
                            task.my_day = if in_day { None } else { Some(today) };
                            changed = true;
                        }
                    });

                    // Reminder, due date, repeat
                    card(ui, |ui| {
                        ui.label(RichText::new(tr("🔔   Remind me")).strong());
                        match task.reminder {
                            Some(r) => {
                                ui.horizontal(|ui| {
                                    let mut date = r.date();
                                    let mut h = r.hour();
                                    let mut m = r.minute();
                                    ui.add(egui_extras::DatePickerButton::new(&mut date).id_salt("reminder_date"));
                                    // Hours as the user reads them: 0-23, or 1-12 plus an AM/PM switch.
                                    let (mut h12, mut pm) = to_12h(h);
                                    if clock_24h {
                                        ui.add(egui::DragValue::new(&mut h).range(0..=23).custom_formatter(|v, _| format!("{:02}", v as u32)));
                                    } else {
                                        ui.add(egui::DragValue::new(&mut h12).range(1..=12));
                                    }
                                    ui.label(":");
                                    ui.add(egui::DragValue::new(&mut m).range(0..=59).speed(0.5).custom_formatter(|v, _| format!("{:02}", v as u32)));
                                    if !clock_24h {
                                        if ui.button(if pm { "PM" } else { "AM" }).on_hover_text(tr("Switch AM / PM")).clicked() {
                                            pm = !pm;
                                        }
                                        h = from_12h(h12, pm);
                                    }
                                    let new_r = date.and_hms_opt(h, m, 0).unwrap_or(r);
                                    if new_r != r {
                                        task.reminder = Some(new_r);
                                        task.reminder_fired = new_r <= now;
                                        changed = true;
                                    }
                                    if small_btn(ui, "✖").on_hover_text(tr("Remove reminder")).clicked() {
                                        task.reminder = None;
                                        task.reminder_fired = false;
                                        changed = true;
                                    }
                                });
                                if let Some(r) = task.reminder {
                                    ui.label(RichText::new(trf("{date} at {time}", &[("date", &relative_date(r.date(), today)), ("time", &format_time(r, clock_24h))])).weak().small());
                                }
                            }
                            None => {
                                ui.horizontal_wrapped(|ui| {
                                    for (label, when) in reminder_presets(now, clock_24h) {
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

                        ui.label(RichText::new(tr("📅   Due date")).strong());
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
                                    if small_btn(ui, "✖").on_hover_text(tr("Remove due date")).clicked() {
                                        task.due = None;
                                        changed = true;
                                    }
                                });
                            }
                            None => {
                                ui.horizontal_wrapped(|ui| {
                                    let presets = [
                                        (tr("Today"), today),
                                        (tr("Tomorrow"), today + Duration::days(1)),
                                        (tr("Next week"), next_monday(today)),
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
                            ui.label(RichText::new(tr("🔁   Repeat")).strong());
                            let mut choice = (task.recurrence, task.repeat_every.max(1));
                            // The usual choices, plus "every 2 weeks"; any other interval is
                            // set with "Every [N]" below.
                            let mut options: Vec<(Recurrence, u32)> = Recurrence::ALL.iter().map(|r| (*r, 1)).collect();
                            options.insert(4, (Recurrence::Weekly, 2));
                            egui::ComboBox::from_id_salt("repeat")
                                .selected_text(choice.0.label_every(choice.1))
                                .show_ui(ui, |ui| {
                                    for (r, every) in options {
                                        ui.selectable_value(&mut choice, (r, every), r.label_every(every));
                                    }
                                });
                            if choice != (task.recurrence, task.repeat_every.max(1)) {
                                task.recurrence = choice.0;
                                task.repeat_every = if choice.0.has_interval() { choice.1 } else { 1 };
                                // A repeating task needs a date to repeat from.
                                if choice.0 != Recurrence::Never && task.planned_date().is_none() {
                                    task.due = Some(today);
                                }
                                changed = true;
                            }
                        });
                        // Any interval: every 14 days, every 3 months, …
                        if task.recurrence.has_interval() {
                            ui.horizontal(|ui| {
                                ui.add_space(28.0);
                                ui.label(tr("Every"));
                                let mut n = task.repeat_every.max(1);
                                if ui.add(egui::DragValue::new(&mut n).range(1..=365).speed(0.1)).changed() {
                                    task.repeat_every = n;
                                    changed = true;
                                }
                                let one = n == 1;
                                ui.label(match task.recurrence {
                                    Recurrence::Daily => if one { tr("day") } else { tr("days") },
                                    Recurrence::Weekly => if one { tr("week") } else { tr("weeks") },
                                    Recurrence::Monthly => if one { tr("month") } else { tr("months") },
                                    _ => if one { tr("year") } else { tr("years") },
                                });
                            });
                        }
                    });

                    // List
                    card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr("☰   List")).strong());
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
                                .hint_text(tr("Add note"))
                                .desired_rows(6)
                                .frame(false)
                                .desired_width(f32::INFINITY),
                        );
                        changed |= r.changed();
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let created = if task.created.date() == today {
                            tr("Created today").to_string()
                        } else {
                            trf("Created {date}", &[("date", &relative_date(task.created.date(), today))])
                        };
                        let info = match task.completed_at {
                            Some(c) => trf("Completed {date}", &[("date", &relative_date(c.date(), today))]),
                            None => created,
                        };
                        ui.label(RichText::new(info).weak().small());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(RichText::new(tr("🗑 Delete")).color(DANGER)))
                                .on_hover_text(tr("Delete task (Del)"))
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
                .frame(side)
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

    /// Reminders that came due while RustDo is open: one box in the corner (bottom centre on
    /// phones) listing them all, which collapses to its header. A new reminder opens it again.
    fn alert_windows(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let store = &self.store;
        self.alerts.retain(|id| store.task(*id).is_some_and(|t| !t.done));
        let count = self.alerts.len();
        let grew = count > self.alerts_shown;
        self.alerts_shown = count;
        if count == 0 {
            return;
        }
        let mut acted = Vec::new();
        let narrow = ctx.screen_rect().width() < NARROW_WIDTH;
        let (align, x) = if narrow { (Align2::CENTER_BOTTOM, 0.0) } else { (Align2::RIGHT_BOTTOM, -16.0) };
        // Never taller than ~60% of the window; more reminders scroll.
        let max_list_h = (ctx.screen_rect().height() * 0.6).max(150.0);
        egui::Window::new(tr("Reminders"))
            .id(Id::new("alerts"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            // Above the Android navigation bar.
            .anchor(align, Vec2::new(x, -16.0 - self.insets[3]))
            .show(ctx, |ui| {
                ui.set_width(dialog_width(ui, 300.0));
                if grew {
                    self.alerts_open = true;
                }
                let open = self.alerts_open;
                ui.horizontal(|ui| {
                    let size = Vec2::splat(ui.spacing().icon_width);
                    let (_, icon) = ui.allocate_exact_size(size, Sense::click());
                    egui::collapsing_header::paint_default_icon(ui, if open { 1.0 } else { 0.0 }, &icon);
                    let title = if count == 1 { "🔔 Reminder".to_string() } else { format!("🔔 {count} reminders") };
                    let label = ui.add(egui::Label::new(RichText::new(title).strong().size(16.0)).sense(Sense::click()));
                    if (icon | label).on_hover_text(if open { tr("Collapse") } else { tr("Expand") }).clicked() {
                        self.alerts_open = !open;
                    }
                    if count > 1 {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if small_btn(ui, tr("Dismiss all")).clicked() {
                                for id in &self.alerts {
                                    acted.push((*id, AlertAction::Dismiss));
                                }
                            }
                        });
                    }
                });
                if open {
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(max_list_h).show(ui, |ui| {
                        for (i, id) in self.alerts.iter().enumerate() {
                            let Some(t) = self.store.task(*id) else { continue };
                            if i > 0 {
                                ui.separator();
                            }
                            ui.label(RichText::new(&t.title).strong());
                            if let Some(due) = t.due {
                                ui.label(RichText::new(trf("Due {date}", &[("date", &relative_date(due, now.date()))])).weak().small());
                            }
                            ui.horizontal_wrapped(|ui| {
                                if ui.button(tr("✔ Complete")).clicked() {
                                    acted.push((*id, AlertAction::Complete));
                                }
                                if ui.button(tr("Open")).clicked() {
                                    acted.push((*id, AlertAction::Open));
                                }
                                if ui.button(tr("Dismiss")).clicked() {
                                    acted.push((*id, AlertAction::Dismiss));
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(tr("Snooze:")).weak());
                                for (label, mins) in [(tr("10 min"), 10), (tr("1 hour"), 60), (tr("Tomorrow"), 24 * 60)] {
                                    if small_btn(ui, label).clicked() {
                                        acted.push((*id, AlertAction::Snooze(mins)));
                                    }
                                }
                            });
                        }
                    });
                }
            });
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
        dialog_window(ctx, tr("Delete list?"), self.insets)
            .show(ctx, |ui| {
                ui.set_max_width(dialog_width(ui, 380.0));
                ui.label(if count == 1 {
                    trf("“{name}” and its 1 task will be permanently deleted.", &[("name", &name)])
                } else {
                    trf("“{name}” and its {count} tasks will be permanently deleted.", &[("name", &name), ("count", &count)])
                });
                if self.store.shares.iter().any(|s| s.list_id == id) {
                    ui.label(RichText::new(tr("It's shared: this only removes it from this device. Everyone else keeps it.")).weak().small());
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(tr("Delete list")).color(Color32::WHITE)).fill(DANGER)).clicked() {
                        confirm = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            if self.store.delete_list(id) {
                if self.view == View::List(id) {
                    self.view = View::List(self.store.default_list);
                    self.notes_page = false;
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
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0 - self.insets[3]))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width((ctx.screen_rect().width() - 80.0).clamp(200.0, 620.0));
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(&toast.text).wrap());
                        if toast.undo.is_some() && ui.button(tr("Undo")).clicked() {
                            undo = true;
                        }
                        if small_btn(ui, "✖").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if undo {
            match self.toast.take().and_then(|t| t.undo) {
                Some(UndoOp::Deleted(idx, task)) => {
                    let id = task.id;
                    self.store.restore_task(idx, task);
                    if !is_touch_ctx(ctx) {
                        self.selected = Some(id);
                    }
                    self.dirty = true;
                }
                Some(UndoOp::Completed { id, next }) => {
                    // The next repeat goes again, and the task repeats as before.
                    if let Some((_, next_task)) = next.and_then(|n| self.store.remove_task(n)) {
                        if let Some(t) = self.store.task_mut(id) {
                            t.recurrence = next_task.recurrence;
                            t.repeat_every = next_task.repeat_every;
                        }
                    }
                    self.store.set_done(id, false, Local::now().naive_local());
                    self.dirty = true;
                }
                Some(UndoOp::Archived(id)) => notes_ui::lock(&self.notes).archive(id, false),
                None => {}
            }
        } else if close {
            self.toast = None;
        }
    }
}

impl eframe::App for TodoApp {
    /// Don't lose the last second of typing in a sticky note.
    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        notes_ui::lock(&self.notes).save_if_due(true);
    }

    /// Android: keep the font atlas small enough for 16-bit texture coordinates.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if cfg!(target_os = "android") {
            raw_input.max_texture_side = capped_texture_side(raw_input.max_texture_side);
        }
    }

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
        self.sync_tick(ctx);
        self.account_tick(ctx);
        self.poll_updates(ctx);
        self.handle_dropped_files(ctx);
        #[cfg(target_arch = "wasm32")]
        {
            let picked = self.web_import.borrow_mut().take();
            if let Some((name, bytes)) = picked {
                self.load_import(&name, &bytes);
            }
        }

        // Phones: back (Android's back button or gesture arrives as Escape) steps back one
        // screen at a time. Taken here so no other part of the app also acts on it.
        if ctx.screen_rect().width() < NARROW_WIDTH && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.phone_back(ctx);
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

        apply_text_scale(ctx, self.store.settings.crisp_text);
        // Must come before the other panels so it sits at the very edges.
        self.apply_safe_insets(ctx);

        // Wide screens: lists | tasks | details side by side.
        // Phones: one screen at a time (lists, or tasks, or one task's details).
        // Web: a picture picked in the browser.
        let picked = self.bg_web_pick.borrow_mut().take();
        if let Some((target, bytes)) = picked {
            self.add_picture(target, &bytes);
        }
        // A sticky note asked for a picture (web and Android; the desktop picks it there).
        if let Some(note) = notes_ui::take_picture_request(ctx) {
            self.pick_picture(ctx, PicFor::Note(note));
        }
        for id in std::mem::take(&mut self.bg_forget) {
            notes_ui::forget_picture(ctx, &id);
        }
        self.scan_pictures();
        #[cfg(target_os = "android")]
        self.photo_picker_dialog(ctx);
        let showing = self.paint_background(ctx);
        self.apply_background_style(ctx, showing);
        self.background_dialog(ctx);

        let narrow = ctx.screen_rect().width() < NARROW_WIDTH;
        if let Some(view) = self.show_list {
            self.show_list_page(ctx, view, today);
        } else if narrow {
            if self.show_settings {
                self.settings_page(ctx);
            } else if self.show_nav {
                self.sidebar(ctx, today, true);
            } else if self.notes_page {
                self.notes_panel(ctx);
            } else if let Some(id) = self.selected {
                self.detail_panel(ctx, id, now, true);
            } else {
                self.main_panel(ctx, now, true);
            }
        } else {
            self.show_nav = false;
            self.sidebar(ctx, today, false);
            // Settings take the right-hand pane while open (instead of a task's details).
            if self.show_settings {
                self.settings_pane(ctx);
            }
            if self.notes_page {
                self.notes_panel(ctx);
            } else {
                if let (Some(id), false) = (self.selected, self.show_settings) {
                    self.detail_panel(ctx, id, now, false);
                }
                self.main_panel(ctx, now, false);
            }
        }

        // Android needs to be told to show the on-screen keyboard for text fields.
        // Android hides the keyboard by itself (its hide key, the back gesture, switching
        // apps) without telling the app, while the text field stays selected. So besides
        // following the selected field, ask for the keyboard again whenever a selected field
        // is tapped and when RustDo comes back to the front.
        let wants_keyboard = ctx.wants_keyboard_input();
        let (tapped, focused) = ctx.input(|i| (i.pointer.any_released(), i.viewport().focused));
        let came_back = focused == Some(true) && self.window_focused == Some(false);
        self.window_focused = focused.or(self.window_focused);
        if wants_keyboard != self.keyboard_shown || (wants_keyboard && (tapped || came_back)) {
            platform::show_keyboard(wants_keyboard);
            self.keyboard_shown = wants_keyboard;
        }

        self.companion_tick(ctx);
        self.close_hint_check(ctx);
        self.pinned_tick(ctx, now);
        // While the companion runs, it shows the notes (they stay up after this window closes).
        if !(self.store.settings.companion && self.companion_running) {
            notes_ui::show_windows(ctx, &self.notes);
        }
        if notes::AVAILABLE && notes_ui::lock(&self.notes).take_backup_pending() {
            self.backup_pending = true;
        }
        self.alert_windows(ctx, now);
        self.delete_list_dialog(ctx);
        self.share_dialog(ctx);
        self.import_dialog(ctx, now);
        self.restore_dialog(ctx);
        self.whats_new_dialog(ctx);
        self.get_apps_dialog(ctx);
        self.hub_guide_dialog(ctx);
        self.account_dialog(ctx);
        self.ms_import_dialog(ctx);
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
fn is_touch_ctx(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::new(TOUCH_ID))).unwrap_or(false)
}

pub(crate) fn is_touch(ui: &egui::Ui) -> bool {
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

/// Widest font atlas whose texture coordinates are still exact in 16-bit floats.
///
/// egui makes the font atlas as wide as the GPU allows (often 8192 or 16384 px) and its
/// GLES shaders use `mediump` texture coordinates. Most phone GPUs quietly compute those at
/// full precision, but some (e.g. the PowerVR GPU in the Pixel 10 series) really use 16-bit
/// floats: on a 16384 px atlas a coordinate is then only accurate to ~8 texels, so every
/// letter is sampled a few pixels to the side, cut off on one edge and showing a sliver of
/// its neighbour. At 2048 px the error stays under half a texel, inside the 1 px padding
/// egui leaves between glyphs.
const MAX_ATLAS_SIDE: usize = 2048;

fn capped_texture_side(side: Option<usize>) -> Option<usize> {
    Some(side.map_or(MAX_ATLAS_SIDE, |s| s.min(MAX_ATLAS_SIDE)))
}

/// The part of an image (as UV coordinates) that fills `area` without stretching:
/// cropped to the area's shape, centred, like CSS `background-size: cover`.
/// Web: a picture picked in the browser, waiting to be stored.
type WebPick = std::rc::Rc<std::cell::RefCell<Option<(PicFor, Vec<u8>)>>>;

/// The sections of Settings, shown one at a time as tabs.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum SettingsTab {
    #[default]
    Appearance,
    Reminders,
    Sync,
    Backups,
    About,
}

impl SettingsTab {
    const ALL: [SettingsTab; 5] = [Self::Appearance, Self::Reminders, Self::Sync, Self::Backups, Self::About];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => tr("Appearance"),
            Self::Reminders => tr("Reminders"),
            Self::Sync => tr("Sync & sharing"),
            Self::Backups => tr("Backups"),
            Self::About => tr("About"),
        }
    }
}

/// What a picked picture is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PicFor {
    /// The app-wide background.
    App,
    /// One list's background.
    List(Uuid),
    /// A sticky note.
    Note(Uuid),
}

pub(crate) fn cover_uv(image: Vec2, area: Vec2) -> Rect {
    if image.x <= 0.0 || image.y <= 0.0 || area.x <= 0.0 || area.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let (ia, aa) = (image.x / image.y, area.x / area.y);
    if ia > aa {
        // Image is wider: crop the sides.
        let w = aa / ia;
        Rect::from_min_max(Pos2::new((1.0 - w) / 2.0, 0.0), Pos2::new((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ia / aa;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - h) / 2.0), Pos2::new(1.0, (1.0 + h) / 2.0))
    }
}

/// "💬 Ideas or bugs? Join the RustDo Discord community!" across the bottom of the sidebar.
fn discord_button(ui: &mut egui::Ui) {
    let button = egui::Button::new(RichText::new(tr("💬 Ideas or bugs? Join the RustDo Discord community!")).color(Color32::WHITE))
        .fill(DISCORD_BLUE)
        .wrap()
        .min_size(Vec2::new(ui.available_width(), 30.0));
    if ui.add(button).on_hover_text(tr("Chat, ideas and help: opens in your browser")).on_hover_cursor(CursorIcon::PointingHand).clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(update::DISCORD));
    }
}

/// Where the on-screen keyboard starts, when it's up (Android).
const KEYBOARD_TOP_ID: &str = "rustdo_keyboard_top";

/// A selected text field that the on-screen keyboard covers scrolls up into view, so
/// what's being typed can be seen. Call it right after adding the field.
fn keep_above_keyboard(r: &egui::Response) {
    if !r.has_focus() {
        return;
    }
    let top: Option<f32> = r.ctx.data(|d| d.get_temp::<Option<f32>>(Id::new(KEYBOARD_TOP_ID))).flatten();
    if let Some(top) = top {
        if r.rect.bottom() > top - 8.0 {
            r.scroll_to_me(Some(Align::Center));
        }
    }
}

/// A copy button that says "✔ Copied" for a moment after it's pressed (so it's clear it
/// worked: on a phone nothing else shows it).
fn copy_button(ui: &mut egui::Ui, label: &str, text: impl FnOnce() -> String) {
    let id = Id::new(("copied", label));
    let now = ui.input(|i| i.time);
    let recent = ui.data(|d| d.get_temp::<f64>(id)).is_some_and(|t| now - t < 2.0);
    let shown = if recent { tr("✔ Copied") } else { label };
    if ui.button(shown).clicked() {
        platform::copy_text(ui.ctx(), text());
        ui.data_mut(|d| d.insert_temp(id, now));
        ui.ctx().request_repaint_after(StdDuration::from_millis(2100));
    }
}

/// A one-line text field with a "📋 Paste" button beside it, and Paste when it's
/// right-clicked (egui text boxes have no menu of their own, and phones have no Ctrl+V).
/// The pasted text replaces what's in the field.
fn field_with_paste(ui: &mut egui::Ui, value: &mut String, hint: &str, target: &'static str) -> egui::Response {
    if let Some(text) = platform::take_paste(ui.ctx(), target) {
        *value = text.trim().to_string();
    }
    // Right-click (or long-press) the field: Paste, like other apps' text boxes.
    let menu = |r: &egui::Response| {
        r.context_menu(|ui| {
            if ui.button(tr("📋 Paste")).clicked() {
                platform::request_paste(ui.ctx(), target);
                ui.close_menu();
            }
        });
    };
    if !platform::CAN_PASTE {
        let r = ui.add(egui::TextEdit::singleline(value).hint_text(hint).min_size(field_height(ui)).desired_width(f32::INFINITY));
        menu(&r);
        keep_above_keyboard(&r);
        return r;
    }
    ui.horizontal(|ui| {
        let button_w = if is_touch(ui) { 96.0 } else { 80.0 };
        let r = ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .min_size(field_height(ui))
                .vertical_align(Align::Center)
                .desired_width((ui.available_width() - button_w - ui.spacing().item_spacing.x).max(80.0)),
        );
        menu(&r);
        keep_above_keyboard(&r);
        if ui.add(egui::Button::new(tr("📋 Paste")).min_size(Vec2::new(button_w, field_height(ui).y))).clicked() {
            platform::request_paste(ui.ctx(), target);
        }
        r
    })
    .inner
}

/// Phones: "‹ Back" as a clear, thumb-sized button. The chevron is drawn (the built-in
/// fonts have no good arrow), in a rounded pill so it reads as something to tap.
fn back_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let touch = is_touch(ui);
    let height = if touch { 48.0 } else { 36.0 };
    let font = egui::FontId::proportional(if touch { 19.0 } else { 16.0 });
    let text_color = ui.visuals().strong_text_color();
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, text_color);
    let chevron_w = height * 0.32;
    let pad = height * 0.32;
    let width = if label.is_empty() { pad * 2.0 + chevron_w } else { pad + chevron_w + pad * 0.6 + galley.size().x + pad };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let pill = rect.shrink2(Vec2::new(0.0, height * 0.1));
        let fill = if response.is_pointer_button_down_on() {
            ui.visuals().widgets.active.weak_bg_fill
        } else if response.hovered() {
            ui.visuals().widgets.hovered.weak_bg_fill
        } else {
            ui.visuals().widgets.inactive.weak_bg_fill
        };
        ui.painter().rect_filled(pill, egui::CornerRadius::same((pill.height() / 2.0) as u8), fill);
        // The chevron: two strokes meeting at a point, in the accent colour.
        let cx = if label.is_empty() { pill.center().x - chevron_w * 0.1 } else { pill.left() + pad + chevron_w * 0.35 };
        let cy = pill.center().y;
        let half = chevron_w * 0.75;
        let stroke = Stroke::new(if touch { 3.0_f32 } else { 2.5 }, ACCENT);
        ui.painter().line_segment([Pos2::new(cx + half * 0.6, cy - half), Pos2::new(cx - half * 0.4, cy)], stroke);
        ui.painter().line_segment([Pos2::new(cx - half * 0.4, cy), Pos2::new(cx + half * 0.6, cy + half)], stroke);
        let text_pos = Pos2::new(pill.left() + pad + chevron_w + pad * 0.6, cy - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, text_color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A dialog that fits the screen. Wide screens: centred. Phones: near the top (below the
/// status bar), so the on-screen keyboard doesn't cover its text fields. Long content
/// (import summaries, pasted backups) scrolls inside the dialog's own ScrollArea.
fn dialog_window<'a>(ctx: &egui::Context, title: impl Into<egui::WidgetText>, insets: [f32; 4]) -> egui::Window<'a> {
    let window = egui::Window::new(title).collapsible(false).resizable(false);
    // The on-screen keyboard is up (Android): move the dialog to the top, into the space
    // above it. Only its position changes: making it scroll here (0.4.11) wrapped its
    // contents in a new scroll area, which egui treats as new widgets, so the selected
    // field lost focus, the keyboard closed, the dialog moved back and the keyboard opened
    // again, over and over. The bottom inset is also the navigation bar, which is always
    // there: only a tall one is the keyboard.
    if insets[3] > ctx.screen_rect().height() * 0.2 {
        return window.anchor(Align2::CENTER_TOP, Vec2::new(0.0, insets[1] + 16.0));
    }
    if ctx.screen_rect().width() < NARROW_WIDTH {
        window.anchor(Align2::CENTER_TOP, Vec2::new(0.0, insets[1] + 16.0))
    } else {
        window.anchor(Align2::CENTER_CENTER, Vec2::ZERO)
    }
}

/// A dialog's content width: what it would like, or less on a narrow screen (leaving
/// room for the window's own margins and a gap to the screen edges).
fn dialog_width(ui: &egui::Ui, preferred: f32) -> f32 {
    preferred.min(ui.ctx().screen_rect().width() - 48.0).max(160.0)
}

/// Android: draw at a whole-number scale. egui places every glyph on the pixel grid, so
/// at a fractional density (2.625x on many Pixels) letters land up to half a pixel
/// apart and text looks uneven and fuzzy. Rounding the scale (2.625x -> 3x) keeps every
/// glyph sharp and evenly spaced, at the cost of the UI being slightly larger or smaller.
fn apply_text_scale(ctx: &egui::Context, crisp: bool) {
    // Android, and the web app on phones and tablets (fractional screen scales squeeze
    // the letters there). Desktop browsers keep their own zoom.
    if !cfg!(target_os = "android") && !platform::web_touch_screen() {
        return;
    }
    let Some(native) = ctx.native_pixels_per_point().filter(|p| *p > 0.0) else {
        return;
    };
    let zoom = if crisp { native.round().max(1.0) / native } else { 1.0 };
    if (ctx.zoom_factor() - zoom).abs() > 0.001 {
        ctx.set_zoom_factor(zoom);
    }
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

fn reminder_presets(now: NaiveDateTime, clock_24h: bool) -> Vec<(String, NaiveDateTime)> {
    let today = now.date();
    let nine = |d: NaiveDate| d.and_hms_opt(9, 0, 0).unwrap();
    let mut out = Vec::new();
    // "Later today": three hours from now, rounded up to the hour.
    let later = (now + Duration::hours(3)).with_minute(0).and_then(|t| t.with_second(0));
    if let Some(later) = later.filter(|l| l.date() == today) {
        out.push((tr("Later today").to_string(), later));
    }
    let tomorrow = nine(today + Duration::days(1));
    out.push((trf("Tomorrow {time}", &[("time", &format_time(tomorrow, clock_24h))]), tomorrow));
    out.push((tr("Next week").to_string(), nine(next_monday(today))));
    out
}

fn nav_row(ui: &mut egui::Ui, selected: bool, icon: &str, name: &str, count: usize) -> egui::Response {
    let touch = is_touch(ui);
    let (height, font, gap) = if touch { (48.0, 17.0, 40.0) } else { (30.0, 14.0, 34.0) };
    // Desktop rows can also be dragged (to reorder lists); on touch, dragging scrolls.
    let sense = if touch { Sense::click() } else { Sense::click_and_drag() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), sense);
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
        .on_hover_text(if done { tr("Mark as not completed") } else { tr("Mark as completed") })
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
        .on_hover_text(if important { tr("Remove importance") } else { tr("Mark as important") })
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
    expanded: bool,
    actions: &mut Vec<Action>,
    order: Option<&[Uuid]>,
) -> egui::Response {
    let vis = ui.visuals().clone();
    let weak = vis.weak_text_color();

    // Second line: list, My Day, steps, due date, reminder, repeat, note.
    let mut meta: Vec<(String, Color32)> = Vec::new();
    if show_list {
        meta.push((store.list_name(t.list_id).to_string(), weak));
    }
    if view != View::MyDay && t.in_my_day(today) {
        meta.push((tr("☀ My Day").into(), weak));
    }
    if !t.steps.is_empty() {
        let n = t.steps.iter().filter(|s| s.done).count();
        meta.push((trf("{n} of {total} subtasks", &[("n", &n), ("total", &t.steps.len())]), weak));
    }
    if let Some(due) = t.due {
        let color = if t.is_overdue(today) {
            DANGER
        } else if due == today && !t.done {
            ACCENT
        } else {
            weak
        };
        let prefix = if t.is_overdue(today) { tr("Overdue, ") } else { "" };
        meta.push((format!("📅 {prefix}{}", relative_date(due, today)), color));
    }
    if let Some(r) = t.reminder {
        if !t.done && !t.reminder_fired {
            meta.push((format!("🔔 {}", format_time(r, store.settings.clock_24h)), weak));
        }
    }
    if t.recurrence != Recurrence::Never {
        let label = if t.repeat_every > 1 && t.recurrence.has_interval() { format!("🔁 {}", t.recurrence.label_every(t.repeat_every)) } else { "🔁".into() };
        meta.push((label, weak));
    }
    if !t.notes.trim().is_empty() {
        meta.push((tr("📝 Note").into(), weak));
    }

    // Touch mode: taller rows, bigger text, and 48pt tap areas for the circle and star.
    let touch = is_touch(ui);
    let (h_one, h_two, title_size, meta_size, hit, radius, inset, text_gap) = if touch {
        (56.0, 66.0, 17.0, 13.5, 48.0, 12.0, 28.0, 56.0)
    } else {
        (42.0, 52.0, 14.5, 11.5, 24.0, 9.5, 22.0, 44.0)
    };
    let height = if meta.is_empty() { h_one } else { h_two };
    // Desktop: open tasks can be dragged to arrange them ("My order"); on touch, dragging
    // scrolls, and the task's menu has Move up / down instead.
    let sense = if order.is_some() && !touch { Sense::click_and_drag() } else { Sense::click() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), sense);
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
    // Subtasks: a ▸ / ▾ toggle beside the star shows them under the task.
    let expand = (!t.steps.is_empty()).then(|| {
        let r = Rect::from_center_size(Pos2::new(star_rect.left() - hit / 2.0 - 2.0, rect.center().y), Vec2::splat(hit));
        let resp = ui.interact(r, Id::new(("expand", t.id)), Sense::click());
        // The icon fills the rect it's given: draw it small inside the larger tap area.
        let mut icon = resp.clone();
        icon.rect = Rect::from_center_size(r.center(), Vec2::splat(if touch { 16.0 } else { 12.0 }));
        egui::collapsing_header::paint_default_icon(ui, if expanded { 1.0 } else { 0.0 }, &icon);
        resp
    });

    paint_check(ui.painter(), check_rect.center(), radius, t.done, check.hovered(), &vis);
    paint_star(ui, star_rect.center(), t.important, star.hovered());

    let text_left = rect.left() + text_gap;
    // Touch: a ≡ handle to drag the task up or down (dragging anywhere else on the row
    // scrolls the list, as usual on a phone). Moving a task switches to "My order".
    let handle = (touch && order.is_some()).then(|| {
        let left = expand.as_ref().map_or(star_rect.left(), |e| e.rect.left());
        let r = Rect::from_center_size(Pos2::new(left - hit * 0.4 - 2.0, rect.center().y), Vec2::new(hit * 0.8, hit));
        let resp = ui.interact(r, Id::new(("task_handle", t.id)), Sense::drag());
        let color = if resp.dragged() { ACCENT } else { weak };
        for dy in [-5.0, 0.0, 5.0] {
            let y = r.center().y + dy;
            ui.painter().hline((r.center().x - 8.0)..=(r.center().x + 8.0), y, Stroke::new(2.0_f32, color));
        }
        if resp.drag_started() {
            ui.ctx().data_mut(|d| d.insert_temp(Id::new(("task_drag", view)), t.id));
        }
        resp
    });
    let right_edge = handle
        .as_ref()
        .map(|h| h.rect.left())
        .or_else(|| expand.as_ref().map(|e| e.rect.left()))
        .unwrap_or(star_rect.left());
    let text_width = (right_edge - 8.0 - text_left).max(10.0);

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

    // Touch: swipe the row right to tick it off, left to delete (both with Undo). Only a
    // clearly sideways move counts, and not one starting on the circle, star or handle.
    if touch {
        let swipe_id = Id::new(("task_swipe", t.id));
        let (down, origin, pos) = ui.input(|i| (i.pointer.primary_down(), i.pointer.press_origin(), i.pointer.interact_pos()));
        let on_controls = |p: Pos2| {
            check_rect.contains(p) || star_rect.contains(p) || handle.as_ref().is_some_and(|h| h.rect.contains(p)) || expand.as_ref().is_some_and(|e| e.rect.contains(p))
        };
        let started_here = origin.is_some_and(|o| rect.contains(o) && !on_controls(o));
        let threshold = (rect.width() * 0.35).max(90.0);
        if down && started_here {
            if let (Some(o), Some(p)) = (origin, pos) {
                let d = p - o;
                if d.x.abs() > 24.0 && d.x.abs() > d.y.abs() * 1.5 {
                    ui.ctx().data_mut(|m| m.insert_temp(swipe_id, d.x));
                }
            }
        }
        let dx: Option<f32> = ui.ctx().data(|m| m.get_temp(swipe_id));
        if let Some(dx) = dx {
            if down {
                // What the swipe will do, growing from the side it moves away from.
                let w = dx.abs().min(rect.width());
                let ready = dx.abs() >= threshold;
                let (strip, color, icon) = if dx > 0.0 {
                    (Rect::from_min_size(rect.min, Vec2::new(w, rect.height())), Color32::from_rgb(0x2E, 0x9E, 0x5B), "✔")
                } else {
                    (Rect::from_min_max(Pos2::new(rect.right() - w, rect.top()), rect.max), DANGER, "🗑")
                };
                let color = if ready { color } else { color.gamma_multiply(0.6) };
                ui.painter().rect_filled(strip, 6.0, color);
                if w > 40.0 {
                    ui.painter().text(strip.center(), Align2::CENTER_CENTER, icon, FontId::proportional(22.0), Color32::WHITE);
                }
                ui.ctx().request_repaint();
            } else {
                ui.ctx().data_mut(|m| m.remove::<f32>(swipe_id));
                if dx >= threshold && !t.done {
                    actions.push(Action::SwipeDone(t.id));
                } else if dx <= -threshold {
                    actions.push(Action::Delete(t.id));
                }
            }
        }
    }

    let expand_clicked = expand.is_some_and(|e| {
        e.on_hover_text(if expanded { tr("Hide subtasks") } else { tr("Show subtasks") }).on_hover_cursor(CursorIcon::PointingHand).clicked()
    });
    if expand_clicked {
        actions.push(Action::ToggleExpand(t.id));
    } else if check.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        actions.push(Action::ToggleDone(t.id));
    } else if star.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        actions.push(Action::ToggleImportant(t.id));
    } else if resp.clicked() {
        actions.push(Action::Select(t.id));
    }

    resp.context_menu(|ui| {
        let my_day = if t.in_my_day(today) { tr("☀  Remove from My Day") } else { tr("☀  Add to My Day") };
        if ui.button(my_day).clicked() {
            actions.push(Action::ToggleMyDay(t.id));
            ui.close_menu();
        }
        let imp = if t.important { tr("☆  Remove importance") } else { tr("★  Mark as important") };
        if ui.button(imp).clicked() {
            actions.push(Action::ToggleImportant(t.id));
            ui.close_menu();
        }
        if ui.button(tr("+  Add a subtask")).clicked() {
            actions.push(Action::ToggleExpand(t.id));
            ui.ctx().data_mut(|d| d.insert_temp(Id::new(("focus_subtask", t.id)), true));
            ui.close_menu();
        }
        let done = if t.done { tr("○  Mark as not completed") } else { tr("✔  Mark as completed") };
        if ui.button(done).clicked() {
            actions.push(Action::ToggleDone(t.id));
            ui.close_menu();
        }
        if let Some(ids) = order {
            if let Some(i) = ids.iter().position(|x| *x == t.id) {
                ui.separator();
                for (show, label, to) in [
                    (i > 0, tr("⬆  Move up"), i.saturating_sub(1)),
                    (i + 1 < ids.len(), tr("⬇  Move down"), i + 1),
                    (i > 1, tr("⬆  Move to top"), 0),
                ] {
                    if show && ui.button(label).on_hover_text(tr("Switches the list to “My order”")).clicked() {
                        actions.push(Action::MoveTask(ids.to_vec(), t.id, to));
                        ui.close_menu();
                    }
                }
            }
        }
        if notes::AVAILABLE && ui.button(tr("📝  Turn into a sticky note")).on_hover_text(tr("Subtasks become a checklist")).clicked() {
            actions.push(Action::ToNote(t.id));
            ui.close_menu();
        }
        ui.separator();
        if ui.button(RichText::new(tr("🗑  Delete task")).color(DANGER)).clicked() {
            actions.push(Action::Delete(t.id));
            ui.close_menu();
        }
    });
    resp
}

/// Version, system, paths and the end of each log: what's needed to look into a problem.
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
fn diagnostic_report() -> String {
    let mut out = format!("RustDo diagnostic info\n{}\n", diagnostics::system_summary());
    for role in ["rustdo", "companion"] {
        let Some(path) = diagnostics::log_path(role) else { continue };
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let lines: Vec<&str> = text.lines().collect();
        let tail = &lines[lines.len().saturating_sub(60)..];
        out.push_str(&format!("\n--- {} (last {} lines) ---\n{}\n", path.display(), tail.len(), tail.join("\n")));
    }
    out
}

/// A task's subtasks under its row in the list: tick them off, or add more.
fn subtask_rows(ui: &mut egui::Ui, t: &Task, input: &mut String, actions: &mut Vec<Action>) {
    let vis = ui.visuals().clone();
    let weak = vis.weak_text_color();
    let touch = is_touch(ui);
    let (indent, line_h, radius, size) = if touch { (56.0, 44.0, 9.0, 15.5) } else { (44.0, 28.0, 7.0, 13.5) };
    for step in &t.steps {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), line_h), Sense::click());
        let center = Pos2::new(rect.left() + indent, rect.center().y);
        let hovered = resp.hovered();
        paint_check(ui.painter(), center, radius, step.done, hovered, &vis);
        let mut job = LayoutJob::single_section(
            step.title.clone(),
            TextFormat {
                font_id: FontId::proportional(size),
                color: if step.done { weak } else { vis.text_color() },
                strikethrough: if step.done { Stroke::new(1.0_f32, weak) } else { Stroke::NONE },
                ..Default::default()
            },
        );
        let left = center.x + radius + 10.0;
        job.wrap = TextWrapping::truncate_at_width((rect.right() - 12.0 - left).max(10.0));
        let galley = ui.fonts(|f| f.layout_job(job));
        ui.painter().galley(Pos2::new(left, rect.center().y - galley.size().y / 2.0), galley, vis.text_color());
        if resp
            .on_hover_text(if step.done { tr("Mark as not done") } else { tr("Mark as done") })
            .on_hover_cursor(CursorIcon::PointingHand)
            .clicked()
        {
            actions.push(Action::ToggleStep(t.id, step.id));
        }
    }
    // "Add a subtask" under them, laid out from the right like the details panel's.
    let field_id = Id::new(("list_subtask", t.id));
    let focus_flag = Id::new(("focus_subtask", t.id));
    if ui.ctx().data_mut(|d| d.remove_temp::<bool>(focus_flag)).unwrap_or(false) {
        ui.memory_mut(|m| m.request_focus(field_id));
    }
    ui.horizontal(|ui| {
        ui.add_space(indent - radius - 4.0);
        let has_text = !input.trim().is_empty();
        let mut add = false;
        let mut enter = false;
        let mut lost = false;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // ✖ folds the box (and the subtasks) away.
            let tip = if t.steps.is_empty() { tr("Done adding subtasks") } else { tr("Hide subtasks") };
            if small_btn(ui, "✖").on_hover_text(tip).clicked() {
                actions.push(Action::ToggleExpand(t.id));
            }
            add = has_text && ui.small_button(tr("Add")).clicked();
            let r = ui.add(
                egui::TextEdit::singleline(input)
                    .id(field_id)
                    .hint_text(tr("+ Add a subtask (Enter after each)"))
                    .min_size(field_height(ui))
                    .vertical_align(Align::Center)
                    .desired_width(ui.available_width()),
            );
            enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            lost = r.lost_focus();
            keep_above_keyboard(&r);
        });
        if (add || enter) && !input.trim().is_empty() {
            actions.push(Action::AddStep(t.id, std::mem::take(input)));
            ui.memory_mut(|m| m.request_focus(field_id));
        } else if lost && t.steps.is_empty() && input.trim().is_empty() {
            // Opened from "Add a subtask" in the menu but left empty: fold it away again.
            actions.push(Action::ToggleExpand(t.id));
        }
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rounds to the nearest 16-bit float (11 significant bits), as a `mediump` GPU does.
    fn to_f16(x: f32) -> f32 {
        if x == 0.0 {
            return 0.0;
        }
        let step = 2f32.powi(x.abs().log2().floor() as i32 - 10);
        (x / step).round() * step
    }

    /// Worst error, in texels, of a texture coordinate across an atlas of this width
    /// (sampled at every glyph-edge-like position, including ones between texels).
    fn worst_texel_error(width: usize) -> f32 {
        (0..width * 4)
            .map(|i| {
                let u = i as f32 / 4.0 / width as f32;
                (to_f16(u) - u).abs() * width as f32
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn atlas_is_capped_for_16_bit_texture_coordinates() {
        assert_eq!(capped_texture_side(Some(16384)), Some(MAX_ATLAS_SIDE));
        assert_eq!(capped_texture_side(Some(1024)), Some(1024));
        assert_eq!(capped_texture_side(None), Some(MAX_ATLAS_SIDE));
        // The bug: on a 16384 px atlas letters were sampled several texels off.
        assert!(worst_texel_error(16384) >= 4.0);
        // The fix: within half a texel, i.e. inside the 1 px padding between glyphs.
        assert!(worst_texel_error(MAX_ATLAS_SIDE) <= 0.5);
    }
}
