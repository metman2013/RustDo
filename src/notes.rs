//! Sticky notes on the desktop (Windows and Linux).
//!
//! Each note is its own small borderless window. Notes are deliberately local: they live in
//! `notes.json` next to the task file, separate from the task store, so they are never sent
//! to a RustDo Hub or included in shared lists.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Sticky notes need real desktop windows: not available on Android or the web.
pub const SUPPORTED: bool = cfg!(not(any(target_os = "android", target_arch = "wasm32")));

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

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
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
    fn title_is_the_first_non_empty_line() {
        let mut n = StickyNote::new(now(), PRESETS[0].1);
        n.text = "\n  Groceries  \nmilk".into();
        assert_eq!(n.title(), "Groceries");
    }

    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
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
