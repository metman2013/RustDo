//! Microsoft Sticky Notes import (Windows): reads the notes Sticky Notes keeps on this PC,
//! no sign-in.
//!
//! Sticky Notes 3 and later keep them in an SQLite file, `plum.sqlite`, in its package
//! folder (`%LOCALAPPDATA%\Packages\Microsoft.MicrosoftStickyNotes_8wekyb3d8bbwe\
//! LocalState`). Its `Note` table has the text (each paragraph starts with an
//! `\id=<guid> ` marker), a theme name (Yellow, Green, …) and .NET timestamps. Notes are
//! copied out first, so this works while Sticky Notes is open. The newer Sticky Notes
//! inside OneNote keeps its notes in OneNote instead; those can't be read here.

#[cfg(any(windows, test))]
use crate::notes::StickyNote;
use chrono::{DateTime, NaiveDateTime};

/// The text without Sticky Notes' paragraph markers (`\id=<guid> `).
pub fn clean_text(raw: &str) -> String {
    let lines: Vec<&str> = raw
        .lines()
        .map(|line| match line.strip_prefix("\\id=") {
            Some(rest) => match rest.find(' ') {
                Some(i) => &rest[i + 1..],
                None => "",
            },
            None => line,
        })
        .collect();
    lines.join("\n").trim().to_string()
}

/// Sticky Notes' themes as RustDo's note colours (the same set).
pub fn theme_colour(theme: &str) -> [u8; 3] {
    match theme {
        "Green" => [0xC5, 0xE8, 0xB7],
        "Pink" => [0xF8, 0xBB, 0xD0],
        "Purple" => [0xD7, 0xC4, 0xF0],
        "Blue" => [0xB3, 0xDB, 0xF7],
        "Gray" => [0xE0, 0xE0, 0xE0],
        "Charcoal" => [0x3A, 0x3A, 0x3A],
        _ => [0xFF, 0xF1, 0x76],
    }
}

/// .NET ticks (100 ns since 0001-01-01, UTC) as local time.
pub fn from_ticks(ticks: i64) -> Option<NaiveDateTime> {
    const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    if ticks <= UNIX_EPOCH_TICKS {
        return None;
    }
    DateTime::from_timestamp_micros((ticks - UNIX_EPOCH_TICKS) / 10).map(|d| d.with_timezone(&chrono::Local).naive_local())
}

/// Where Sticky Notes keeps its notes on this PC, if it's installed.
#[cfg(windows)]
pub fn default_path() -> Option<std::path::PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let path = std::path::Path::new(&local)
        .join("Packages")
        .join("Microsoft.MicrosoftStickyNotes_8wekyb3d8bbwe")
        .join("LocalState")
        .join("plum.sqlite");
    path.is_file().then_some(path)
}

/// Reads the notes from a `plum.sqlite` file (a copy is read, so Sticky Notes can stay open).
#[cfg(any(windows, test))]
pub fn read(path: &std::path::Path, now: NaiveDateTime) -> Result<Vec<StickyNote>, String> {
    let dir = std::env::temp_dir().join(format!("rustdo-sticky-notes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = (|| {
        let name = path.file_name().ok_or("not a file")?;
        let copy = dir.join(name);
        std::fs::copy(path, &copy).map_err(|e| e.to_string())?;
        // Recent changes may still be in the write-ahead log next to it.
        for ext in ["-wal", "-shm"] {
            let mut side = path.as_os_str().to_owned();
            side.push(ext);
            let mut side_copy = copy.as_os_str().to_owned();
            side_copy.push(ext);
            let _ = std::fs::copy(std::path::PathBuf::from(side), std::path::PathBuf::from(side_copy));
        }
        read_db(&copy, now)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(any(windows, test))]
fn read_db(path: &std::path::Path, now: NaiveDateTime) -> Result<Vec<StickyNote>, String> {
    use rusqlite::types::Value;
    let db = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    let mut stmt = db.prepare("SELECT * FROM Note").map_err(|e| e.to_string())?;
    let columns: Vec<String> = stmt.column_names().map(|c| c.to_string()).collect();
    let col = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (text_col, theme_col, id_col) = (col("Text"), col("Theme"), col("Id"));
    let (created_col, updated_col, deleted_col) = (col("CreatedAt"), col("UpdatedAt"), col("DeletedAt"));
    let Some(text_col) = text_col else {
        return Err(crate::i18n::tr("This isn't a Sticky Notes file.").to_string());
    };
    let as_text = |v: &Value| match v {
        Value::Text(s) => Some(s.clone()),
        Value::Blob(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Integer(i) => Some(i.to_string()),
        _ => None,
    };
    let as_int = |v: &Value| match v {
        Value::Integer(i) => Some(*i),
        Value::Text(s) => s.parse().ok(),
        _ => None,
    };
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let get = |i: Option<usize>| i.and_then(|i| row.get::<_, Value>(i).ok()).unwrap_or(Value::Null);
        // Deleted notes wait in the table until they're synced away.
        if !matches!(get(deleted_col), Value::Null) && as_int(&get(deleted_col)).unwrap_or(1) != 0 {
            continue;
        }
        let text = clean_text(&as_text(&get(Some(text_col))).unwrap_or_default());
        if text.is_empty() {
            continue;
        }
        let mut n = StickyNote::new(now, theme_colour(&as_text(&get(theme_col)).unwrap_or_default()));
        n.text = text;
        n.created = as_int(&get(created_col)).and_then(from_ticks).unwrap_or(now);
        n.updated = as_int(&get(updated_col)).and_then(from_ticks).unwrap_or(n.created);
        // Shown from the Sticky notes page, not all at once on the desktop.
        n.open = false;
        n.source_id = Some(format!(
            "msnotes:{}",
            as_text(&get(id_col)).unwrap_or_else(|| format!("{}", n.created.and_utc().timestamp_micros()))
        ));
        out.push(n);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 28).unwrap().and_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn cleans_paragraph_markers() {
        let raw = "\\id=0d7f1a5c-1111-2222-3333-444455556666 Shopping\n\\id=1d7f1a5c-1111-2222-3333-444455556666 Milk and eggs\n\\id=2d7f1a5c-1111-2222-3333-444455556666 ";
        assert_eq!(clean_text(raw), "Shopping\nMilk and eggs");
        assert_eq!(clean_text("plain text"), "plain text");
    }

    #[test]
    fn reads_a_plum_database() {
        let dir = std::env::temp_dir().join(format!("rustdo-msnotes-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plum.sqlite");
        {
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch(
                "CREATE TABLE Note (Text TEXT, WindowPosition TEXT, IsOpen INTEGER, Theme TEXT, Id TEXT PRIMARY KEY,
                   CreatedAt INTEGER, DeletedAt INTEGER, UpdatedAt INTEGER);
                 INSERT INTO Note VALUES ('\\id=aaaa Shopping\n\\id=bbbb Milk', NULL, 1, 'Pink', 'n1', 638700000000000000, NULL, 638700000010000000);
                 INSERT INTO Note VALUES ('\\id=cccc Old note', NULL, 0, 'Yellow', 'n2', 638700000000000000, 638700000020000000, 638700000020000000);
                 INSERT INTO Note VALUES ('\\id=dddd ', NULL, 0, 'Blue', 'n3', 638700000000000000, NULL, 638700000000000000);",
            )
            .unwrap();
        }
        let notes = read(&path, now()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(notes.len(), 1, "deleted and empty notes are skipped");
        let n = &notes[0];
        assert_eq!(n.text, "Shopping\nMilk");
        assert_eq!(n.color, [0xF8, 0xBB, 0xD0]);
        assert_eq!(n.source_id.as_deref(), Some("msnotes:n1"));
        assert!(!n.open);
        assert_eq!(n.created.date().year(), 2024);
        assert!(n.updated > n.created);
    }
}
