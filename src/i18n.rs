//! The app's languages: English (built in) and German (`de.rs`).
//!
//! Every piece of text people see goes through [`tr`] (or [`trf`] when it has values in
//! it), with the English text itself as the key. German comes from the table in `de.rs`;
//! anything missing there shows in English. A test checks that every `tr`/`trf` text in
//! the source has a German version, so new features can't ship half-translated.
//!
//! The language is the device's (Settings › Language › Automatic) unless chosen.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

mod de;

/// Settings › Language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    /// The device's language, if RustDo has it; else English.
    #[default]
    Automatic,
    English,
    German,
}

impl Language {
    pub const ALL: [Language; 3] = [Self::Automatic, Self::English, Self::German];

    /// Each language in its own words (so it can be found whatever is showing).
    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => tr("Automatic"),
            Self::English => "English",
            Self::German => "Deutsch",
        }
    }
}

const EN: u8 = 0;
const DE: u8 = 1;
static CURRENT: AtomicU8 = AtomicU8::new(EN);

/// Switches the app's language (at start-up and when the setting changes).
pub fn set(language: Language) {
    let german = match language {
        Language::German => true,
        Language::English => false,
        Language::Automatic => device_language().is_some_and(|l| l.to_ascii_lowercase().starts_with("de")),
    };
    CURRENT.store(if german { DE } else { EN }, Ordering::Relaxed);
}

/// True while German is showing.
pub fn german() -> bool {
    CURRENT.load(Ordering::Relaxed) == DE
}

fn device_language() -> Option<String> {
    sys_locale::get_locale()
}

fn german_table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| de::TEXTS.iter().copied().collect())
}

/// `text` in the current language.
pub fn tr(text: &'static str) -> &'static str {
    if german() {
        german_table().get(text).copied().unwrap_or(text)
    } else {
        text
    }
}

/// `text` in the current language, with each `{name}` replaced by its value.
pub fn trf(text: &'static str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = tr(text).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// "1 task" / "3 tasks" (German: "1 Aufgabe" / "3 Aufgaben"): `one` and `many` are
/// English texts with `{n}` in them, both translated.
pub fn plural(n: usize, one: &'static str, many: &'static str) -> String {
    trf(if n == 1 { one } else { many }, &[("n", &n)])
}

// ------------------------------------------------------------------ dates

const WEEKDAYS_DE: [&str; 7] = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];
const WEEKDAYS_SHORT_DE: [&str; 7] = ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."];
const MONTHS_DE: [&str; 12] =
    ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
const MONTHS_SHORT_DE: [&str; 12] =
    ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."];

fn weekday_index(date: chrono::NaiveDate) -> usize {
    use chrono::Datelike;
    date.weekday().num_days_from_monday() as usize
}

fn month_index(date: chrono::NaiveDate) -> usize {
    use chrono::Datelike;
    date.month0() as usize
}

/// "Wednesday, November 25" / "Mittwoch, 25. November".
pub fn date_long(date: chrono::NaiveDate) -> String {
    date_long_in(date, german())
}

fn date_long_in(date: chrono::NaiveDate, german: bool) -> String {
    use chrono::Datelike;
    if german {
        format!("{}, {}. {}", WEEKDAYS_DE[weekday_index(date)], date.day(), MONTHS_DE[month_index(date)])
    } else {
        date.format("%A, %B %-d").to_string()
    }
}

/// "Friday" / "Freitag".
pub fn weekday(date: chrono::NaiveDate) -> String {
    if german() {
        WEEKDAYS_DE[weekday_index(date)].to_string()
    } else {
        date.format("%A").to_string()
    }
}

/// "Mon, Dec 1" / "Mo., 1. Dez." (with the year: "Mon, Dec 1, 2027" / "Mo., 1. Dez. 2027").
pub fn date_short(date: chrono::NaiveDate, with_year: bool) -> String {
    date_short_in(date, with_year, german())
}

fn date_short_in(date: chrono::NaiveDate, with_year: bool, german: bool) -> String {
    use chrono::Datelike;
    if german {
        let mut s = format!("{}, {}. {}", WEEKDAYS_SHORT_DE[weekday_index(date)], date.day(), MONTHS_SHORT_DE[month_index(date)]);
        if with_year {
            s.push_str(&format!(" {}", date.year()));
        }
        s
    } else if with_year {
        date.format("%a, %b %-d, %Y").to_string()
    } else {
        date.format("%a, %b %-d").to_string()
    }
}

/// "Nov 25, 2026" / "25. Nov. 2026".
pub fn date_medium(date: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    if german() {
        format!("{}. {} {}", date.day(), MONTHS_SHORT_DE[month_index(date)], date.year())
    } else {
        date.format("%b %-d, %Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every `tr("…")`, `trf("…"` and `plural(…, "…", "…")` text in the source.
    fn texts_in_source() -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![];
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
        for path in files {
            let src = std::fs::read_to_string(&path).unwrap();
            // The tests' own example strings aren't app text.
            let src = src.split("#[cfg(test)]").next().unwrap_or_default().to_string();
            for marker in ["tr(", "trf(", "tr_label("] {
                let mut rest = src.as_str();
                while let Some(i) = rest.find(marker) {
                    // Not part of a longer name (e.g. `attr(`).
                    let before = rest[..i].chars().last();
                    rest = &rest[i + marker.len()..];
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    // The text may start on the next line.
                    let after = rest.trim_start();
                    if let Some(lit) = after.strip_prefix('"') {
                        if let Some(text) = read_literal(lit) {
                            out.insert(text);
                        }
                    }
                }
            }
            // plural(n, "one", "many"): the next two literals (not in this file, which
            // defines it and talks about it).
            if path.ends_with("i18n.rs") {
                continue;
            }
            let mut rest = src.as_str();
            while let Some(i) = rest.find("plural(") {
                rest = &rest[i + 7..];
                let mut r = rest;
                for _ in 0..2 {
                    let Some(q) = r.find('"') else { break };
                    r = &r[q + 1..];
                    let len = literal_len(r);
                    if let Some(text) = read_literal(r) {
                        out.insert(text);
                    }
                    r = &r[(len + 1).min(r.len())..];
                }
            }
        }
        out.remove("");
        out
    }

    /// The string literal starting right after an opening quote, unescaped.
    fn read_literal(s: &str) -> Option<String> {
        let len = literal_len(s);
        let raw = &s[..len];
        Some(raw.replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\"))
    }

    fn literal_len(s: &str) -> usize {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => return i,
                _ => i += 1,
            }
        }
        bytes.len()
    }

    #[test]
    fn every_text_has_a_german_version() {
        let table = german_table();
        let missing: Vec<String> = texts_in_source().into_iter().filter(|t| !table.contains_key(t.as_str())).collect();
        assert!(missing.is_empty(), "{} texts have no German version in src/i18n/de.rs:\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn german_keeps_placeholders() {
        for (en, de) in de::TEXTS {
            let names = |s: &str| {
                let mut v: Vec<String> = s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n.to_string())).collect();
                v.sort();
                v
            };
            assert_eq!(names(en), names(de), "placeholders differ for {en:?}");
        }
    }

    #[test]
    fn no_duplicate_or_unused_entries() {
        let used = texts_in_source();
        let mut seen = BTreeSet::new();
        for (en, _) in de::TEXTS {
            assert!(seen.insert(*en), "listed twice in de.rs: {en:?}");
        }
        let unused: Vec<&&str> = seen.iter().filter(|t| !used.contains(**t)).collect();
        assert!(unused.is_empty(), "no longer used, remove from de.rs: {unused:?}");
    }

    #[test]
    fn dates_in_german() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(date_long_in(d, true), "Mittwoch, 25. November");
        assert_eq!(date_short_in(d, false, true), "Mi., 25. Nov.");
        assert_eq!(date_short_in(d, true, true), "Mi., 25. Nov. 2026");
        assert_eq!(date_long_in(d, false), "Wednesday, November 25");
    }
}
