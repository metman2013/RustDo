//! The sticky note windows (Windows and Linux).
//!
//! Every open note is a separate borderless OS window ("deferred viewport" in egui terms):
//! it draws and saves itself, so notes keep working while the main RustDo window is
//! minimized. The notes live in one shared, locked [`Shared`] that the main window's
//! "Sticky notes" page and the note windows all read and write.

use crate::backgrounds::{self, Background};
use crate::i18n::{tr, trf};
use crate::notes::{self, Notes, StickyNote, MAX_TEXT_SIZE, MIN_TEXT_SIZE, PRESETS};
use crate::pinned::{CardAction, CardList};
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
    /// `notes.json` next to the tasks (desktop and Android; the web app uses browser storage).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
    /// When `notes.json` was last written or read by this process: another RustDo process
    /// (the main window or the companion) saving it changes the file's time.
    file_stamp: Option<std::time::SystemTime>,
    /// Pinned lists' tasks, filled in by whoever shows the cards (see pinned.rs).
    pub cards: HashMap<Uuid, CardList>,
    /// What was done on the cards, for the owner to carry out.
    pub card_actions: Vec<CardAction>,
}

pub type SharedNotes = Arc<Mutex<Shared>>;

/// Whether a warning is the "couldn't save" one (in whichever language it was made).
fn is_save_warning(w: &str) -> bool {
    let prefix = tr("Couldn't save your sticky notes: {e}").split('{').next().unwrap_or_default();
    w.starts_with(prefix) || w.starts_with("Couldn't save")
}

trait WithStamp {
    fn with_stamp(self) -> Self;
}

impl WithStamp for SharedNotes {
    /// Remembers the file time of what was just loaded.
    fn with_stamp(self) -> Self {
        {
            let mut s = lock(&self);
            s.file_stamp = s.stamp();
        }
        self
    }
}

pub fn lock(shared: &SharedNotes) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    /// Loads `notes.json` from `folder` (desktop), or starts empty in memory.
    pub fn load(folder: Option<PathBuf>) -> SharedNotes {
        *PICTURE_DIR.lock().unwrap_or_else(|e| e.into_inner()) = folder.clone();
        let path = folder.map(|f| f.join("notes.json"));
        #[cfg(not(target_arch = "wasm32"))]
        let (notes, warning) = path.as_deref().map_or((Notes::default(), None), notes::file::load);
        #[cfg(target_arch = "wasm32")]
        let (notes, warning) = notes::browser::load();
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
            file_stamp: None,
            cards: HashMap::new(),
            card_actions: Vec::new(),
        }))
        .with_stamp()
    }

    fn stamp(&self) -> Option<std::time::SystemTime> {
        let path = self.path.as_ref()?;
        std::fs::metadata(path).and_then(|m| m.modified()).ok()
    }

    /// Picks up changes another RustDo process saved (the main window and the background
    /// companion share `notes.json`). Our own unsaved changes win: they're saved shortly.
    /// Returns whether anything was reloaded.
    pub fn reload_if_changed(&mut self) -> bool {
        if self.dirty {
            return false;
        }
        let stamp = self.stamp();
        if stamp.is_none() || stamp == self.file_stamp {
            return false;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.path {
            let (fresh, warning) = notes::file::load(path);
            if warning.is_none() {
                // Keep window positions for notes that are still showing.
                self.spawn.retain(|id, _| fresh.notes.iter().any(|n| n.id == *id && n.open));
                self.notes = fresh;
            }
        }
        self.file_stamp = stamp;
        true
    }

    /// Shows or hides every note (the tray menu).
    pub fn set_all_open(&mut self, open: bool) {
        // Archived notes stay put away.
        let ids: Vec<Uuid> = self.notes.notes.iter().filter(|n| !n.archived).map(|n| n.id).collect();
        for id in ids {
            self.set_open(id, open);
        }
        self.save_if_due(true);
    }

    /// When the notes last changed in this process (for "Sync my devices").
    pub fn last_change(&self) -> Instant {
        self.last_change
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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = safety_dir.filter(|_| !self.notes.notes.is_empty()) {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let copy = dir.join(format!("{}before-restore-{stamp}.json", notes::BACKUP_PREFIX));
            notes::file::save(&copy, &self.notes).map_err(|e| trf("couldn't save your current sticky notes first ({e})", &[("e", &e)]))?;
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
    /// Adds imported notes (skipping ones already here); returns how many were added.
    pub fn import(&mut self, notes: Vec<StickyNote>) -> usize {
        let added = self.notes.import(notes);
        if added > 0 {
            self.changed();
            self.save_if_due(true);
        }
        added
    }

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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.path {
            match notes::file::save(path, &self.notes) {
                Ok(()) => {
                    self.file_stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
                    if self.warning.as_deref().is_some_and(is_save_warning) {
                        self.warning = None;
                    }
                }
                Err(e) => self.warning = Some(trf("Couldn't save your sticky notes: {e}", &[("e", &e)])),
            }
        }
        #[cfg(target_arch = "wasm32")]
        match notes::browser::save(&self.notes) {
            Ok(()) => {
                if self.warning.as_deref().is_some_and(is_save_warning) {
                    self.warning = None;
                }
            }
            Err(e) => self.warning = Some(trf("Couldn't save your sticky notes: {e}", &[("e", &e)])),
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

    /// Puts a note away in the Archive (hidden from the desktop and the wall), or back.
    pub fn archive(&mut self, id: Uuid, archived: bool) {
        if let Some(n) = self.notes.get_mut(id) {
            n.archived = archived;
            if archived {
                n.open = false;
                n.pinned = false;
            } else {
                n.open = true;
            }
            self.changed();
        }
    }

    pub fn delete(&mut self, id: Uuid) {
        self.notes.remove(id);
        self.spawn.remove(&id);
        self.confirm_delete = None;
        self.changed();
    }
}

/// The owner of the cards (main window or companion) hands over the pinned lists' tasks;
/// card windows that show something new are redrawn.
pub fn set_cards(ctx: &egui::Context, shared: &SharedNotes, cards: HashMap<Uuid, CardList>) {
    let mut s = lock(shared);
    if s.cards == cards {
        return;
    }
    s.cards = cards;
    for n in s.notes.notes.iter().filter(|n| n.list.is_some()) {
        ctx.request_repaint_of(viewport_id(n.id));
    }
}

/// Pins a list: a new card (on the desktop as its own window, on the wall elsewhere), or
/// shows the card it already has. Returns whether it was newly pinned.
pub fn pin_list(shared: &SharedNotes, list: Uuid, name: &str) -> bool {
    let mut s = lock(shared);
    let existing = s.notes.card_for(list).map(|n| n.id);
    let added = match existing {
        Some(id) => {
            if let Some(n) = s.notes.get_mut(id) {
                n.archived = false;
            }
            s.set_open(id, true);
            false
        }
        None => {
            let near = s.notes.notes.iter().rev().find(|n| n.open).and_then(|n| n.pos);
            let mut card = crate::pinned::new_card(list, name, now());
            card.pos = near.map(|[x, y]| [x + 32.0, y + 32.0]);
            s.notes.notes.push(card);
            true
        }
    };
    s.changed();
    s.save_if_due(true);
    added
}

/// Unpins a list (removes its card).
pub fn unpin_list(shared: &SharedNotes, list: Uuid) {
    let mut s = lock(shared);
    if let Some(id) = s.notes.card_for(list).map(|n| n.id) {
        s.delete(id);
        s.save_if_due(true);
    }
}

/// A pinned list's look: the app's own (light or dark) card with an accent bar, so it
/// isn't mistaken for a sticky note. (background, text, bar)
fn card_colours(dark: bool) -> (Color32, Color32, Color32) {
    if dark {
        (Color32::from_rgb(0x26, 0x28, 0x2C), Color32::from_rgb(0xE6, 0xE6, 0xE6), Color32::from_rgb(0x25, 0x63, 0xEB))
    } else {
        (Color32::WHITE, Color32::from_rgb(0x1F, 0x29, 0x37), Color32::from_rgb(0x3B, 0x82, 0xF6))
    }
}

/// A note's colours: (background, text, top bar, top bar's text).
fn colours(ui_dark: bool, note: &StickyNote) -> (Color32, Color32, Color32, Color32) {
    if note.list.is_some() {
        let (bg, ink, bar) = card_colours(ui_dark);
        (bg, ink, bar, Color32::WHITE)
    } else {
        let ink = c32(note.ink());
        (c32(note.color), ink, c32(notes::header_color(note.color)), ink)
    }
}

/// The card's title in its top bar: "📋 Groceries  3".
fn card_title(ui: &mut egui::Ui, card: Option<&CardList>, color: Color32, size: f32) {
    let text = match card {
        Some(c) if c.open > 0 => format!("{} {}  {}", c.icon, c.name, c.open),
        Some(c) => format!("{} {}", c.icon, c.name),
        None => tr("📋 Pinned list").to_string(),
    };
    ui.add(egui::Label::new(RichText::new(text).size(size).strong().color(color)).truncate().selectable(false));
}

/// A pinned list's ⚙ menu.
fn card_menu(ui: &mut egui::Ui, note: &mut StickyNote, action: &mut NoteAction, desktop: bool, sections: &[String]) {
    ui.set_min_width(220.0);
    ui.visuals_mut().override_text_color = None;
    ui.horizontal(|ui| {
        ui.label(tr("Size:"));
        if ui.small_button("A−").on_hover_text(tr("Smaller")).clicked() {
            note.text_size = (note.text_size - 2.0).max(MIN_TEXT_SIZE);
        }
        ui.add(egui::Slider::new(&mut note.text_size, MIN_TEXT_SIZE..=MAX_TEXT_SIZE).step_by(1.0).show_value(true));
        if ui.small_button("A+").on_hover_text(tr("Bigger")).clicked() {
            note.text_size = (note.text_size + 2.0).min(MAX_TEXT_SIZE);
        }
    });
    ui.checkbox(&mut note.show_done, tr("Show completed tasks"));
    ui.separator();
    if desktop {
        ui.checkbox(&mut note.on_top, tr("📌 Keep on top of other windows"));
    } else {
        ui.checkbox(&mut note.pinned, tr("📍  Pin to the top")).on_hover_text(tr("Keeps the note above the others on the wall"));
        section_picker(ui, note, sections);
    }
    ui.separator();
    if ui.button(tr("Unpin list")).on_hover_text(tr("Removes this card (the list and its tasks stay)")).clicked() {
        *action = NoteAction::Delete;
        ui.close_menu();
    }
}

/// A pinned list's tasks: tick, star, and "+ Add a task".
fn card_body(ui: &mut egui::Ui, note: &StickyNote, card: Option<&CardList>, actions: &mut Vec<CardAction>, ink: Color32) {
    let Some((list, card)) = note.list.zip(card) else {
        ui.label(RichText::new(tr("This list isn't on this device.")).color(ink));
        return;
    };
    let size = note.text_size;
    let add_id = Id::new(("card_add", note.id));
    let mut text: String = ui.ctx().data_mut(|d| d.get_temp(add_id)).unwrap_or_default();
    let r = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(Id::new(("card_add_edit", note.id)))
            .hint_text(RichText::new(tr("+ Add a task")).color(ink.gamma_multiply(0.55)))
            .font(FontId::proportional(size))
            .text_color(ink)
            .desired_width(f32::INFINITY),
    );
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
        actions.push(CardAction::Add(list, text.trim().to_string()));
        text.clear();
        r.request_focus();
    }
    ui.ctx().data_mut(|d| d.insert_temp(add_id, text));
    ui.add_space(4.0);
    let today = chrono::Local::now().date_naive();
    let star_color = Color32::from_rgb(0xF5, 0x9E, 0x0B);
    let mut shown = 0;
    for t in card.tasks.iter().filter(|t| !t.done || note.show_done) {
        shown += 1;
        ui.horizontal(|ui| {
            // A round tick, like the task list's.
            let d = (size + 4.0).max(18.0);
            let (r, tick) = ui.allocate_exact_size(Vec2::splat(d), Sense::click());
            let accent = Color32::from_rgb(0x3B, 0x82, 0xF6);
            let c = r.center();
            if t.done {
                ui.painter().circle_filled(c, d / 2.0 - 1.0, accent);
                ui.painter().text(c, egui::Align2::CENTER_CENTER, "✔", FontId::proportional(d * 0.6), Color32::WHITE);
            } else {
                let ring = if tick.hovered() { accent } else { ink.gamma_multiply(0.55) };
                ui.painter().circle_stroke(c, d / 2.0 - 1.5, Stroke::new(1.5_f32, ring));
            }
            if tick.on_hover_text(if t.done { tr("Mark as not done") } else { tr("Mark as done") }).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                actions.push(CardAction::SetDone(t.id, !t.done));
            }
            let star_w = size + 10.0;
            let width = (ui.available_width() - star_w).max(40.0);
            let mut title = RichText::new(&t.title).size(size).color(if t.done { ink.gamma_multiply(0.5) } else { ink });
            if t.done {
                title = title.strikethrough();
            }
            ui.allocate_ui(Vec2::new(width, 0.0), |ui| {
                ui.set_width(width);
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(title).wrap().selectable(false));
                    if let Some(due) = t.due.filter(|_| !t.done) {
                        let color = if t.overdue { Color32::from_rgb(0xDC, 0x26, 0x26) } else { ink.gamma_multiply(0.6) };
                        ui.label(RichText::new(crate::model::relative_date(due, today)).size((size - 3.0).max(10.0)).color(color));
                    }
                });
            });
            let (glyph, color) = if t.important { ("★", star_color) } else { ("☆", ink.gamma_multiply(0.5)) };
            let star = ui.add(egui::Button::new(RichText::new(glyph).size(size + 2.0).color(color)).frame(false));
            if star.on_hover_text(if t.important { tr("Remove importance") } else { tr("Mark as important") }).clicked() {
                actions.push(CardAction::SetImportant(t.id, !t.important));
            }
        });
    }
    if shown == 0 {
        ui.add_space(8.0);
        ui.label(RichText::new(if card.tasks.is_empty() { tr("No tasks yet.") } else { tr("All done!") }).color(ink.gamma_multiply(0.6)));
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
                    .with_title(format!("{} - {}", n.title(), tr("Sticky note")))
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

    // A small note grows while its 🎨 menu is open, so the menu and its colour picker fit,
    // and goes back to its own size when the menu closes. That size is what's remembered.
    let restore_id = Id::new(("note_restore_size", id));
    let restore: Option<(Vec2, Option<Pos2>)> = ctx.data(|d| d.get_temp(restore_id));
    let menu_open = ctx
        .data(|d| d.get_temp::<u64>(menu_seen_id(id)))
        .is_some_and(|pass| pass + 1 >= ctx.cumulative_pass_nr());
    match (menu_open, restore) {
        (true, None) => {
            if let Some(r) = inner {
                if r.width() < MENU_ROOM.x || r.height() < MENU_ROOM.y {
                    let grown = r.size().max(MENU_ROOM);
                    // Near the bottom or right of the screen, move up/left so it stays visible.
                    let monitor = ctx.input(|i| i.viewport().monitor_size);
                    let moved = outer.zip(monitor).and_then(|(o, m)| {
                        let extra = o.size() - r.size();
                        let to = Pos2::new(
                            o.min.x.min(m.x - grown.x - extra.x).max(0.0),
                            o.min.y.min(m.y - grown.y - extra.y - 48.0).max(0.0),
                        );
                        (to != o.min).then_some((o.min, to))
                    });
                    ctx.data_mut(|d| d.insert_temp(restore_id, (r.size(), moved.map(|(from, _)| from))));
                    if let Some((_, to)) = moved {
                        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(to));
                    }
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(grown));
                }
            }
        }
        (false, Some((size, pos))) => {
            ctx.data_mut(|d| d.remove::<(Vec2, Option<Pos2>)>(restore_id));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            if let Some(pos) = pos {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
            }
        }
        _ => {}
    }
    // While grown (or moved) for the menu, don't remember that size or place.
    let (outer, inner) = if restore.is_some() { (None, None) } else { (outer, inner) };
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
    let sections = s.notes.sections();
    let (bg, ink, header, bar_ink) = colours(ctx.style().visuals.dark_mode, &note);
    // A pinned list: its tasks, and what's done to them.
    let is_card = note.list.is_some();
    let card = note.list.and_then(|l| s.cards.get(&l).cloned());
    let mut card_actions = Vec::new();
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
        let paper = egui::Rect::from_min_max(full.min + Vec2::new(0.0, HEADER_H), full.max);
        paint_picture(ui, &note, paper, CornerRadius::ZERO);

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
        if !is_card && bar_ui.add(egui::Button::new(RichText::new("+").size(18.0).color(ink)).frame(false)).on_hover_text(tr("New note")).clicked() {
            action = NoteAction::New;
        }
        bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let hide_tip = if is_card { tr("Hide (the list stays pinned in RustDo's Sticky notes list)") } else { tr("Hide note (it stays in RustDo's Sticky notes list)") };
            if ui.add(egui::Button::new(RichText::new("✖").color(bar_ink)).frame(false)).on_hover_text(hide_tip).clicked() {
                action = NoteAction::Hide;
            }
            if is_card {
                ui.menu_button(RichText::new("⚙").color(bar_ink), |ui| card_menu(ui, &mut edited, &mut action, true, &sections))
                    .response
                    .on_hover_text(tr("Text size, completed tasks and more"));
            } else {
                ui.menu_button(RichText::new("🎨").color(ink), |ui| {
                    note_menu(ui, &mut edited, &mut action, true, &sections, false);
                })
                .response
                .on_hover_text(tr("Colour, text and more"));
            }
            if edited.on_top {
                ui.label(RichText::new("📌").color(bar_ink)).on_hover_text(tr("Kept on top of other windows"));
            }
            if is_card {
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| card_title(ui, card.as_ref(), bar_ink, 15.0));
            }
        });

        // The text.
        let body = egui::Rect::from_min_max(full.min + Vec2::new(0.0, HEADER_H), full.max);
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body.shrink(10.0)).layout(Layout::top_down(Align::Min)));
        if confirming {
            body_ui.label(RichText::new(tr("Delete this note?")).strong().size(16.0));
            body_ui.label(RichText::new(tr("This can't be undone.")).small());
            body_ui.horizontal(|ui| {
                if ui.button(RichText::new(tr("Delete")).color(ink)).clicked() {
                    action = NoteAction::Delete;
                }
                if ui.button(RichText::new(tr("Cancel")).color(ink)).clicked() {
                    action = NoteAction::CancelDelete;
                }
            });
        } else if is_card {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(&mut body_ui, |ui| {
                card_body(ui, &edited, card.as_ref(), &mut card_actions, ink);
            });
        } else {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(&mut body_ui, |ui| {
                if edited.is_checklist() {
                    checklist_body(ui, &mut edited, ink);
                    return;
                }
                let r = ui.add(
                    egui::TextEdit::multiline(&mut edited.text)
                        .font(FontId::proportional(edited.text_size))
                        .text_color(ink)
                        .hint_text(RichText::new(tr("Take a note…")).color(ink.gamma_multiply(0.5)))
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
    if !card_actions.is_empty() {
        if let Some(c) = note.list.and_then(|l| s.cards.get_mut(&l)) {
            for a in &card_actions {
                c.preview(a);
            }
        }
        ctx.request_repaint();
        s.card_actions.extend(card_actions);
        // The owner (the main window or the companion) carries them out.
        ctx.request_repaint_of(ViewportId::ROOT);
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
        NoteAction::Archive => {
            s.archive(id, true);
            s.save_if_due(true);
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        // Only offered in the main window (it makes the task); not in a note window.
        NoteAction::ToTask => {}
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
    /// Put the note away in the Archive.
    Archive,
    /// Make a task from the note (the main window only).
    ToTask,
}

/// The 🎨 menu: note colour, text colour and size, keep on top, delete.
/// `desktop`: offer "Keep on top" (a desktop window can float above others; a note on the
/// web wall can't).
fn note_menu(ui: &mut egui::Ui, note: &mut StickyNote, action: &mut NoteAction, desktop: bool, sections: &[String], in_app: bool) {
    ui.set_min_width(220.0);
    // The menu has its own (theme) background: use normal text colours inside it.
    ui.visuals_mut().override_text_color = None;

    // Which page of the menu is showing: the options, or a colour picker. A picker takes the
    // whole menu (a note window is too small for egui's picker to pop up beside it), and the
    // menu always opens on the options page.
    let ctx = ui.ctx().clone();
    let page_id = Id::new(("note_menu_page", note.id));
    let seen_id = menu_seen_id(note.id);
    let pass = ctx.cumulative_pass_nr();
    let last_seen: Option<u64> = ctx.data(|d| d.get_temp(seen_id));
    let just_opened = last_seen.is_none_or(|last| last + 1 < pass);
    let mut page: MenuPage = if just_opened { MenuPage::Options } else { ctx.data(|d| d.get_temp(page_id)).unwrap_or_default() };
    ctx.data_mut(|d| d.insert_temp(seen_id, pass));
    // Clicking another window closes the menu (and the note shrinks back), as clicking
    // anywhere else in the note does.
    if desktop && !just_opened && ctx.input(|i| i.viewport().focused == Some(false)) {
        ui.close_menu();
    }

    match page {
        MenuPage::NoteColour | MenuPage::TextColour => {
            let (title, rgb) = if page == MenuPage::NoteColour {
                (tr("Note colour"), note.color)
            } else {
                (tr("Text colour"), note.ink())
            };
            ui.label(RichText::new(title).strong());
            // Fit the picker's square in the window; scroll if it's smaller still.
            let side = (ctx.screen_rect().height() - HEADER_H - 150.0).clamp(90.0, 200.0);
            egui::ScrollArea::vertical().max_height(ctx.screen_rect().height() - HEADER_H - 60.0).show(ui, |ui| {
                ui.spacing_mut().slider_width = side;
                let mut c = c32(rgb);
                if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
                    let picked = [c.r(), c.g(), c.b()];
                    if page == MenuPage::NoteColour {
                        note.color = picked;
                    } else {
                        note.text_color = Some(picked);
                    }
                }
            });
            if ui.button(tr("✔  Done")).on_hover_text(tr("Keep this colour and go back")).clicked() {
                page = MenuPage::Options;
            }
        }
        MenuPage::Options => {
            ui.label(RichText::new(tr("Note colour")).strong());
            ui.horizontal_wrapped(|ui| {
                for (name, color) in PRESETS {
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                    let selected = note.color == color;
                    ui.painter().rect_filled(rect, CornerRadius::same(4), c32(color));
                    let outline = if selected || r.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
                    ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(if selected { 2.0_f32 } else { 1.0 }, outline), egui::StrokeKind::Inside);
                    if r.on_hover_text(notes::colour_name(name)).clicked() {
                        note.color = color;
                    }
                }
            });
            if colour_row(ui, tr("Any colour:"), note.color) {
                page = MenuPage::NoteColour;
            }
            ui.separator();
            picture_row(ui, note);
            ui.separator();
            ui.label(RichText::new(tr("Text")).strong());
            let mut auto = note.text_color.is_none();
            if ui.checkbox(&mut auto, tr("Automatic colour (suits the note)")).changed() {
                note.text_color = if auto { None } else { Some(note.ink()) };
            }
            if !auto && colour_row(ui, tr("Text colour:"), note.ink()) {
                page = MenuPage::TextColour;
            }
            note_menu_rest(ui, note, action, desktop, sections, in_app);
        }
    }
    ctx.data_mut(|d| d.insert_temp(page_id, page));
}

/// Where a note's 🎨 menu records the last pass it was shown (so: whether it's open).
fn menu_seen_id(note: Uuid) -> Id {
    Id::new(("note_menu_seen", note))
}

/// The room (in points) a note window needs while its 🎨 menu is open.
const MENU_ROOM: Vec2 = Vec2::new(270.0, 530.0);

/// The pages of a note's 🎨 menu.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum MenuPage {
    #[default]
    Options,
    NoteColour,
    TextColour,
}

/// "Label: [swatch] Change…"; true when the user asks to change the colour.
fn colour_row(ui: &mut egui::Ui, label: &str, rgb: [u8; 3]) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        let (rect, swatch) = ui.allocate_exact_size(Vec2::new(32.0, 20.0), Sense::click());
        ui.painter().rect_filled(rect, CornerRadius::same(3), c32(rgb));
        ui.painter().rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0_f32, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
        let button = ui.button(tr("Change…"));
        swatch.on_hover_text(tr("Pick any colour")).clicked() | button.clicked()
    })
    .inner
}

/// Text size, keep on top and delete (the rest of the options page).
fn note_menu_rest(ui: &mut egui::Ui, note: &mut StickyNote, action: &mut NoteAction, desktop: bool, sections: &[String], in_app: bool) {
    ui.horizontal(|ui| {
        ui.label(tr("Size:"));
        if ui.small_button("A−").on_hover_text(tr("Smaller")).clicked() {
            note.text_size = (note.text_size - 2.0).max(MIN_TEXT_SIZE);
        }
        ui.add(egui::Slider::new(&mut note.text_size, MIN_TEXT_SIZE..=MAX_TEXT_SIZE).step_by(1.0).show_value(true));
        if ui.small_button("A+").on_hover_text(tr("Bigger")).clicked() {
            note.text_size = (note.text_size + 2.0).min(MAX_TEXT_SIZE);
        }
    });
    ui.separator();
    // Checklist, pinned, section.
    let mut checklist = note.is_checklist();
    if ui.checkbox(&mut checklist, tr("Checklist")).on_hover_text(tr("Each line becomes an item you can tick off")).changed() {
        note.set_checklist(checklist);
    }
    ui.checkbox(&mut note.pinned, tr("📍  Pin to the top")).on_hover_text(tr("Keeps the note above the others on the wall"));
    section_picker(ui, note, sections);
    ui.separator();
    if desktop {
        ui.checkbox(&mut note.on_top, tr("📌 Keep on top of other windows"));
    }
    if in_app && ui.button(tr("✔  Turn into a task")).on_hover_text(tr("Makes a task from this note (a checklist becomes its subtasks)")).clicked() {
        *action = NoteAction::ToTask;
        ui.close_menu();
    }
    if ui.button(tr("🗄  Archive")).on_hover_text(tr("Puts the note away; bring it back from the Archive")).clicked() {
        *action = NoteAction::Archive;
        ui.close_menu();
    }
    if ui.button(tr("🗑  Delete note…")).clicked() {
        *action = NoteAction::AskDelete;
        ui.close_menu();
    }
}

/// "Section: [None ▾]": one of the sections in use, none, or a new one typed in.
fn section_picker(ui: &mut egui::Ui, note: &mut StickyNote, sections: &[String]) {
    ui.horizontal(|ui| {
        ui.label(tr("Section:"));
        let shown = note.section.clone().unwrap_or_else(|| tr("None").to_string());
        egui::ComboBox::from_id_salt(("note_section", note.id)).selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(note.section.is_none(), tr("None")).clicked() {
                note.section = None;
            }
            for sec in sections {
                if ui.selectable_label(note.section.as_deref() == Some(sec.as_str()), sec).clicked() {
                    note.section = Some(sec.clone());
                }
            }
        });
    });
    // A new section: type its name and press Enter.
    let new_id = Id::new(("new_section", note.id));
    let mut name: String = ui.ctx().data_mut(|d| d.get_temp(new_id)).unwrap_or_default();
    let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text(tr("+ New section")).desired_width(180.0));
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.trim().is_empty() {
        // Same name in another case: the existing section.
        let typed = name.trim().to_string();
        note.section = Some(sections.iter().find(|s| s.eq_ignore_ascii_case(&typed)).cloned().unwrap_or(typed));
        name.clear();
    }
    ui.ctx().data_mut(|d| d.insert_temp(new_id, name));
}

/// What the Sticky notes page asks the main window to do (file dialogs live there).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageAction {
    None,
    Backup,
    Restore,
    /// Phones: the ☰ button (back to the lists).
    ShowLists,
    /// Make a task from this note (and remove the note).
    NoteToTask(Uuid),
    /// A note was archived (the main window offers Undo).
    Archived(Uuid),
    /// "Import from Google Keep…" (pick a Takeout zip).
    ImportKeep,
    /// Windows: "Import from Microsoft Sticky Notes".
    ImportMsNotes,
}

/// What was pressed in the page's top row.
#[derive(Default)]
struct Header {
    new_note: bool,
    tidy: bool,
}

/// The top row, laid out like the task pages: (☰ on phones) 📝 Sticky notes, then
/// "+ New note" and a "More" menu (back up, restore, tidy up) on the right.
fn page_header(ui: &mut egui::Ui, phone: bool, can_tidy: bool, page_action: &mut PageAction) -> Header {
    let mut out = Header::default();
    let touch = crate::app::is_touch(ui);
    ui.horizontal(|ui| {
        if phone {
            let menu = egui::Button::new(RichText::new("☰").size(if touch { 26.0 } else { 20.0 })).frame(false);
            let menu = if touch { menu.min_size(Vec2::splat(44.0)) } else { menu };
            if ui.add(menu).on_hover_text(tr("Lists")).clicked() {
                *page_action = PageAction::ShowLists;
            }
        }
        ui.label(RichText::new(tr("📝 Sticky notes")).size(if phone { 20.0 } else { 24.0 }).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let min = if touch { Vec2::new(44.0, 44.0) } else { Vec2::ZERO };
            ui.menu_button(tr("More"), |ui| {
                if can_tidy && ui.button(tr("Tidy up")).on_hover_text(tr("Line all notes up neatly")).clicked() {
                    out.tidy = true;
                    ui.close_menu();
                }
                if ui.button(tr("Back up notes…")).on_hover_text(tr("Save all sticky notes to a file")).clicked() {
                    *page_action = PageAction::Backup;
                    ui.close_menu();
                }
                if ui.button(tr("Restore notes…")).on_hover_text(tr("Bring notes back from a backup file (your current notes are kept)")).clicked() {
                    *page_action = PageAction::Restore;
                    ui.close_menu();
                }
                // Android can't pick files yet.
                if !cfg!(target_os = "android")
                    && ui
                        .button(tr("Import from Google Keep…"))
                        .on_hover_text(tr("Your Keep notes from a Google Takeout zip (takeout.google.com)"))
                        .clicked()
                {
                    *page_action = PageAction::ImportKeep;
                    ui.close_menu();
                }
                if cfg!(windows)
                    && ui
                        .button(tr("Import from Microsoft Sticky Notes"))
                        .on_hover_text(tr("Copy the notes from the Sticky Notes app on this PC"))
                        .clicked()
                {
                    *page_action = PageAction::ImportMsNotes;
                    ui.close_menu();
                }
            });
            // Small phones: just "+" (the row is narrow), like the task pages' icon buttons.
            let short = phone && ui.available_width() < 200.0;
            let label = if short { RichText::new("+").size(22.0) } else { RichText::new(tr("+ New note")) };
            let new = egui::Button::new(label.color(Color32::WHITE)).fill(Color32::from_rgb(0x3B, 0x82, 0xF6)).min_size(min);
            if ui.add(new).on_hover_text(tr("New note")).clicked() {
                out.new_note = true;
            }
        });
    });
    out
}

/// The main window's "Sticky notes" page: every note, to show, hide, create or delete.
/// The web app shows the notes themselves there instead, on a wall.
pub fn notes_page(ui: &mut egui::Ui, shared: &SharedNotes, phone: bool) -> PageAction {
    // Test hook (debug builds only): show the web wall in the desktop app.
    let test_wall = cfg!(debug_assertions) && std::env::var_os("RUSTDO_TEST_WALL").is_some();
    if notes::WALL || test_wall {
        return wall_page(ui, shared, phone);
    }
    let mut page_action = PageAction::None;
    let mut s = lock(shared);
    if page_header(ui, phone, false, &mut page_action).new_note {
        // Next to the RustDo window (cascading from the newest note if there is one).
        let near = s.notes.notes.last().filter(|n| n.open).and_then(|n| n.pos).or_else(|| {
            ui.ctx().input(|i| i.viewport().outer_rect).map(|r| [r.right() - 300.0, r.top() + 40.0])
        });
        s.new_note(near);
    }
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
            ui.label(RichText::new(tr("No sticky notes yet. Click “+ New note” to put one on your desktop.")).weak());
        });
        return page_action;
    }
    let mut toggle = None;
    let mut delete = None;
    let mut ask_delete = None;
    let mut cancel_delete = false;
    let mut archive = None;
    let confirming = s.confirm_delete;
    let archived_count = s.notes.notes.iter().filter(|n| n.archived).count();
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        // Pinned first, then by section, then the rest; newest first within each.
        let mut rows: Vec<(usize, &StickyNote)> = s.notes.notes.iter().enumerate().filter(|(_, n)| !n.archived).collect();
        rows.sort_by_key(|(i, n)| (!n.pinned, n.section.is_none(), n.section.as_ref().map(|x| x.to_lowercase()), std::cmp::Reverse(*i)));
        for (_, note) in rows {
            let is_card = note.list.is_some();
            let (bg, ink, _, _) = colours(ui.visuals().dark_mode, note);
            let title = match note.list.and_then(|l| s.cards.get(&l)) {
                Some(c) => format!("{} {}", c.icon, c.name),
                None => note.title().to_string(),
            };
            egui::Frame::new()
                .stroke(if is_card { Stroke::new(1.0_f32, ui.visuals().weak_text_color().gamma_multiply(0.5)) } else { Stroke::NONE })
                .fill(bg)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if note.pinned {
                            ui.label(RichText::new("📍").color(ink));
                        }
                        ui.label(RichText::new(title).color(ink).strong());
                        if let Some(sec) = &note.section {
                            ui.label(RichText::new(sec).color(ink.gamma_multiply(0.7)).small());
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.visuals_mut().override_text_color = Some(ink);
                            ui.visuals_mut().widgets.inactive.weak_bg_fill = ink.gamma_multiply(0.12);
                            if confirming == Some(note.id) {
                                if ui.button(tr("Cancel")).clicked() {
                                    cancel_delete = true;
                                }
                                if ui.button(tr("Delete")).clicked() {
                                    delete = Some(note.id);
                                }
                                ui.label(RichText::new(tr("Delete this note?")).color(ink));
                                return;
                            }
                            if is_card {
                                // A pinned list: unpinning loses nothing (the list stays).
                                if ui.button(tr("Unpin")).on_hover_text(tr("Removes this card (the list and its tasks stay)")).clicked() {
                                    delete = Some(note.id);
                                }
                            } else {
                                if ui.button("🗑").on_hover_text(tr("Delete note")).clicked() {
                                    ask_delete = Some(note.id);
                                }
                                if ui.button("🗄").on_hover_text(tr("Archive")).clicked() {
                                    archive = Some(note.id);
                                }
                                if ui.button("✔").on_hover_text(tr("Turn into a task")).clicked() {
                                    page_action = PageAction::NoteToTask(note.id);
                                }
                            }
                            let label = if note.open { tr("Hide") } else { tr("Show") };
                            if ui.button(label).clicked() {
                                toggle = Some((note.id, !note.open));
                            }
                        });
                    });
                    let preview: String = if is_card {
                        String::new()
                    } else {
                        note.text.lines().skip_while(|l| l.trim().is_empty()).skip(1).take(2).collect::<Vec<_>>().join("\n")
                    };
                    if !preview.trim().is_empty() {
                        ui.label(RichText::new(preview).color(ink.gamma_multiply(0.8)).small());
                    }
                });
            ui.add_space(6.0);
        }
        if archived_count > 0 {
            ui.add_space(6.0);
            archive_section(ui, &mut s);
        }
    });
    if let Some((id, open)) = toggle {
        s.set_open(id, open);
    }
    if let Some(id) = archive {
        s.archive(id, true);
        page_action = PageAction::Archived(id);
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

// ---------------------------------------------------------------- the web wall

/// Below this width the wall becomes a column of full-width notes (dragging notes around a
/// phone screen would fight with scrolling).
const WALL_MIN_WIDTH: f32 = 560.0;
const WALL_HEADER_H: f32 = 28.0;
/// The top bar on touch screens: room for finger-sized buttons.
const WALL_HEADER_TOUCH_H: f32 = 44.0;

/// The web app's Sticky notes page: the notes themselves, on a wall you can arrange.
fn wall_page(ui: &mut egui::Ui, shared: &SharedNotes, phone: bool) -> PageAction {
    let mut page_action = PageAction::None;
    let mut s = lock(shared);
    let narrow = ui.available_width() < WALL_MIN_WIDTH;
    let header = page_header(ui, phone, !narrow && !s.notes.notes.is_empty(), &mut page_action);
    let tidy = header.tidy;
    if header.new_note {
        let near = s.notes.notes.last().and_then(|n| n.pos);
        s.new_note(near.or(Some([-16.0, -16.0])));
    }
    let kept = if cfg!(target_os = "android") { tr("on this phone") } else { tr("in this browser") };
    let hint = if narrow {
        trf("Notes are kept {kept} and included in your backups.", &[("kept", &kept)])
    } else {
        trf("Drag a note by its top bar, resize it from the corner. Notes are kept {kept} and included in your backups.", &[("kept", &kept)])
    };
    ui.add(egui::Label::new(RichText::new(hint).weak().small()).wrap());
    if let Some(w) = &s.warning {
        ui.colored_label(Color32::from_rgb(0xE5, 0x48, 0x4D), w);
    }
    ui.add_space(6.0);

    let sections = s.notes.sections();
    let archived_count = s.notes.notes.iter().filter(|n| n.archived).count();
    if s.notes.notes.iter().all(|n| n.archived) {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("📝").size(40.0));
            let verb = if crate::app::is_touch(ui) { tr("Tap") } else { tr("Click") };
            ui.add(egui::Label::new(RichText::new(trf("No sticky notes yet. {verb} “+ New note” to add one.", &[("verb", &verb)])).weak()).wrap());
        });
        if archived_count > 0 {
            archive_section(ui, &mut s);
        }
        return page_action;
    }

    // Filter chips (once there are two or more sections): All · Pinned · each section.
    let filter_id = Id::new("note_filter");
    let mut filter: NoteFilter = ui.ctx().data(|d| d.get_temp(filter_id)).unwrap_or_default();
    let any_pinned = s.notes.notes.iter().any(|n| n.pinned && !n.archived);
    if sections.len() >= 2 {
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(filter == NoteFilter::All, tr("All")).clicked() {
                filter = NoteFilter::All;
            }
            if any_pinned && ui.selectable_label(filter == NoteFilter::Pinned, tr("📍 Pinned")).clicked() {
                filter = NoteFilter::Pinned;
            }
            for sec in &sections {
                let on = matches!(&filter, NoteFilter::Section(f) if f.eq_ignore_ascii_case(sec));
                if ui.selectable_label(on, sec).clicked() {
                    filter = NoteFilter::Section(sec.clone());
                }
            }
        });
        ui.add_space(4.0);
    } else {
        filter = NoteFilter::All;
    }
    // A filter whose section is gone shows everything again.
    if let NoteFilter::Section(f) = &filter {
        if !sections.iter().any(|s| s.eq_ignore_ascii_case(f)) {
            filter = NoteFilter::All;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(filter_id, filter.clone()));

    let wall_width = ui.available_width();
    if tidy {
        s.notes.tidy(wall_width);
        s.changed();
    }
    if !narrow && s.notes.place_unplaced(wall_width) {
        s.changed();
    }

    let mut actions: Vec<(Uuid, WallAction)> = Vec::new();
    if narrow {
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            let groups = s.notes.column_groups();
            // Headings only when there's something to tell apart.
            let headings = groups.len() > 1 || groups.iter().any(|(g, _)| *g != notes::Group::Other);
            let mut shown: Vec<Uuid> = Vec::new();
            let mut rects: Vec<(Uuid, egui::Rect)> = Vec::new();
            for (group, ids) in groups {
                let ids: Vec<Uuid> = ids.into_iter().filter(|id| s.notes.get(*id).is_some_and(|n| filter.matches(n))).collect();
                if ids.is_empty() {
                    continue;
                }
                if headings && filter == NoteFilter::All {
                    let title = match &group {
                        notes::Group::Pinned => tr("📍 Pinned").to_string(),
                        notes::Group::Section(name) => name.clone(),
                        notes::Group::Other => tr("Other notes").to_string(),
                    };
                    ui.add_space(4.0);
                    ui.label(RichText::new(title).strong().weak());
                }
                for id in ids {
                    let Shared { notes, confirm_delete, focus, cards, card_actions, .. } = &mut *s;
                    let Some(note) = notes.get_mut(id) else { continue };
                    let height = note.size[1].clamp(140.0, 420.0);
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
                    let focus_this = *focus == Some(id);
                    let card = note.list.and_then(|l| cards.get(&l));
                    if let Some(a) = wall_card(ui, note, rect, *confirm_delete == Some(id), focus_this, false, &sections, card, card_actions) {
                        actions.push((id, a));
                    }
                    if focus_this {
                        *focus = None;
                    }
                    rects.push((id, rect));
                    shown.push(id);
                    ui.add_space(10.0);
                }
            }
            // A note being dragged by its handle: show where it will go, move it on release.
            let drag_id = Id::new("note_drag");
            if let Some(dragged) = ui.ctx().data(|d| d.get_temp::<Uuid>(drag_id)) {
                let (pointer, released) = ui.input(|i| (i.pointer.interact_pos(), !i.pointer.primary_down()));
                match pointer {
                    Some(pos) if shown.contains(&dragged) => {
                        let others: Vec<&(Uuid, egui::Rect)> = rects.iter().filter(|(id, _)| *id != dragged).collect();
                        let to = others.iter().filter(|(_, r)| r.center().y < pos.y).count();
                        let y = match others.get(to) {
                            Some((_, r)) => r.top() - 5.0,
                            None => others.last().map_or(pos.y, |(_, r)| r.bottom() + 5.0),
                        };
                        ui.painter().hline(ui.min_rect().x_range(), y, Stroke::new(3.0_f32, Color32::from_rgb(0x3B, 0x82, 0xF6)));
                        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                        edge_scroll(ui, pos);
                        if released {
                            // The new order of what's shown (a note stays in its group).
                            let mut seq: Vec<Uuid> = shown.iter().copied().filter(|id| *id != dragged).collect();
                            seq.insert(to.min(seq.len()), dragged);
                            s.notes.arrange(&seq);
                            s.changed();
                            ui.ctx().data_mut(|d| d.remove::<Uuid>(drag_id));
                        }
                    }
                    _ if released => ui.ctx().data_mut(|d| d.remove::<Uuid>(drag_id)),
                    _ => {}
                }
            }
            if archived_count > 0 {
                ui.add_space(8.0);
                archive_section(ui, &mut s);
            }
        });
    } else {
        // Dragging is for moving and resizing notes, so the wall scrolls with the wheel and
        // scrollbars only (drag-to-scroll would move the wall under the note being dragged).
        egui::ScrollArea::both().auto_shrink([false; 2]).drag_to_scroll(false).show(ui, |ui| {
            // The wall grows to hold every note, and is at least as big as the page.
            let extent = s.notes.notes.iter().filter(|n| !n.archived).fold(ui.available_size(), |acc, n| {
                let [x, y] = n.pos.unwrap_or([0.0, 0.0]);
                acc.max(Vec2::new(x + n.size[0] + notes::WALL_GAP, y + n.size[1] + notes::WALL_GAP))
            });
            let (wall, _) = ui.allocate_exact_size(extent, Sense::hover());
            // A faint dot grid, like a board.
            let dot = ui.visuals().weak_text_color().gamma_multiply(0.25);
            let mut y = wall.top() + 24.0;
            while y < wall.bottom() {
                let mut x = wall.left() + 24.0;
                while x < wall.right() {
                    ui.painter().circle_filled(Pos2::new(x, y), 1.0, dot);
                    x += 24.0;
                }
                y += 24.0;
            }
            // Bottom note first, so the top note is drawn (and takes clicks) last. Pinned
            // notes stay on top of the others.
            let mut ids: Vec<Uuid> = s.notes.notes.iter().filter(|n| !n.archived && filter.matches(n)).map(|n| n.id).collect();
            ids.sort_by_key(|id| s.notes.get(*id).is_some_and(|n| n.pinned));
            for id in ids {
                let Shared { notes, confirm_delete, focus, cards, card_actions, .. } = &mut *s;
                let Some(note) = notes.get_mut(id) else { continue };
                let [x, y] = note.pos.unwrap_or([0.0, 0.0]);
                let rect = egui::Rect::from_min_size(wall.min + Vec2::new(x, y), Vec2::from(note.size));
                let focus_this = *focus == Some(id);
                let card = note.list.and_then(|l| cards.get(&l));
                if let Some(a) = wall_card(ui, note, rect, *confirm_delete == Some(id), focus_this, true, &sections, card, card_actions) {
                    actions.push((id, a));
                }
                if focus_this {
                    *focus = None;
                }
            }
            if archived_count > 0 {
                ui.add_space(8.0);
                archive_section(ui, &mut s);
            }
        });
    }

    for (id, action) in actions {
        match action {
            WallAction::Edited => s.changed(),
            WallAction::Moved => {
                s.notes.bring_to_front(id);
                s.changed();
            }
            WallAction::Front => {
                if s.notes.notes.last().map(|n| n.id) != Some(id) {
                    s.notes.bring_to_front(id);
                    s.changed();
                }
            }
            WallAction::New => {
                let near = s.notes.notes.iter().find(|n| n.id == id).and_then(|n| n.pos);
                s.new_note(near);
            }
            WallAction::AskDelete => s.confirm_delete = Some(id),
            WallAction::CancelDelete => s.confirm_delete = None,
            WallAction::Delete => s.delete(id),
            WallAction::Archive => {
                s.archive(id, true);
                page_action = PageAction::Archived(id);
            }
            WallAction::ToTask => page_action = PageAction::NoteToTask(id),
        }
    }
    s.save_if_due(false);
    if s.dirty {
        ui.ctx().request_repaint_after(SAVE_DELAY);
    }
    page_action
}

enum WallAction {
    Edited,
    Moved,
    Front,
    New,
    AskDelete,
    CancelDelete,
    Delete,
    Archive,
    ToTask,
}

/// One note on the wall (or, on a phone, in the column). `movable`: drag the top bar to
/// move it and the corner to resize it.
#[allow(clippy::too_many_arguments)]
fn wall_card(
    ui: &mut egui::Ui,
    note: &mut StickyNote,
    rect: egui::Rect,
    confirming: bool,
    focus_text: bool,
    movable: bool,
    sections: &[String],
    list_card: Option<&CardList>,
    card_actions: &mut Vec<CardAction>,
) -> Option<WallAction> {
    let id = note.id;
    let is_card = note.list.is_some();
    let (bg, ink, header, bar_ink) = colours(ui.visuals().dark_mode, note);
    let mut action = None;
    let before = note.clone();

    // Shadow, paper and top bar.
    let radius = CornerRadius::same(6);
    ui.painter().rect_filled(rect.translate(Vec2::new(2.0, 3.0)), radius, Color32::from_black_alpha(60));
    ui.painter().rect_filled(rect, radius, bg);
    let touch = crate::app::is_touch(ui);
    let header_h = if touch { WALL_HEADER_TOUCH_H } else { WALL_HEADER_H };
    let bar = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), header_h));
    paint_picture(ui, note, egui::Rect::from_min_max(bar.left_bottom(), rect.max), CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 });
    ui.painter().rect_filled(bar, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, header);

    // Clicking anywhere on the note brings it to the front (checked before the widgets).
    if movable && ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(rect) {
        action = Some(WallAction::Front);
    }
    if movable {
        let drag = ui.interact(bar, Id::new(("wall_bar", id)), Sense::drag()).on_hover_cursor(CursorIcon::Grab);
        if drag.dragged() && !drag.drag_started() {
            let d = drag.drag_delta();
            let [x, y] = note.pos.unwrap_or([0.0, 0.0]);
            note.pos = Some([(x + d.x).max(0.0), (y + d.y).max(0.0)]);
        }
    }

    let mut card = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
    let v = card.visuals_mut();
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

    let mut bar_ui = card.new_child(
        UiBuilder::new().max_rect(bar.shrink2(Vec2::new(4.0, 1.0))).layout(Layout::left_to_right(Align::Center)),
    );
    // Touch: bigger icons with finger-sized (44 pt) tap areas.
    let (icon_size, plus_size, tap) = if touch { (22.0, 26.0, Vec2::new(44.0, 40.0)) } else { (14.0, 17.0, Vec2::ZERO) };
    if !is_card && bar_ui.add(egui::Button::new(RichText::new("+").size(plus_size).color(ink)).frame(false).min_size(tap)).on_hover_text(tr("New note")).clicked() {
        action = Some(WallAction::New);
    }
    bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let mut menu_action = NoteAction::None;
        if touch {
            ui.spacing_mut().button_padding = Vec2::new(10.0, 6.0);
        }
        if is_card {
            ui.menu_button(RichText::new("⚙").size(icon_size).color(bar_ink), |ui| card_menu(ui, note, &mut menu_action, false, sections))
                .response
                .on_hover_text(tr("Text size, completed tasks and more"));
        } else {
            if ui.add(egui::Button::new(RichText::new("🗑").size(icon_size).color(ink)).frame(false).min_size(tap)).on_hover_text(tr("Delete note")).clicked() {
                action = Some(WallAction::AskDelete);
            }
            ui.menu_button(RichText::new("🎨").size(icon_size).color(ink), |ui| note_menu(ui, note, &mut menu_action, false, sections, true))
                .response
                .on_hover_text(tr("Colour and text"));
        }
        // The phone column: a ≡ handle drags the note up or down (the rest scrolls).
        if !movable {
            let size = if touch { tap } else { Vec2::new(24.0, 20.0) };
            let (r, handle) = ui.allocate_exact_size(size, Sense::drag());
            let handle = handle.on_hover_text(tr("Drag to move the note up or down")).on_hover_cursor(CursorIcon::Grab);
            let color = if handle.dragged() { bar_ink } else { bar_ink.gamma_multiply(0.6) };
            for dy in [-5.0, 0.0, 5.0] {
                ui.painter().hline((r.center().x - 8.0)..=(r.center().x + 8.0), r.center().y + dy, Stroke::new(2.0_f32, color));
            }
            if handle.drag_started() {
                ui.ctx().data_mut(|d| d.insert_temp(Id::new("note_drag"), id));
            }
        }
        if is_card {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| card_title(ui, list_card, bar_ink, if touch { 17.0 } else { 14.0 }));
        }
        match menu_action {
            NoteAction::AskDelete => action = Some(WallAction::AskDelete),
            NoteAction::Delete => action = Some(WallAction::Delete),
            NoteAction::Archive => action = Some(WallAction::Archive),
            NoteAction::ToTask => action = Some(WallAction::ToTask),
            _ => {}
        }
    });

    let body = egui::Rect::from_min_max(rect.min + Vec2::new(0.0, header_h), rect.max);
    // The phone column: swipe a note sideways (starting below its top bar) to archive it.
    if !movable && touch {
        let swipe_id = Id::new(("note_swipe", id));
        let (down, origin, pos) = ui.input(|i| (i.pointer.primary_down(), i.pointer.press_origin(), i.pointer.interact_pos()));
        let threshold = (rect.width() * 0.35).max(90.0);
        if down && origin.is_some_and(|o| body.contains(o)) {
            if let (Some(o), Some(p)) = (origin, pos) {
                let d = p - o;
                if d.x.abs() > 24.0 && d.x.abs() > d.y.abs() * 1.5 {
                    ui.ctx().data_mut(|m| m.insert_temp(swipe_id, d.x));
                }
            }
        }
        if let Some(dx) = ui.ctx().data(|m| m.get_temp::<f32>(swipe_id)) {
            if down {
                let w = dx.abs().min(rect.width());
                let strip = if dx > 0.0 {
                    egui::Rect::from_min_size(rect.min, Vec2::new(w, rect.height()))
                } else {
                    egui::Rect::from_min_max(Pos2::new(rect.right() - w, rect.top()), rect.max)
                };
                let gray = if dx.abs() >= threshold { Color32::from_gray(90) } else { Color32::from_gray(140) };
                ui.painter().rect_filled(strip, CornerRadius::same(6), gray);
                if w > 40.0 {
                    ui.painter().text(strip.center(), egui::Align2::CENTER_CENTER, "🗄", FontId::proportional(24.0), Color32::WHITE);
                }
                ui.ctx().request_repaint();
            } else {
                ui.ctx().data_mut(|m| m.remove::<f32>(swipe_id));
                if dx.abs() >= threshold {
                    action = Some(WallAction::Archive);
                }
            }
        }
    }
    let mut body_ui = card.new_child(UiBuilder::new().max_rect(body.shrink(10.0)).layout(Layout::top_down(Align::Min)));
    if confirming {
        body_ui.label(RichText::new(tr("Delete this note?")).strong().size(16.0));
        body_ui.label(RichText::new(tr("This can't be undone.")).small());
        body_ui.horizontal(|ui| {
            if ui.button(RichText::new(tr("Delete")).color(ink)).clicked() {
                action = Some(WallAction::Delete);
            }
            if ui.button(RichText::new(tr("Cancel")).color(ink)).clicked() {
                action = Some(WallAction::CancelDelete);
            }
        });
    } else if is_card {
        egui::ScrollArea::vertical().id_salt(("wall_card", id)).auto_shrink([false; 2]).show(&mut body_ui, |ui| {
            card_body(ui, note, list_card, card_actions, ink);
        });
    } else {
        egui::ScrollArea::vertical()
            .id_salt(("wall_text", id))
            .auto_shrink([false; 2])
            .drag_to_scroll(false)
            .show(&mut body_ui, |ui| {
            if note.is_checklist() {
                checklist_body(ui, note, ink);
                return;
            }
            let r = ui.add(
                egui::TextEdit::multiline(&mut note.text)
                    .id(Id::new(("wall_edit", id)))
                    .font(FontId::proportional(note.text_size))
                    .text_color(ink)
                    .hint_text(RichText::new(tr("Take a note…")).color(ink.gamma_multiply(0.5)))
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .desired_rows(((body.height() - 20.0) / (note.text_size * 1.3)).max(1.0) as usize),
            );
            if focus_text {
                r.request_focus();
            }
        });
    }

    // Resize from the bottom-right corner.
    // Touch: a finger-sized corner (the lines are drawn bigger to match).
    let grip_size = if touch { 40.0 } else { 18.0 };
    let grip = egui::Rect::from_min_max(rect.max - Vec2::splat(grip_size), rect.max);
    if movable {
        let step = if touch { 7.0 } else { 4.0 };
        for i in 1..=3 {
            let d = i as f32 * step;
            ui.painter().line_segment(
                [grip.right_bottom() - Vec2::new(d, 3.0), grip.right_bottom() - Vec2::new(3.0, d)],
                Stroke::new(if touch { 1.5_f32 } else { 1.0 }, ink.gamma_multiply(0.5)),
            );
        }
        let g = ui.interact(grip, Id::new(("wall_grip", id)), Sense::drag()).on_hover_cursor(CursorIcon::ResizeNwSe);
        // Not on the frame the drag starts: a finger (or a fast mouse) lands without a hover
        // first, and that frame's "movement" is the jump from wherever the pointer was.
        if g.dragged() && !g.drag_started() {
            let d = g.drag_delta();
            note.size = [(note.size[0] + d.x).max(160.0), (note.size[1] + d.y).max(120.0)];
        }
    }

    if *note != before {
        if note.text != before.text {
            note.updated = now();
        }
        note.text_size = note.text_size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
        let moved = note.pos != before.pos || note.size != before.size;
        // A note that was clicked (to the front) and then dragged or typed in: save it,
        // and keep it on top.
        let to_front = matches!(action, Some(WallAction::Front));
        if action.is_none() || to_front {
            action = Some(if moved || to_front { WallAction::Moved } else { WallAction::Edited });
        }
    }
    action
}

// ------------------------------------------------------------------ pictures

/// The data folder (pictures live in its `Backgrounds` folder, shared with the app's
/// backgrounds, so they sync the same way). None on the web: browser storage.
static PICTURE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn picture_dir() -> Option<PathBuf> {
    PICTURE_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// How strongly the note colour tints its picture (so the text stays readable).
const PICTURE_TINT: f32 = 0.55;

/// Loaded note pictures: key → texture, or when loading last failed (a picture that
/// hasn't arrived from another device yet is tried again now and then).
#[derive(Clone, Default)]
struct PictureCache(HashMap<String, Result<egui::TextureHandle, f64>>);

fn picture_texture(ctx: &egui::Context, picture: &Background) -> Option<egui::TextureHandle> {
    let key = match picture {
        Background::None => return None,
        Background::Builtin(id) => format!("note-builtin:{id}"),
        Background::Custom(id) => format!("note-custom:{id}"),
    };
    let now = ctx.input(|i| i.time);
    let cache_id = Id::new("note_picture_cache");
    let cached = ctx.data_mut(|d| d.get_temp_mut_or_default::<PictureCache>(cache_id).0.get(&key).cloned());
    match cached {
        Some(Ok(t)) => return Some(t),
        Some(Err(at)) if now - at < 5.0 => return None,
        _ => {}
    }
    let image = match picture {
        Background::None => None,
        Background::Builtin(id) => backgrounds::builtin(id).map(|b| {
            let (w, h) = (240, 240);
            egui::ColorImage::from_rgba_unmultiplied([w, h], &backgrounds::render_builtin(b, w, h))
        }),
        Background::Custom(id) => backgrounds::load_one(picture_dir().as_deref(), id)
            .and_then(|jpeg| backgrounds::decode(&jpeg))
            .map(|(w, h, px)| egui::ColorImage::from_rgba_unmultiplied([w, h], &px)),
    };
    let entry = match image {
        Some(image) => Ok(ctx.load_texture(&key, image, egui::TextureOptions::LINEAR)),
        None => Err(now),
    };
    ctx.data_mut(|d| d.get_temp_mut_or_default::<PictureCache>(cache_id).0.insert(key, entry.clone()));
    entry.ok()
}

/// Forgets a loaded picture (it was replaced or removed).
pub fn forget_picture(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<PictureCache>(Id::new("note_picture_cache")).0.retain(|k, _| !k.ends_with(id)));
}

/// The note's picture (if it has one) over `rect`, tinted with the note colour.
fn paint_picture(ui: &egui::Ui, note: &StickyNote, rect: egui::Rect, corner: CornerRadius) {
    let Some(picture) = &note.picture else { return };
    let Some(texture) = picture_texture(ui.ctx(), picture) else { return };
    let uv = crate::app::cover_uv(texture.size_vec2(), rect.size());
    let p = ui.painter();
    p.add(egui::epaint::RectShape::filled(rect, corner, Color32::WHITE).with_texture(texture.id(), uv));
    let [r, g, b] = note.color;
    p.rect_filled(rect, corner, Color32::from_rgba_unmultiplied(r, g, b, (PICTURE_TINT * 255.0) as u8));
}

/// Asks the app to pick a picture for a note (the web and Android pick it themselves).
#[cfg_attr(not(any(target_arch = "wasm32", target_os = "android")), allow(dead_code))]
fn request_picture(ctx: &egui::Context, note: Uuid) {
    ctx.data_mut(|d| d.insert_temp(Id::new("note_picture_request"), Some(note)));
}

/// The note that wants a picture picked, if one asked since last time.
pub fn take_picture_request(ctx: &egui::Context) -> Option<Uuid> {
    ctx.data_mut(|d| d.remove_temp::<Option<Uuid>>(Id::new("note_picture_request")).flatten())
}

/// Stores a picked picture (shrunk to a JPEG copy) in the pictures folder; its id.
pub fn store_picture(bytes: &[u8]) -> Result<String, String> {
    let jpeg = backgrounds::prepare(bytes)?;
    let id = backgrounds::image_id(&jpeg);
    backgrounds::save(picture_dir().as_deref(), &id, &jpeg)?;
    Ok(id)
}

/// "Picture: none, the built-in ones, your pictures, + Picture…" in a note's 🎨 menu.
fn picture_row(ui: &mut egui::Ui, note: &mut StickyNote) {
    ui.label(RichText::new(tr("Picture")).strong());
    let mut choices: Vec<(Option<Background>, String)> = vec![(None, tr("No picture").to_string())];
    for b in &backgrounds::BUILTINS {
        choices.push((Some(Background::Builtin(b.id.to_string())), backgrounds::builtin_name(b.id).to_string()));
    }
    for id in backgrounds::list_ids(picture_dir().as_deref()) {
        choices.push((Some(Background::Custom(id)), tr("Your picture").to_string()));
    }
    let size = Vec2::new(36.0, 28.0);
    ui.horizontal_wrapped(|ui| {
        for (value, name) in choices {
            let (rect, r) = ui.allocate_exact_size(size, Sense::click());
            let corner = CornerRadius::same(4);
            let texture = value.as_ref().and_then(|v| picture_texture(ui.ctx(), v));
            match &texture {
                Some(t) => {
                    let uv = crate::app::cover_uv(t.size_vec2(), rect.size());
                    ui.painter().add(egui::epaint::RectShape::filled(rect, corner, Color32::WHITE).with_texture(t.id(), uv));
                }
                None => {
                    ui.painter().rect_filled(rect, corner, c32(note.color));
                    if value.is_none() {
                        ui.painter().line_segment([rect.left_bottom() + Vec2::new(5.0, -5.0), rect.right_top() + Vec2::new(-5.0, 5.0)], Stroke::new(1.5_f32, c32(note.ink())));
                    }
                }
            }
            let selected = note.picture == value;
            let outline = if selected || r.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
            ui.painter().rect_stroke(rect, corner, Stroke::new(if selected { 2.0_f32 } else { 1.0 }, outline), egui::StrokeKind::Inside);
            if r.on_hover_text(name).clicked() {
                note.picture = value;
            }
        }
    });
    if ui.button(tr("+ Picture…")).on_hover_text(tr("Use one of your own pictures on this note")).clicked() {
        pick_picture(ui, note);
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn pick_picture(ui: &mut egui::Ui, note: &mut StickyNote) {
    let _ = ui;
    let picked = rfd::FileDialog::new()
        .set_title(tr("Pick a picture"))
        .add_filter(tr("Pictures"), &["jpg", "jpeg", "png"])
        .pick_file();
    let Some(path) = picked else { return };
    match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| store_picture(&bytes)) {
        Ok(id) => note.picture = Some(Background::Custom(id)),
        Err(e) => {
            let _ = rfd::MessageDialog::new()
                .set_title("RustDo")
                .set_description(trf("Couldn't use that picture: {e}", &[("e", &e)]))
                .show();
        }
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn pick_picture(ui: &mut egui::Ui, note: &mut StickyNote) {
    request_picture(ui.ctx(), note.id);
    ui.close_menu();
}

/// While something is dragged near the top or bottom of a scrolling list, scroll it, so
/// it can be carried further than the screen shows.
pub fn edge_scroll(ui: &mut egui::Ui, pos: Pos2) {
    let clip = ui.clip_rect();
    let zone = 70.0;
    let speed = if pos.y > clip.bottom() - zone {
        -(pos.y - (clip.bottom() - zone)).min(zone) * 0.25
    } else if pos.y < clip.top() + zone {
        (clip.top() + zone - pos.y).min(zone) * 0.25
    } else {
        0.0
    };
    if speed != 0.0 {
        ui.scroll_with_delta(Vec2::new(0.0, speed));
        ui.ctx().request_repaint();
    }
}

/// A checklist note: its heading, then each item with a tick box. Enter adds an item
/// below; an emptied item goes away when you leave it; "+ Add item" at the end.
/// Returns whether anything changed.
fn checklist_body(ui: &mut egui::Ui, note: &mut StickyNote, ink: Color32) -> bool {
    let id = note.id;
    let size = note.text_size;
    let mut items = note.items();
    let mut changed = false;
    let focus_id = Id::new(("check_focus", id));
    let want_focus: Option<usize> = ui.ctx().data_mut(|d| d.remove_temp(focus_id));
    if let Some(h) = note.heading() {
        ui.label(RichText::new(h).size(size).strong().color(ink));
    }
    let mut insert_after: Option<usize> = None;
    let mut remove: Option<usize> = None;
    for (i, item) in items.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if ui.add(egui::Checkbox::without_text(&mut item.done)).changed() {
                changed = true;
            }
            let color = if item.done { ink.gamma_multiply(0.5) } else { ink };
            let r = ui.add(
                egui::TextEdit::singleline(&mut item.text)
                    .id(Id::new(("check_item", id, i)))
                    .font(FontId::proportional(size))
                    .text_color(color)
                    .frame(false)
                    .desired_width(f32::INFINITY),
            );
            if want_focus == Some(i) {
                r.request_focus();
            }
            if r.changed() {
                changed = true;
            }
            if r.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    insert_after = Some(i);
                } else if item.text.trim().is_empty() {
                    remove = Some(i);
                }
            }
        });
    }
    if let Some(i) = insert_after {
        items.insert(i + 1, notes::CheckItem { text: String::new(), done: false });
        ui.ctx().data_mut(|d| d.insert_temp(focus_id, i + 1));
        changed = true;
    } else if let Some(i) = remove {
        if items.len() > 1 {
            items.remove(i);
            changed = true;
        }
    }
    // "+ Add item": type and press Enter.
    let add_id = Id::new(("check_add", id));
    let mut new_item: String = ui.ctx().data_mut(|d| d.get_temp(add_id)).unwrap_or_default();
    let r = ui.add(
        egui::TextEdit::singleline(&mut new_item)
            .hint_text(RichText::new(tr("+ Add item")).color(ink.gamma_multiply(0.5)))
            .font(FontId::proportional(size))
            .text_color(ink)
            .frame(false)
            .desired_width(f32::INFINITY),
    );
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !new_item.trim().is_empty() {
        items.push(notes::CheckItem { text: new_item.trim().to_string(), done: false });
        new_item.clear();
        r.request_focus();
        changed = true;
    }
    ui.ctx().data_mut(|d| d.insert_temp(add_id, new_item));
    if changed {
        note.set_items(&items);
    }
    changed
}

/// Which notes the wall shows.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
enum NoteFilter {
    #[default]
    All,
    Pinned,
    Section(String),
}

impl NoteFilter {
    fn matches(&self, n: &StickyNote) -> bool {
        match self {
            Self::All => true,
            Self::Pinned => n.pinned,
            Self::Section(s) => n.section.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(s)),
        }
    }
}

/// "🗄 Archive (3)": the notes put away, folded up; restore or delete each.
fn archive_section(ui: &mut egui::Ui, s: &mut Shared) {
    let archived: Vec<(Uuid, String)> = s.notes.notes.iter().filter(|n| n.archived).map(|n| (n.id, n.title().to_string())).collect();
    let mut restore = None;
    let mut delete = None;
    egui::CollapsingHeader::new(RichText::new(trf("🗄 Archive ({n})", &[("n", &archived.len())])).strong())
        .id_salt("note_archive")
        .default_open(false)
        .show(ui, |ui| {
            for (id, title) in &archived {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(title.as_str()).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("🗑").on_hover_text(tr("Delete note")).clicked() {
                            delete = Some(*id);
                        }
                        if ui.button(tr("Restore")).on_hover_text(tr("Put it back on the wall")).clicked() {
                            restore = Some(*id);
                        }
                    });
                });
            }
        });
    if let Some(id) = restore {
        s.archive(id, false);
    }
    if let Some(id) = delete {
        s.delete(id);
    }
}
