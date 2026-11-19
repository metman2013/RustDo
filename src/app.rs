//! The egui user interface.

use crate::model::{
    relative_date, sort_tasks, PlannedBucket, Recurrence, SortMode, Step, Store, Task,
    ThemeChoice, View,
};
use crate::import::{self, Import};
use crate::platform;
use crate::storage::Storage;
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
        let storage = Storage::for_platform();
        let loaded = storage.load();
        apply_theme(&cc.egui_ctx, loaded.store.settings.theme);
        cc.egui_ctx.all_styles_mut(|s| {
            s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
            s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = Vec2::new(8.0, 4.0);
        });
        Self {
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

    /// Starts the daily background check (desktop and Android; the web app updates itself).
    fn maybe_check_updates(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        if cfg!(target_arch = "wasm32") || self.update_rx.is_some() || self.update_available.is_some() {
            return;
        }
        let due = self
            .store
            .settings
            .last_update_check
            .is_none_or(|last| now - last >= Duration::hours(24) || last > now);
        if self.update_manual || (self.store.settings.check_updates && due) {
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
                        self.update_available = Some(release);
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
                    Ok(installer) => match std::process::Command::new(&installer).spawn() {
                        Ok(_) => {
                            // The installer replaces this program, so get out of its way.
                            self.persist();
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(e) => self.notify(format!("Couldn't start the installer: {e}")),
                    },
                    Err(e) => self.notify(format!("Couldn't download the update: {e}")),
                }
            }
        }
    }

    fn install_update(&mut self, ctx: &egui::Context) {
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
                    let busy = self.install_rx.is_some();
                    let label = if busy { "Downloading…" } else { "Update now" };
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
            ui.label(RichText::new(format!("RustDo {}", update::CURRENT_VERSION)).strong());
            ui.separator();
            if cfg!(target_arch = "wasm32") {
                ui.label(RichText::new("The web app updates itself automatically.").weak());
            } else {
                if ui.checkbox(&mut self.store.settings.check_updates, "Check for updates daily").changed() {
                    self.dirty = true;
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
            ui.hyperlink_to("Project page", format!("https://github.com/{}", update::REPO));
        });
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
        if let Err(e) = self.storage.save(&self.store) {
            self.notify(format!("Couldn't save: {e}"));
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
                ui.horizontal(|ui| {
                    if narrow && ui.add(egui::Button::new(RichText::new("<").size(18.0)).frame(false)).clicked() {
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

                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("🔍 Search  (Ctrl+F)")
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
                let reserved = if CAN_IMPORT { 86.0 } else { 52.0 };
                let max_h = (ui.available_height() - reserved).max(60.0);
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
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.new_list)
                        .hint_text("+ New list")
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
            egui::CentralPanel::default().show(ctx, body);
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
            ui.horizontal(|ui| {
                if narrow
                    && ui
                        .add(egui::Button::new(RichText::new("☰").size(20.0)).frame(false))
                        .on_hover_text("Lists")
                        .clicked()
                {
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
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.new_task)
                        .hint_text(if platform::IS_MOBILE_OR_WEB || narrow {
                            "+  Add a task"
                        } else {
                            "+  Add a task   (Enter to save, Ctrl+N)"
                        })
                        .font(FontId::proportional(15.0))
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

            let store = &self.store;
            let view = self.view;
            let selected = self.selected;
            let sort = store.settings.sort;
            let show_list = searching || !matches!(view, View::List(_));
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
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
        });
        for a in actions {
            self.apply(a, now);
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
                        if ui.add(egui::Button::new(RichText::new("<  Back").size(16.0)).frame(false)).clicked() {
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
                                    if ui.small_button("✖").on_hover_text("Delete step").clicked() {
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
                        let (text, color) = if in_day {
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
                                    if ui.small_button("✖").on_hover_text("Remove reminder").clicked() {
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
                                    if ui.small_button("✖").on_hover_text("Remove due date").clicked() {
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
                            if ui.small_button(label).clicked() {
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
                        if ui.small_button("✖").clicked() {
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
        self.toast_area(ctx);

        if self.dirty {
            self.persist();
        }
    }
}

// ---------------------------------------------------------------------- widgets

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
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let vis = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 6.0, vis.selection.bg_fill.gamma_multiply(0.45));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, vis.widgets.hovered.weak_bg_fill);
    }
    let color = if selected { vis.strong_text_color() } else { vis.text_color() };
    let icon_color = if selected { ACCENT } else { vis.weak_text_color() };
    let p = ui.painter();
    p.text(rect.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(14.0), icon_color);
    p.text(rect.left_center() + Vec2::new(34.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(14.0), color);
    if count > 0 {
        p.text(rect.right_center() - Vec2::new(10.0, 0.0), Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(12.0), vis.weak_text_color());
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
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    paint_check(ui.painter(), rect.center(), size * 0.42, done, resp.hovered(), ui.visuals());
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
    ui.painter().text(center, Align2::CENTER_CENTER, glyph, FontId::proportional(18.0), color);
}

fn star_button(ui: &mut egui::Ui, important: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    paint_star(ui, rect.center(), important, resp.hovered());
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if important { "Remove importance" } else { "Mark as important" })
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

    let height = if meta.is_empty() { 42.0 } else { 52.0 };
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
    let check_rect = Rect::from_center_size(Pos2::new(rect.left() + 22.0, rect.center().y), Vec2::splat(24.0));
    let check = ui.interact(check_rect, Id::new(("check", t.id)), Sense::click());
    let star_rect = Rect::from_center_size(Pos2::new(rect.right() - 22.0, rect.center().y), Vec2::splat(26.0));
    let star = ui.interact(star_rect, Id::new(("star", t.id)), Sense::click());

    paint_check(ui.painter(), check_rect.center(), 9.5, t.done, check.hovered(), &vis);
    paint_star(ui, star_rect.center(), t.important, star.hovered());

    let text_left = rect.left() + 44.0;
    let text_width = (star_rect.left() - 8.0 - text_left).max(10.0);

    let mut title_job = LayoutJob::single_section(
        t.title.clone(),
        TextFormat {
            font_id: FontId::proportional(14.5),
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
                job.append("  ·  ", 0.0, TextFormat { font_id: FontId::proportional(11.5), color: weak, ..Default::default() });
            }
            job.append(&text, 0.0, TextFormat { font_id: FontId::proportional(11.5), color, ..Default::default() });
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
