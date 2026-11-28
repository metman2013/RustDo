//! Google Keep import, from a Google Takeout export (takeout.google.com, Keep only).
//!
//! Takeout gives a zip with one JSON file per note (`Takeout/Keep/<title>.json`), next to
//! HTML copies and attachments. Each JSON has the note's title, text or checklist
//! (`listContent`), colour, pinned / archived / trashed flags, labels and timestamps.
//! Notes become sticky notes: the title as the first line (a checklist's heading), the
//! checklist as ☐ / ☑ lines, the first label as the section. Trashed notes are skipped,
//! and each note remembers where it came from, so importing again adds only new ones.

use crate::i18n::trf;
use crate::notes::{StickyNote, CHECKED, UNCHECKED};
use chrono::{DateTime, NaiveDateTime};
use serde::Deserialize;
use std::io::Read;

/// What an import found.
#[derive(Debug, Default)]
pub struct KeepImport {
    pub notes: Vec<StickyNote>,
    /// Notes in Keep's trash (not imported).
    pub trashed: usize,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct KeepNote {
    title: String,
    text_content: String,
    list_content: Vec<KeepItem>,
    color: String,
    is_trashed: bool,
    is_pinned: bool,
    is_archived: bool,
    labels: Vec<KeepLabel>,
    created_timestamp_usec: i64,
    user_edited_timestamp_usec: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct KeepItem {
    text: String,
    is_checked: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct KeepLabel {
    name: String,
}

/// Reads a Takeout zip, or one note's JSON file.
pub fn parse(bytes: &[u8], now: NaiveDateTime) -> Result<KeepImport, String> {
    let mut out = KeepImport::default();
    if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().replace('\\', "/");
            if !name.ends_with(".json") || !name.to_lowercase().contains("keep/") {
                continue;
            }
            let mut text = String::new();
            if file.read_to_string(&mut text).is_err() {
                continue;
            }
            add(&mut out, &text, now);
        }
    } else {
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        add(&mut out, text, now);
    }
    if out.notes.is_empty() && out.trashed == 0 {
        return Err(trf("No Google Keep notes found. Choose the zip from takeout.google.com (with Keep selected).", &[]));
    }
    Ok(out)
}

/// Whether a .json file is a Keep note (rather than a To Do export or a backup).
pub fn looks_like_note(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let text = String::from_utf8_lossy(head);
    text.trim_start().starts_with('{') && text.contains("\"userEditedTimestampUsec\"")
}

fn add(out: &mut KeepImport, json: &str, now: NaiveDateTime) {
    let Ok(k) = serde_json::from_str::<KeepNote>(json) else { return };
    // Not a Keep note (Takeout has other JSON files too).
    if k.created_timestamp_usec == 0 && k.user_edited_timestamp_usec == 0 {
        return;
    }
    if k.is_trashed {
        out.trashed += 1;
        return;
    }
    out.notes.push(note(&k, now));
}

fn note(k: &KeepNote, now: NaiveDateTime) -> StickyNote {
    let mut n = StickyNote::new(now, keep_colour(&k.color));
    let title = k.title.trim();
    let mut lines: Vec<String> = Vec::new();
    if !title.is_empty() {
        lines.push(title.to_string());
    }
    if k.list_content.is_empty() {
        let body = k.text_content.trim_end();
        if !body.is_empty() {
            lines.push(body.to_string());
        }
    } else {
        for item in &k.list_content {
            let text = item.text.trim();
            if !text.is_empty() {
                lines.push(format!("{}{}", if item.is_checked { CHECKED } else { UNCHECKED }, text));
            }
        }
    }
    n.text = lines.join("\n");
    n.pinned = k.is_pinned && !k.is_archived;
    n.archived = k.is_archived;
    n.section = k.labels.iter().map(|l| l.name.trim()).find(|l| !l.is_empty()).map(str::to_string);
    n.created = usec(k.created_timestamp_usec).unwrap_or(now);
    n.updated = usec(k.user_edited_timestamp_usec).unwrap_or(n.created);
    // On the desktop, imported notes start hidden (not dozens of windows at once).
    n.open = false;
    n.source_id = Some(format!("keep:{}", if k.created_timestamp_usec != 0 { k.created_timestamp_usec } else { k.user_edited_timestamp_usec }));
    n
}

fn usec(us: i64) -> Option<NaiveDateTime> {
    (us > 0).then(|| DateTime::from_timestamp_micros(us)).flatten().map(|d| d.naive_local())
}

/// Keep's colours as RustDo's nearest note colours.
fn keep_colour(c: &str) -> [u8; 3] {
    match c {
        "RED" => [0xF8, 0xBB, 0xD0],
        "ORANGE" | "BROWN" => [0xFF, 0xCC, 0x80],
        "YELLOW" => [0xFF, 0xF1, 0x76],
        "GREEN" | "TEAL" => [0xC5, 0xE8, 0xB7],
        "BLUE" | "CERULEAN" => [0xB3, 0xDB, 0xF7],
        "PURPLE" | "PINK" => [0xD7, 0xC4, 0xF0],
        "GRAY" => [0xE0, 0xE0, 0xE0],
        // Keep's default is white; RustDo's notes are yellow by default.
        _ => [0xFF, 0xF1, 0x76],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 28).unwrap().and_hms_opt(12, 0, 0).unwrap()
    }

    const LIST: &str = r#"{"color":"GREEN","isTrashed":false,"isPinned":true,"isArchived":false,"title":"Groceries",
        "listContent":[{"text":"Milk","isChecked":false},{"text":"Bread","isChecked":true}],
        "labels":[{"name":"Shopping"}],"userEditedTimestampUsec":1700000000000000,"createdTimestampUsec":1690000000000000}"#;
    const TEXT: &str = r#"{"color":"DEFAULT","isTrashed":false,"isPinned":false,"isArchived":true,"title":"Wifi",
        "textContent":"Name: home\nPassword: in the drawer","userEditedTimestampUsec":1700000000000001,"createdTimestampUsec":1690000000000001}"#;
    const TRASHED: &str = r#"{"isTrashed":true,"title":"Old","textContent":"x","createdTimestampUsec":5,"userEditedTimestampUsec":5}"#;

    fn takeout_zip() -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            for (name, body) in [
                ("Takeout/Keep/Groceries.json", LIST),
                ("Takeout/Keep/Wifi.json", TEXT),
                ("Takeout/Keep/Old.json", TRASHED),
                ("Takeout/Keep/Groceries.html", "<html></html>"),
                ("Takeout/archive_browser.html", "<html></html>"),
                ("Takeout/Keep/Labels.txt", "Shopping"),
            ] {
                z.start_file(name, opts).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn reads_a_takeout_zip() {
        let imp = parse(&takeout_zip(), now()).unwrap();
        assert_eq!(imp.trashed, 1);
        assert_eq!(imp.notes.len(), 2);
        let g = imp.notes.iter().find(|n| n.title() == "Groceries").unwrap();
        assert_eq!(g.text, "Groceries\n☐ Milk\n☑ Bread");
        assert!(g.is_checklist() && g.pinned && !g.archived);
        assert_eq!(g.section.as_deref(), Some("Shopping"));
        assert_eq!(g.color, [0xC5, 0xE8, 0xB7]);
        assert_eq!(g.source_id.as_deref(), Some("keep:1690000000000000"));
        let w = imp.notes.iter().find(|n| n.title() == "Wifi").unwrap();
        assert_eq!(w.text, "Wifi\nName: home\nPassword: in the drawer");
        assert!(w.archived && !w.pinned);
    }

    #[test]
    fn importing_again_adds_only_new_notes() {
        let mut notes = crate::notes::Notes::default();
        assert_eq!(notes.import(parse(&takeout_zip(), now()).unwrap().notes), 2);
        assert_eq!(notes.import(parse(&takeout_zip(), now()).unwrap().notes), 0);
        assert_eq!(notes.notes.len(), 2);
    }

    #[test]
    fn reads_one_note_and_refuses_other_files() {
        assert_eq!(parse(TEXT.as_bytes(), now()).unwrap().notes.len(), 1);
        assert!(parse(b"{\"something\": \"else\"}", now()).is_err());
        assert!(looks_like_note(TEXT.as_bytes()));
        assert!(!looks_like_note(b"{\"lists\": []}"));
    }
}
