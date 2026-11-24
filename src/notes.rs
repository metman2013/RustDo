//! Sticky notes on the desktop (Windows and Linux).
//!
//! Each note is its own small borderless window. Notes are deliberately local: they live in
//! `notes.json` next to the task file, separate from the task store, so they are never sent
//! to a RustDo Hub or included in shared lists.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Sticky notes as their own desktop windows (Windows and Linux).
pub const SUPPORTED: bool = cfg!(not(any(target_os = "android", target_arch = "wasm32")));

/// The web app and the Android app show sticky notes on a wall inside the app instead
/// (a column of notes on a phone).
pub const WALL: bool = cfg!(any(target_arch = "wasm32", target_os = "android"));

/// Sticky notes in any form: desktop windows, or the wall (web, Android).
pub const AVAILABLE: bool = SUPPORTED || WALL;

/// Room between notes (and from the wall's edge) when placing them.
pub const WALL_GAP: f32 = 16.0;

/// Ready-made note colours (any other colour can be picked too).
pub const PRESETS: [(&str, [u8; 3]); 8] = [
    ("Yellow", [0xFF, 0xF1, 0x76]),
    ("Green", [0xC5, 0xE8, 0xB7]),
    ("Pink", [0xF8, 0xBB, 0xD0]),
    ("Purple", [0xD7, 0xC4, 0xF0]),
    ("Blue", [0xB3, 0xDB, 0xF7]),
    ("Orange", [0xFF, 0xCC, 0x80]),
    ("Gray", [0xE0, 0xE0, 0xE0]),
    ("Charcoal", [0x3A, 0x3A, 0x3A]),
];

pub const MIN_TEXT_SIZE: f32 = 10.0;
pub const MAX_TEXT_SIZE: f32 = 48.0;
pub const DEFAULT_SIZE: [f32; 2] = [280.0, 280.0];

fn default_text_size() -> f32 {
    16.0
}

fn default_size() -> [f32; 2] {
    DEFAULT_SIZE
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StickyNote {
    pub id: Uuid,
    #[serde(default)]
    pub text: String,
    /// Background colour (sRGB).
    pub color: [u8; 3],
    /// Text colour; `None` picks black or white automatically to suit the note colour.
    #[serde(default)]
    pub text_color: Option<[u8; 3]>,
    #[serde(default = "default_text_size")]
    pub text_size: f32,
    /// Shown on the desktop (closing a note hides it; it stays in the list).
    #[serde(default)]
    pub open: bool,
    /// Stays above other windows.
    #[serde(default)]
    pub on_top: bool,
    /// Last position on screen (outer window corner, in points).
    #[serde(default)]
    pub pos: Option<[f32; 2]>,
    #[serde(default = "default_size")]
    pub size: [f32; 2],
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl StickyNote {
    pub fn new(now: NaiveDateTime, color: [u8; 3]) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: String::new(),
            color,
            text_color: None,
            text_size: default_text_size(),
            open: true,
            on_top: false,
            pos: None,
            size: DEFAULT_SIZE,
            created: now,
            updated: now,
        }
    }

    /// The colour the text is drawn in.
    pub fn ink(&self) -> [u8; 3] {
        self.text_color.unwrap_or_else(|| auto_text_color(self.color))
    }

    /// First non-empty line, for lists and window titles.
    pub fn title(&self) -> &str {
        self.text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Empty note")
    }
}

/// All notes, as saved in `notes.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notes {
    #[serde(default)]
    pub notes: Vec<StickyNote>,
}

impl Notes {
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut StickyNote> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    /// A new note, shown at once. It takes the colour of the newest note (so "+" on a pink
    /// note makes another pink one), or yellow.
    pub fn add(&mut self, now: NaiveDateTime, near: Option<[f32; 2]>) -> Uuid {
        let color = self.notes.last().map_or(PRESETS[0].1, |n| n.color);
        let mut note = StickyNote::new(now, color);
        // Cascade new notes a little so they don't sit exactly on top of each other.
        note.pos = near.map(|[x, y]| [x + 32.0, y + 32.0]);
        let id = note.id;
        self.notes.push(note);
        id
    }

    pub fn remove(&mut self, id: Uuid) {
        self.notes.retain(|n| n.id != id);
    }

    /// Brings in notes from a backup without losing any here: new notes are added (hidden,
    /// so a restore doesn't cover the desktop; show them from the Sticky notes page), and a
    /// note that exists in both keeps whichever version was edited last.
    /// Returns (added, updated).
    pub fn merge(&mut self, other: &Notes) -> (usize, usize) {
        let (mut added, mut updated) = (0, 0);
        for theirs in &other.notes {
            match self.notes.iter_mut().find(|n| n.id == theirs.id) {
                Some(ours) => {
                    if theirs.updated > ours.updated {
                        let open = ours.open;
                        *ours = theirs.clone();
                        ours.open = open;
                        updated += 1;
                    }
                }
                None => {
                    let mut n = theirs.clone();
                    n.open = false;
                    self.notes.push(n);
                    added += 1;
                }
            }
        }
        self.repair();
        (added, updated)
    }

    /// Web wall: gives notes without a place (new, or from a desktop backup whose positions
    /// were screen coordinates far off the wall) a free spot. Returns whether any moved.
    pub fn place_unplaced(&mut self, wall_width: f32) -> bool {
        let limit = (wall_width * 3.0).max(4000.0);
        let mut moved = false;
        for i in 0..self.notes.len() {
            let ok = self.notes[i].pos.is_some_and(|[x, y]| x >= 0.0 && y >= 0.0 && x < limit && y < limit);
            if !ok {
                self.notes[i].pos = Some(self.free_spot(i, wall_width));
                moved = true;
            }
        }
        moved
    }

    /// The first grid slot (left to right, top to bottom) not covered by another note.
    fn free_spot(&self, skip: usize, wall_width: f32) -> [f32; 2] {
        let [w, h] = self.notes[skip].size;
        let step_x = w + WALL_GAP;
        let cols = ((wall_width - WALL_GAP) / step_x).floor().max(1.0) as usize;
        for slot in 0.. {
            let (col, row) = (slot % cols, slot / cols);
            let x = WALL_GAP + col as f32 * step_x;
            let y = WALL_GAP + row as f32 * (h + WALL_GAP);
            let taken = self.notes.iter().enumerate().any(|(j, n)| {
                j != skip
                    && n.pos.is_some_and(|[nx, ny]| {
                        x < nx + n.size[0] && nx < x + w && y < ny + n.size[1] && ny < y + h
                    })
            });
            if !taken {
                return [x, y];
            }
        }
        unreachable!()
    }

    /// Web wall: lines every note up in rows that fit `wall_width`, oldest first.
    pub fn tidy(&mut self, wall_width: f32) {
        let mut order: Vec<usize> = (0..self.notes.len()).collect();
        order.sort_by_key(|&i| self.notes[i].created);
        let (mut x, mut y, mut row_h) = (WALL_GAP, WALL_GAP, 0.0_f32);
        for i in order {
            let [w, h] = self.notes[i].size;
            if x > WALL_GAP && x + w > wall_width - WALL_GAP {
                x = WALL_GAP;
                y += row_h + WALL_GAP;
                row_h = 0.0;
            }
            self.notes[i].pos = Some([x, y]);
            x += w + WALL_GAP;
            row_h = row_h.max(h);
        }
    }

    /// Puts a note on top of the others (the last note is drawn last).
    pub fn bring_to_front(&mut self, id: Uuid) {
        if let Some(i) = self.notes.iter().position(|n| n.id == id) {
            if i + 1 != self.notes.len() {
                let n = self.notes.remove(i);
                self.notes.push(n);
            }
        }
    }

    /// Clamps values that a hand-edited or damaged file could get wrong.
    pub fn repair(&mut self) {
        for n in &mut self.notes {
            if !n.text_size.is_finite() {
                n.text_size = default_text_size();
            }
            n.text_size = n.text_size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
            if !(n.size[0].is_finite() && n.size[1].is_finite()) || n.size[0] < 120.0 || n.size[1] < 100.0 {
                n.size = DEFAULT_SIZE;
            }
            if n.pos.is_some_and(|[x, y]| !(x.is_finite() && y.is_finite())) {
                n.pos = None;
            }
        }
    }
}

// ---------------------------------------------------------------- backups

/// Daily automatic notes backups: `RustDo-notes-<date>.json` (next to the task backups).
pub const BACKUP_PREFIX: &str = "RustDo-notes-";

pub fn backup_file_name(date: chrono::NaiveDate) -> String {
    format!("{BACKUP_PREFIX}{}.json", date.format("%Y-%m-%d"))
}

/// The key under which a full RustDo backup (tasks export) carries the sticky notes.
pub const BACKUP_KEY: &str = "sticky_notes";

/// Reads sticky notes from a notes backup (`{"notes": [...]}`) or from a full RustDo
/// backup that carries them under `sticky_notes`.
pub fn parse_backup(bytes: &[u8]) -> Result<Notes, String> {
    let text = String::from_utf8_lossy(bytes);
    let value: serde_json::Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| "This isn't a sticky notes backup.".to_string())?;
    let notes_value = match value.get(BACKUP_KEY) {
        Some(v) => v.clone(),
        None if value.get("notes").is_some_and(|n| n.is_array()) && value.get("default_list").is_none() => value,
        None => return Err("This backup doesn't contain any sticky notes.".into()),
    };
    let mut notes: Notes =
        serde_json::from_value(notes_value).map_err(|e| format!("This sticky notes backup is damaged ({e})."))?;
    notes.repair();
    Ok(notes)
}

/// Relative luminance (WCAG) of an sRGB colour, 0 (black) to 1 (white).
pub fn luminance([r, g, b]: [u8; 3]) -> f32 {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio between two colours (1 to 21).
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

pub const DARK_INK: [u8; 3] = [0x20, 0x20, 0x20];
pub const LIGHT_INK: [u8; 3] = [0xF5, 0xF5, 0xF5];

/// Near-black or near-white, whichever reads better on `background`.
pub fn auto_text_color(background: [u8; 3]) -> [u8; 3] {
    if contrast(DARK_INK, background) >= contrast(LIGHT_INK, background) {
        DARK_INK
    } else {
        LIGHT_INK
    }
}

/// A slightly darker (or, on dark notes, lighter) shade for the note's title bar.
pub fn header_color(background: [u8; 3]) -> [u8; 3] {
    let dark = luminance(background) < 0.2;
    background.map(|c| {
        let c = c as f32;
        (if dark { c + (255.0 - c) * 0.12 } else { c * 0.9 }).round() as u8
    })
}

// ---------------------------------------------------------------- the notes file

#[cfg(not(target_arch = "wasm32"))]
pub mod file {
    use super::Notes;
    use std::fs;
    use std::io;
    use std::path::Path;

    /// Reads `notes.json`; a missing file is no notes. A damaged file is set aside (never
    /// overwritten) and reported.
    pub fn load(path: &Path) -> (Notes, Option<String>) {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (Notes::default(), None),
            Err(e) => return (Notes::default(), Some(format!("Couldn't read your sticky notes: {e}"))),
        };
        match serde_json::from_str::<Notes>(text.trim_start_matches('\u{feff}')) {
            Ok(mut notes) => {
                notes.repair();
                let _ = fs::write(path.with_extension("json.bak"), &text);
                (notes, None)
            }
            Err(e) => {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let aside = path.with_extension(format!("corrupt-{stamp}.json"));
                let _ = fs::rename(path, &aside);
                (Notes::default(), Some(format!("Your sticky notes file was damaged ({e}). It was saved as {}.", aside.display())))
            }
        }
    }

    /// Writes today's `RustDo-notes-<date>.json` into `dir` (replacing today's earlier one)
    /// and deletes all but the newest `keep`. Nothing is written while there are no notes.
    pub fn write_daily_backup(dir: &Path, notes: &Notes, today: chrono::NaiveDate, keep: usize) -> io::Result<()> {
        if notes.notes.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(dir)?;
        save(&dir.join(super::backup_file_name(today)), notes)?;
        let mut backups: Vec<_> = fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.starts_with(super::BACKUP_PREFIX)
                        && n.ends_with(".json")
                        && n[super::BACKUP_PREFIX.len()..].starts_with(|c: char| c.is_ascii_digit())
                })
            })
            .collect();
        backups.sort();
        let excess = backups.len().saturating_sub(keep);
        for old in &backups[..excess] {
            let _ = fs::remove_file(old);
        }
        Ok(())
    }

    /// Atomic save: write a temporary file, then rename it over the old one.
    pub fn save(path: &Path, notes: &Notes) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(notes).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)
    }
}

// ---------------------------------------------------------------- browser storage (web)

#[cfg(target_arch = "wasm32")]
pub mod browser {
    use super::Notes;

    const KEY: &str = "rustdo.notes";

    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    /// The notes kept in this browser (none if there are none, or storage is blocked).
    pub fn load() -> (Notes, Option<String>) {
        let Some(ls) = local_storage() else { return (Notes::default(), None) };
        let Ok(Some(text)) = ls.get_item(KEY) else { return (Notes::default(), None) };
        match serde_json::from_str::<Notes>(&text) {
            Ok(mut notes) => {
                notes.repair();
                let _ = ls.set_item(&format!("{KEY}.bak"), &text);
                (notes, None)
            }
            Err(e) => {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let _ = ls.set_item(&format!("{KEY}.corrupt-{stamp}"), &text);
                (Notes::default(), Some(format!("Your saved sticky notes were damaged ({e}); a copy was kept in browser storage.")))
            }
        }
    }

    pub fn save(notes: &Notes) -> Result<(), String> {
        let ls = local_storage().ok_or("browser storage is unavailable")?;
        let json = serde_json::to_string(notes).map_err(|e| e.to_string())?;
        ls.set_item(KEY, &json).map_err(|_| "browser storage is full or blocked".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap().and_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn automatic_text_colour_is_readable_on_every_preset() {
        for (name, bg) in PRESETS {
            let ink = auto_text_color(bg);
            assert!(contrast(ink, bg) >= 4.5, "{name}: contrast {}", contrast(ink, bg));
        }
        assert_eq!(auto_text_color([0xFF, 0xF1, 0x76]), DARK_INK);
        assert_eq!(auto_text_color([0x3A, 0x3A, 0x3A]), LIGHT_INK);
        assert_eq!(auto_text_color([0, 0, 0x80]), LIGHT_INK);
    }

    #[test]
    fn chosen_text_colour_overrides_automatic() {
        let mut n = StickyNote::new(now(), [0xFF, 0xF1, 0x76]);
        assert_eq!(n.ink(), DARK_INK);
        n.text_color = Some([0xC0, 0, 0]);
        assert_eq!(n.ink(), [0xC0, 0, 0]);
    }

    #[test]
    fn new_notes_follow_the_last_colour_and_cascade() {
        let mut notes = Notes::default();
        let a = notes.add(now(), None);
        notes.get_mut(a).unwrap().color = [1, 2, 3];
        let b = notes.add(now(), Some([100.0, 50.0]));
        let b = notes.notes.iter().find(|n| n.id == b).unwrap();
        assert_eq!(b.color, [1, 2, 3]);
        assert_eq!(b.pos, Some([132.0, 82.0]));
        assert!(b.open);
    }

    #[test]
    fn old_or_damaged_values_are_repaired() {
        let json = r#"{"notes":[{"id":"6f1f2b7e-0d7a-4a43-9d8e-1a2b3c4d5e6f","color":[255,241,118],
            "text_size":500.0,"size":[5.0,5.0],"created":"2026-11-22T12:00:00","updated":"2026-11-22T12:00:00"}]}"#;
        let mut notes: Notes = serde_json::from_str(json).unwrap();
        notes.repair();
        let n = &notes.notes[0];
        assert_eq!(n.text_size, MAX_TEXT_SIZE);
        assert_eq!(n.size, DEFAULT_SIZE);
        assert_eq!(n.text_color, None);
        assert!(!n.open);
        assert_eq!(n.title(), "Empty note");
    }

    #[test]
    fn wall_places_new_notes_in_free_spots_and_tidies_into_rows() {
        let mut notes = Notes::default();
        for _ in 0..3 {
            notes.add(now(), None);
        }
        // A note from a desktop backup sits far off at screen coordinates.
        notes.notes[2].pos = Some([90_000.0, 60.0]);
        assert!(notes.place_unplaced(1000.0));
        let pos: Vec<_> = notes.notes.iter().map(|n| n.pos.unwrap()).collect();
        // 280 px notes, 16 px gaps: three fit side by side in 1000 px.
        assert_eq!(pos, vec![[16.0, 16.0], [312.0, 16.0], [608.0, 16.0]]);
        assert!(!notes.place_unplaced(1000.0), "already placed");

        // Tidy into a narrower wall: two per row.
        notes.tidy(700.0);
        let pos: Vec<_> = notes.notes.iter().map(|n| n.pos.unwrap()).collect();
        assert_eq!(pos, vec![[16.0, 16.0], [312.0, 16.0], [16.0, 312.0]]);
    }

    #[test]
    fn bring_to_front_moves_a_note_last() {
        let mut notes = Notes::default();
        let a = notes.add(now(), None);
        let b = notes.add(now(), None);
        notes.bring_to_front(a);
        assert_eq!(notes.notes.iter().map(|n| n.id).collect::<Vec<_>>(), vec![b, a]);
    }

    #[test]
    fn title_is_the_first_non_empty_line() {
        let mut n = StickyNote::new(now(), PRESETS[0].1);
        n.text = "\n  Groceries  \nmilk".into();
        assert_eq!(n.title(), "Groceries");
    }

    #[test]
    fn merge_adds_new_notes_hidden_and_keeps_the_newest_edit() {
        let t = |m| now() + chrono::Duration::minutes(m);
        let mut mine = Notes::default();
        let shared = mine.add(t(0), None);
        mine.get_mut(shared).unwrap().text = "mine, newer".into();
        mine.get_mut(shared).unwrap().updated = t(10);
        let old_here = mine.add(t(0), None);
        mine.get_mut(old_here).unwrap().text = "old".into();

        let mut backup = mine.clone();
        backup.get_mut(shared).unwrap().text = "backup, older".into();
        backup.get_mut(shared).unwrap().updated = t(5);
        backup.get_mut(old_here).unwrap().text = "backup, newer".into();
        backup.get_mut(old_here).unwrap().updated = t(20);
        let only_in_backup = backup.add(t(0), None);
        backup.get_mut(only_in_backup).unwrap().text = "from backup".into();

        assert_eq!(mine.merge(&backup), (1, 1));
        let text = |n: &Notes, id| n.notes.iter().find(|x| x.id == id).unwrap().text.clone();
        assert_eq!(text(&mine, shared), "mine, newer");
        assert_eq!(text(&mine, old_here), "backup, newer");
        assert_eq!(text(&mine, only_in_backup), "from backup");
        assert!(!mine.notes.iter().find(|x| x.id == only_in_backup).unwrap().open);
        // Merging the same backup again changes nothing.
        assert_eq!(mine.merge(&backup), (0, 0));
    }

    #[test]
    fn reads_notes_backups_and_full_backups() {
        let mut notes = Notes::default();
        let id = notes.add(now(), None);
        notes.get_mut(id).unwrap().text = "Call mum".into();
        let notes_file = serde_json::to_string(&notes).unwrap();
        assert_eq!(parse_backup(notes_file.as_bytes()).unwrap(), notes);

        let full = format!(r#"{{"lists":[],"tasks":[],"default_list":"x","{BACKUP_KEY}":{notes_file}}}"#);
        assert_eq!(parse_backup(full.as_bytes()).unwrap(), notes);

        let tasks_only = r#"{"lists":[],"tasks":[],"default_list":"x"}"#;
        assert!(parse_backup(tasks_only.as_bytes()).unwrap_err().contains("doesn't contain"));
        assert!(parse_backup(b"not json").is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn daily_notes_backups_keep_the_newest() {
        let dir = std::env::temp_dir().join(format!("rustdo-notes-bk-{}", Uuid::new_v4()));
        let mut notes = Notes::default();
        file::write_daily_backup(&dir, &notes, now().date(), 3).unwrap();
        assert!(!dir.exists(), "no notes, no backup");
        notes.add(now(), None);
        for d in 1..=5 {
            let day = chrono::NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
            file::write_daily_backup(&dir, &notes, day, 3).unwrap();
        }
        // A manual "before restore" copy shares the prefix but isn't a daily backup.
        std::fs::write(dir.join("RustDo-notes-before-restore-1.json"), "{}").unwrap();
        file::write_daily_backup(&dir, &notes, chrono::NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(), 3).unwrap();
        let mut names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(
            names,
            ["RustDo-notes-2026-11-04.json", "RustDo-notes-2026-11-05.json", "RustDo-notes-2026-11-06.json", "RustDo-notes-before-restore-1.json"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn notes_file_round_trips() {
        let dir = std::env::temp_dir().join(format!("rustdo-notes-{}", Uuid::new_v4()));
        let path = dir.join("notes.json");
        let mut notes = Notes::default();
        let id = notes.add(now(), Some([10.0, 20.0]));
        notes.get_mut(id).unwrap().text = "Call mum".into();
        file::save(&path, &notes).unwrap();
        let (back, warning) = file::load(&path);
        assert_eq!(warning, None);
        assert_eq!(back, notes);
        let _ = std::fs::remove_dir_all(dir);
    }
}
