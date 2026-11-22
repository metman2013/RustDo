//! The sticky note windows (Windows and Linux).
//!
//! Every open note is a separate borderless OS window ("deferred viewport" in egui terms):
//! it draws and saves itself, so notes keep working while the main RustDo window is
//! minimized. The notes live in one shared, locked [`Shared`] that the main window's
//! "Sticky notes" page and the note windows all read and write.

use crate::notes::{self, Notes, StickyNote, MAX_TEXT_SIZE, MIN_TEXT_SIZE, PRESETS};
use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontId, Id, Layout, Pos2, RichText, Sense, Stroke, UiBuilder, Vec2,
    ViewportBuilder, ViewportCommand, ViewportId,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use web_time::Instant;
use uuid::Uuid;

const HEADER_H: f32 = 30.0;
const SAVE_DELAY: Duration = Duration::from_millis(600);

pub struct Shared {
    pub notes: Notes,
    /// `notes.json` (desktop only: the web and Android have no sticky notes).
    #[cfg_attr(any(target_os = "android", target_arch = "wasm32"), allow(dead_code))]
    path: Option<PathBuf>,
    dirty: bool,
    last_change: Instant,
    /// Shown on the Sticky notes page (e.g. the file couldn't be read or saved).
    pub warning: Option<String>,
    /// The note asking "Delete this note?".
    confirm_delete: Option<Uuid>,
    /// Where each note window opened this session. egui only moves a window when the
    /// position it's given changes, so we keep handing it the same one and let the user
    /// drag freely; the live position is saved separately.
    spawn: HashMap<Uuid, (Option<Pos2>, Vec2)>,
    /// A just-created note whose text box should get the keyboard.
    focus: Option<Uuid>,
    /// Notes changed since the last automatic backup.
    backup_pending: bool,
}

pub type SharedNotes = Arc<Mutex<Shared>>;

pub fn lock(shared: &SharedNotes) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    /// Loads `notes.json` from `folder` (desktop), or starts empty in memory.
    pub fn load(folder: Option<PathBuf>) -> SharedNotes {
        let path = folder.map(|f| f.join("notes.json"));
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        let (notes, warning) = path.as_deref().map_or((Notes::default(), None), notes::file::load);
        #[cfg(any(target_os = "android", target_arch = "wasm32"))]
        let (notes, warning) = (Notes::default(), None);
        Arc::new(Mutex::new(Shared {
            notes,
            path,
            dirty: false,
            last_change: Instant::now(),
            warning,
            confirm_delete: None,
            spawn: HashMap::new(),
            focus: None,
            backup_pending: false,
        }))
    }

    pub fn changed(&mut self) {
        self.dirty = true;
        self.backup_pending = true;
        self.last_change = Instant::now();
    }

    /// Whether notes changed since this was last asked (for the automatic backup).
    pub fn take_backup_pending(&mut self) -> bool {
        std::mem::take(&mut self.backup_pending)
    }

    /// A full restore: the backup's notes replace these. A copy of the current notes is
    /// saved in `safety_dir` first (`RustDo-notes-before-restore-<time>.json`).
    pub fn replace_all(&mut self, notes: Notes, safety_dir: Option<&std::path::Path>) -> Result<(), String> {
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        if let Some(dir) = safety_dir.filter(|_| !self.notes.notes.is_empty()) {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let copy = dir.join(format!("{}before-restore-{stamp}.json", notes::BACKUP_PREFIX));
            notes::file::save(&copy, &self.notes).map_err(|e| format!("couldn't save your current sticky notes first ({e})"))?;
        }
        let _ = safety_dir;
        self.notes = notes;
        self.notes.repair();
        self.spawn.clear();
        self.confirm_delete = None;
        self.changed();
        self.save_if_due(true);
        Ok(())
    }

    /// A notes-only restore: adds the backup's notes without losing any here.
    pub fn merge_in(&mut self, notes: &Notes) -> (usize, usize) {
        let counts = self.notes.merge(notes);
        self.changed();
        self.save_if_due(true);
        counts
    }

    /// Saves if something changed a moment ago (typing doesn't write on every key).
    /// `force` saves right away (e.g. when RustDo closes).
    pub fn save_if_due(&mut self, force: bool) {
        if !self.dirty || (!force && self.last_change.elapsed() < SAVE_DELAY) {
            return;
        }
        self.dirty = false;
        #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
        if let Some(path) = &self.path {
            match notes::file::save(path, &self.notes) {
                Ok(()) => {
                    if self.warning.as_deref().is_some_and(|w| w.starts_with("Couldn't save")) {
                        self.warning = None;
                    }
                }
                Err(e) => self.warning = Some(format!("Couldn't save your sticky notes: {e}")),
            }
        }
    }

    pub fn new_note(&mut self, near: Option<[f32; 2]>) -> Uuid {
        let id = self.notes.add(now(), near);
        self.focus = Some(id);
        self.changed();
        id
    }

    pub fn set_open(&mut self, id: Uuid, open: bool) {
        if let Some(n) = self.notes.get_mut(id) {
            if n.open != open {
                n.open = open;
                self.spawn.remove(&id);
                self.changed();
            }
        }
    }

    pub fn delete(&mut self, id: Uuid) {
        self.notes.remove(id);
        self.spawn.remove(&id);
        self.confirm_delete = None;
        self.changed();
    }
}

fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

fn c32([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn viewport_id(id: Uuid) -> ViewportId {
    ViewportId::from_hash_of(("sticky_note", id))
}

/// Called every frame by the main window: keeps one window per open note.
pub fn show_windows(ctx: &egui::Context, shared: &SharedNotes) {
    if !notes::SUPPORTED {
        return;
    }
    // Build the window descriptions under the lock, then release it: the windows' own
    // drawing code takes the lock too.
    let windows: Vec<(Uuid, ViewportBuilder)> = {
        let mut s = lock(shared);
        s.save_if_due(false);
        let Shared { notes, spawn, .. } = &mut *s;
        notes
            .notes
            .iter()
            .filter(|n| n.open)
            .map(|n| {
                let (pos, size) =
                    *spawn.entry(n.id).or_insert_with(|| (n.pos.map(Pos2::from), Vec2::from(n.size)));
                let mut b = ViewportBuilder::default()
                    .with_title(format!("{} - Sticky note", n.title()))
                    .with_app_id("rustdo")
                    .with_inner_size(size)
                    .with_min_inner_size([160.0, 120.0])
                    .with_decorations(false)
                    .with_taskbar(false)
                    .with_window_level(if n.on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    });
                if let Some(pos) = pos {
                    b = b.with_position(pos);
                }
                (n.id, b)
            })
            .collect()
    };
    for (id, builder) in windows {
        let shared = shared.clone();
        ctx.show_viewport_deferred(viewport_id(id), builder, move |ctx, _class| note_window(ctx, &shared, id));
    }
}

/// One note's window.
fn note_window(ctx: &egui::Context, shared: &SharedNotes, id: Uuid) {
    let mut guard = lock(shared);
    let s = &mut *guard;
    let Some(index) = s.notes.notes.iter().position(|n| n.id == id) else {
        ctx.send_viewport_cmd(ViewportCommand::Close);
        return;
    };

    // Alt+F4 or the system's close: hide the note (it stays in the list).
    if ctx.input(|i| i.viewport().close_requested()) {
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        s.set_open(id, false);
        s.save_if_due(true);
        ctx.request_repaint_of(ViewportId::ROOT);
        return;
    }

    // Remember where the note is and how big, to reopen it there next time.
    let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
    {
        let note = &mut s.notes.notes[index];
        let mut moved = false;
        if let Some(r) = outer {
            let pos = [r.min.x, r.min.y];
            if note.pos.is_none_or(|p| (p[0] - pos[0]).abs() > 0.5 || (p[1] - pos[1]).abs() > 0.5) {
                note.pos = Some(pos);
                moved = true;
            }
        }
        if let Some(r) = inner {
            let size = [r.width(), r.height()];
            if (note.size[0] - size[0]).abs() > 0.5 || (note.size[1] - size[1]).abs() > 0.5 {
                note.size = size;
                moved = true;
            }
        }
        if moved {
            s.changed();
        }
    }

    let note = s.notes.notes[index].clone();
    let bg = c32(note.color);
    let ink = c32(note.ink());
    let header = c32(notes::header_color(note.color));
    let mut action = NoteAction::None;
    let mut edited = note.clone();
    let confirming = s.confirm_delete == Some(id);
    let focus_text = s.focus == Some(id);

    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg)).show(ctx, |ui| {
        // Readable widgets on any note colour: cursor, selection and buttons follow the ink.
        let v = ui.visuals_mut();
        v.override_text_color = Some(ink);
        v.text_cursor.stroke = Stroke::new(2.0_f32, ink);
        v.selection.bg_fill = ink.gamma_multiply(0.25);
        v.selection.stroke = Stroke::new(1.0_f32, ink);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.fg_stroke.color = ink;
        }
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.hovered.weak_bg_fill = ink.gamma_multiply(0.12);
        v.widgets.active.weak_bg_fill = ink.gamma_multiply(0.2);

        let full = ui.max_rect();

        // Title bar: drag it to move the note.
        let bar = egui::Rect::from_min_size(full.min, Vec2::new(full.width(), HEADER_H));
        let drag = ui.interact(bar, Id::new(("note_bar", id)), Sense::click_and_drag());
        ui.painter().rect_filled(bar, CornerRadius::ZERO, header);
        if drag.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        let mut bar_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(bar.shrink2(Vec2::new(4.0, 2.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        if bar_ui.add(egui::Button::new(RichText::new("+").size(18.0).color(ink)).frame(false)).on_hover_text("New note").clicked() {
            action = NoteAction::New;
        }
        bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(egui::Button::new(RichText::new("✖").color(ink)).frame(false)).on_hover_text("Hide note (it stays in RustDo's Sticky notes list)").clicked() {
                action = NoteAction::Hide;
            }
            ui.menu_button(RichText::new("🎨").color(ink), |ui| {
                note_menu(ui, &mut edited, &mut action);
            })
            .response
            .on_hover_text("Colour, text and more");
            if edited.on_top {
                ui.label(RichText::new("📌").color(ink)).on_hover_text("Kept on top of other windows");
            }
        });

        // The text.
        let body = egui::Rect::from_min_max(full.min + Vec2::new(0.0, HEADER_H), full.max);
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body.shrink(10.0)).layout(Layout::top_down(Align::Min)));
        if confirming {
            body_ui.label(RichText::new("Delete this note?").strong().size(16.0));
            body_ui.label(RichText::new("This can't be undone.").small());
            body_ui.horizontal(|ui| {
                if ui.button(RichText::new("Delete").color(ink)).clicked() {
                    action = NoteAction::Delete;
                }
                if ui.button(RichText::new("Cancel").color(ink)).clicked() {
                    action = NoteAction::CancelDelete;
                }
            });
        } else {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(&mut body_ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::multiline(&mut edited.text)
                        .font(FontId::proportional(edited.text_size))
                        .text_color(ink)
                        .hint_text(RichText::new("Take a note…").color(ink.gamma_multiply(0.5)))
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .desired_rows(((body.height() - 20.0) / (edited.text_size * 1.3)).max(1.0) as usize),
                );
                if focus_text {
                    r.request_focus();
                }
            });
        }

        // Resize grip in the bottom-right corner.
        let grip = egui::Rect::from_min_max(full.max - Vec2::splat(16.0), full.max);
        let g = ui.interact(grip, Id::new(("note_grip", id)), Sense::drag()).on_hover_cursor(CursorIcon::ResizeNwSe);
        for i in 1..=3 {
            let d = i as f32 * 4.0;
            ui.painter().line_segment(
                [grip.right_bottom() - Vec2::new(d, 2.0), grip.right_bottom() - Vec2::new(2.0, d)],
                Stroke::new(1.0_f32, ink.gamma_multiply(0.5)),
            );
        }
        if g.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(egui::ResizeDirection::SouthEast));
        }
    });

    if focus_text {
        s.focus = None;
    }
    if edited != note {
        if edited.text != note.text {
            edited.updated = now();
        }
        edited.text_size = edited.text_size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
        s.notes.notes[index] = edited;
        s.changed();
        // The main window's Sticky notes page shows titles and colours: keep it current.
        ctx.request_repaint_of(ViewportId::ROOT);
    }
    match action {
        NoteAction::None => {}
        NoteAction::New => {
            let near = s.notes.notes[index].pos;
            s.new_note(near);
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        NoteAction::Hide => {
            s.set_open(id, false);
            s.save_if_due(true);
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        NoteAction::AskDelete => s.confirm_delete = Some(id),
        NoteAction::CancelDelete => s.confirm_delete = None,
        NoteAction::Delete => {
            s.delete(id);
            s.save_if_due(true);
            ctx.request_repaint_of(ViewportId::ROOT);
        }
    }
    s.save_if_due(false);
    if s.dirty {
        // Come back to save once typing pauses, even if nothing else happens.
        ctx.request_repaint_after(SAVE_DELAY);
    }
}

enum NoteAction {
    None,
    New,
    Hide,
    AskDelete,
    CancelDelete,
    Delete,
}

/// The 🎨 menu: note colour, text colour and size, keep on top, delete.
fn note_menu(ui: &mut egui::Ui, note: &mut StickyNote, action: &mut NoteAction) {
    ui.set_min_width(220.0);
    // The menu has its own (theme) background: use normal text colours inside it.
    ui.visuals_mut().override_text_color = None;
    ui.label(RichText::new("Note colour").strong());
    ui.horizontal_wrapped(|ui| {
        for (name, color) in PRESETS {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
            let selected = note.color == color;
            ui.painter().rect_filled(rect, CornerRadius::same(4), c32(color));
            let outline = if selected || r.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
            ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(if selected { 2.0_f32 } else { 1.0 }, outline), egui::StrokeKind::Inside);
            if r.on_hover_text(name).clicked() {
                note.color = color;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Any colour:");
        egui::color_picker::color_edit_button_srgb(ui, &mut note.color);
    });
    ui.separator();
    ui.label(RichText::new("Text").strong());
    let mut auto = note.text_color.is_none();
    if ui.checkbox(&mut auto, "Automatic colour (suits the note)").changed() {
        note.text_color = if auto { None } else { Some(note.ink()) };
    }
    if let Some(color) = note.text_color.as_mut() {
        ui.horizontal(|ui| {
            ui.label("Text colour:");
            egui::color_picker::color_edit_button_srgb(ui, color);
        });
    }
    ui.horizontal(|ui| {
        ui.label("Size:");
        if ui.small_button("A−").on_hover_text("Smaller").clicked() {
            note.text_size = (note.text_size - 2.0).max(MIN_TEXT_SIZE);
        }
        ui.add(egui::Slider::new(&mut note.text_size, MIN_TEXT_SIZE..=MAX_TEXT_SIZE).step_by(1.0).show_value(true));
        if ui.small_button("A+").on_hover_text("Bigger").clicked() {
            note.text_size = (note.text_size + 2.0).min(MAX_TEXT_SIZE);
        }
    });
    ui.separator();
    ui.checkbox(&mut note.on_top, "📌 Keep on top of other windows");
    if ui.button("🗑  Delete note…").clicked() {
        *action = NoteAction::AskDelete;
        ui.close_menu();
    }
}

/// What the Sticky notes page asks the main window to do (file dialogs live there).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageAction {
    None,
    Backup,
    Restore,
}

/// The main window's "Sticky notes" page: every note, to show, hide, create or delete.
pub fn notes_page(ui: &mut egui::Ui, shared: &SharedNotes) -> PageAction {
    let mut page_action = PageAction::None;
    let mut s = lock(shared);
    ui.horizontal(|ui| {
        ui.label(RichText::new("📝 Sticky notes").size(24.0).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Restore notes…").on_hover_text("Bring notes back from a backup file (your current notes are kept)").clicked() {
                page_action = PageAction::Restore;
            }
            if ui.button("Back up notes…").on_hover_text("Save all sticky notes to a file").clicked() {
                page_action = PageAction::Backup;
            }
            if ui.button("+ New note").clicked() {
                // Next to the RustDo window (cascading from the newest note if there is one).
                let near = s.notes.notes.last().filter(|n| n.open).and_then(|n| n.pos).or_else(|| {
                    ui.ctx().input(|i| i.viewport().outer_rect).map(|r| [r.right() - 300.0, r.top() + 40.0])
                });
                s.new_note(near);
            }
        });
    });
    ui.label(
        RichText::new(
            "Notes sit on your desktop as their own little windows while RustDo is running. \
             They stay on this computer: never synced or shared. They're included in RustDo's \
             daily backups, and “Back up notes…” saves them to a file of your own.",
        )
        .weak(),
    );
    if let Some(w) = &s.warning {
        ui.colored_label(Color32::from_rgb(0xE5, 0x48, 0x4D), w);
    }
    ui.add_space(8.0);
    if s.notes.notes.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("📝").size(40.0));
            ui.label(RichText::new("No sticky notes yet. Click “+ New note” to put one on your desktop.").weak());
        });
        return page_action;
    }
    let mut toggle = None;
    let mut delete = None;
    let mut ask_delete = None;
    let mut cancel_delete = false;
    let confirming = s.confirm_delete;
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        // Newest first.
        for note in s.notes.notes.iter().rev() {
            let bg = c32(note.color);
            let ink = c32(note.ink());
            egui::Frame::new()
                .fill(bg)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(note.title()).color(ink).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.visuals_mut().override_text_color = Some(ink);
                            ui.visuals_mut().widgets.inactive.weak_bg_fill = ink.gamma_multiply(0.12);
                            if confirming == Some(note.id) {
                                if ui.button("Cancel").clicked() {
                                    cancel_delete = true;
                                }
                                if ui.button("Delete").clicked() {
                                    delete = Some(note.id);
                                }
                                ui.label(RichText::new("Delete this note?").color(ink));
                                return;
                            }
                            if ui.button("🗑").on_hover_text("Delete note").clicked() {
                                ask_delete = Some(note.id);
                            }
                            let label = if note.open { "Hide" } else { "Show" };
                            if ui.button(label).clicked() {
                                toggle = Some((note.id, !note.open));
                            }
                        });
                    });
                    let preview: String = note.text.lines().skip_while(|l| l.trim().is_empty()).skip(1).take(2).collect::<Vec<_>>().join("\n");
                    if !preview.trim().is_empty() {
                        ui.label(RichText::new(preview).color(ink.gamma_multiply(0.8)).small());
                    }
                });
            ui.add_space(6.0);
        }
    });
    if let Some((id, open)) = toggle {
        s.set_open(id, open);
    }
    if let Some(id) = delete {
        s.delete(id);
    } else if let Some(id) = ask_delete {
        s.confirm_delete = Some(id);
    } else if cancel_delete {
        s.confirm_delete = None;
    }
    s.save_if_due(false);
    page_action
}
