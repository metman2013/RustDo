//! The app's languages: English (built in), German (`de.rs`), Spanish (`es.rs`) and
//! French (`fr.rs`).
//!
//! Every piece of text people see goes through [`tr`] (or [`trf`] when it has values in
//! it), with the English text itself as the key. Other languages come from their tables;
//! anything missing there shows in English. A test checks that every `tr`/`trf` text in
//! the source is in every table, so new features can't ship half-translated.
//!
//! The language is the device's (Settings › Language › Automatic) unless chosen.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

mod de;
mod es;
mod fr;

/// Settings › Language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    /// The device's language, if RustDo has it; else English.
    #[default]
    Automatic,
    English,
    German,
    Spanish,
    French,
}

impl Language {
    pub const ALL: [Language; 5] = [Self::Automatic, Self::English, Self::German, Self::Spanish, Self::French];

    /// Each language in its own words (so it can be found whatever is showing).
    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => tr("Automatic"),
            Self::English => "English",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::French => "Français",
        }
    }
}

/// The language showing (never "automatic").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    En = 0,
    De = 1,
    Es = 2,
    Fr = 3,
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

/// Switches the app's language (at start-up and when the setting changes).
pub fn set(language: Language) {
    let lang = match language {
        Language::English => Lang::En,
        Language::German => Lang::De,
        Language::Spanish => Lang::Es,
        Language::French => Lang::Fr,
        Language::Automatic => device_language().map_or(Lang::En, |l| from_locale(&l)),
    };
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// "de-AT", "es_MX", "fr-CA"… → the language RustDo has for it.
fn from_locale(locale: &str) -> Lang {
    match locale.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("de") => Lang::De,
        Some("es") => Lang::Es,
        Some("fr") => Lang::Fr,
        _ => Lang::En,
    }
}

fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::De,
        2 => Lang::Es,
        3 => Lang::Fr,
        _ => Lang::En,
    }
}

/// Whether people reading the current language expect a 24-hour clock (German, Spanish,
/// French). Used for a new install's default; the setting can still be changed.
pub fn prefers_24h() -> bool {
    current() != Lang::En
}

/// The default list's name in every language RustDo has ("Tasks", "Aufgaben"…), to tell
/// whether a list still has its default name.
pub fn default_list_names() -> Vec<&'static str> {
    let mut names = vec!["Tasks"];
    for lang in [Lang::De, Lang::Es, Lang::Fr] {
        if let Some(name) = table(lang).and_then(|t| t.get("Tasks").copied()) {
            names.push(name);
        }
    }
    names
}

/// The language showing, as a code: "en", "de", "es" or "fr".
pub fn code() -> &'static str {
    match current() {
        Lang::En => "en",
        Lang::De => "de",
        Lang::Es => "es",
        Lang::Fr => "fr",
    }
}

/// True while German is showing.
pub fn german() -> bool {
    current() == Lang::De
}

fn device_language() -> Option<String> {
    sys_locale::get_locale()
}

fn table(lang: Lang) -> Option<&'static HashMap<&'static str, &'static str>> {
    static DE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    static ES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    static FR: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    match lang {
        Lang::En => None,
        Lang::De => Some(DE.get_or_init(|| de::TEXTS.iter().copied().collect())),
        Lang::Es => Some(ES.get_or_init(|| es::TEXTS.iter().copied().collect())),
        Lang::Fr => Some(FR.get_or_init(|| fr::TEXTS.iter().copied().collect())),
    }
}

/// `text` in the current language.
pub fn tr(text: &'static str) -> &'static str {
    table(current()).and_then(|t| t.get(text).copied()).unwrap_or(text)
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

struct DateWords {
    weekdays: [&'static str; 7],
    weekdays_short: [&'static str; 7],
    months: [&'static str; 12],
    months_short: [&'static str; 12],
}

const DE_DATES: DateWords = DateWords {
    weekdays: ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"],
    weekdays_short: ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."],
    months: ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
    months_short: ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."],
};

const ES_DATES: DateWords = DateWords {
    weekdays: ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"],
    weekdays_short: ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
    months: ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
    months_short: ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"],
};

const FR_DATES: DateWords = DateWords {
    weekdays: ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"],
    weekdays_short: ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."],
    months: ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
    months_short: ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
};

fn date_words(lang: Lang) -> Option<&'static DateWords> {
    match lang {
        Lang::En => None,
        Lang::De => Some(&DE_DATES),
        Lang::Es => Some(&ES_DATES),
        Lang::Fr => Some(&FR_DATES),
    }
}

/// Capitalizes the first letter (Spanish and French write weekdays in lower case, except
/// at the start of a line).
fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

fn parts(date: chrono::NaiveDate) -> (usize, u32, usize, i32) {
    use chrono::Datelike;
    (date.weekday().num_days_from_monday() as usize, date.day(), date.month0() as usize, date.year())
}

/// "Wednesday, November 25" / "Mittwoch, 25. November" / "Miércoles, 25 de noviembre" /
/// "Mercredi 25 novembre".
pub fn date_long(date: chrono::NaiveDate) -> String {
    date_long_in(date, current())
}

fn date_long_in(date: chrono::NaiveDate, lang: Lang) -> String {
    let (wd, day, m, _) = parts(date);
    let Some(w) = date_words(lang) else { return date.format("%A, %B %-d").to_string() };
    match lang {
        Lang::De => format!("{}, {day}. {}", w.weekdays[wd], w.months[m]),
        Lang::Es => capitalized(&format!("{}, {day} de {}", w.weekdays[wd], w.months[m])),
        _ => capitalized(&format!("{} {day} {}", w.weekdays[wd], w.months[m])),
    }
}

/// "Friday" / "Freitag" / "Viernes" / "Vendredi".
pub fn weekday(date: chrono::NaiveDate) -> String {
    let (wd, ..) = parts(date);
    match date_words(current()) {
        Some(w) => capitalized(w.weekdays[wd]),
        None => date.format("%A").to_string(),
    }
}

/// "Mon, Dec 1" / "Mo., 1. Dez." / "Lun, 1 dic" / "Lun. 1 déc." (optionally with the year).
pub fn date_short(date: chrono::NaiveDate, with_year: bool) -> String {
    date_short_in(date, with_year, current())
}

fn date_short_in(date: chrono::NaiveDate, with_year: bool, lang: Lang) -> String {
    let (wd, day, m, year) = parts(date);
    let Some(w) = date_words(lang) else {
        return date.format(if with_year { "%a, %b %-d, %Y" } else { "%a, %b %-d" }).to_string();
    };
    let mut s = match lang {
        Lang::De => format!("{}, {day}. {}", w.weekdays_short[wd], w.months_short[m]),
        Lang::Es => capitalized(&format!("{}, {day} {}", w.weekdays_short[wd], w.months_short[m])),
        _ => capitalized(&format!("{} {day} {}", w.weekdays_short[wd], w.months_short[m])),
    };
    if with_year {
        s.push_str(&format!(" {year}"));
    }
    s
}

/// "Nov 25, 2026" / "25. Nov. 2026" / "25 nov 2026" / "25 nov. 2026".
pub fn date_medium(date: chrono::NaiveDate) -> String {
    let (_, day, m, year) = parts(date);
    match (current(), date_words(current())) {
        (Lang::De, Some(w)) => format!("{day}. {} {year}", w.months_short[m]),
        (_, Some(w)) => format!("{day} {} {year}", w.months_short[m]),
        _ => date.format("%b %-d, %Y").to_string(),
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

    const TABLES: [(&str, &[(&str, &str)]); 3] = [("de", de::TEXTS), ("es", es::TEXTS), ("fr", fr::TEXTS)];

    #[test]
    fn every_text_is_translated() {
        let used = texts_in_source();
        for (code, texts) in TABLES {
            let table: HashMap<&str, &str> = texts.iter().copied().collect();
            let missing: Vec<&String> = used.iter().filter(|t| !table.contains_key(t.as_str())).collect();
            assert!(
                missing.is_empty(),
                "{} texts have no translation in src/i18n/{code}.rs:\n{}",
                missing.len(),
                missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
            );
        }
    }

    #[test]
    fn translations_keep_placeholders() {
        for (en, de) in TABLES.iter().flat_map(|(_, t)| t.iter()) {
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
        for (code, texts) in TABLES {
            let mut seen = BTreeSet::new();
            for (en, _) in texts {
                assert!(seen.insert(*en), "listed twice in {code}.rs: {en:?}");
            }
            let unused: Vec<&&str> = seen.iter().filter(|t| !used.contains(**t)).collect();
            assert!(unused.is_empty(), "no longer used, remove from {code}.rs: {unused:?}");
        }
    }

    #[test]
    fn dates_in_each_language() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(date_long_in(d, Lang::De), "Mittwoch, 25. November");
        assert_eq!(date_short_in(d, false, Lang::De), "Mi., 25. Nov.");
        assert_eq!(date_short_in(d, true, Lang::De), "Mi., 25. Nov. 2026");
        assert_eq!(date_long_in(d, Lang::Es), "Miércoles, 25 de noviembre");
        assert_eq!(date_short_in(d, false, Lang::Es), "Mié, 25 nov");
        assert_eq!(date_long_in(d, Lang::Fr), "Mercredi 25 novembre");
        assert_eq!(date_short_in(d, false, Lang::Fr), "Mer. 25 nov.");
        assert_eq!(date_long_in(d, Lang::En), "Wednesday, November 25");
    }

    #[test]
    fn device_languages() {
        assert_eq!(from_locale("de-AT"), Lang::De);
        assert_eq!(from_locale("es_MX"), Lang::Es);
        assert_eq!(from_locale("fr-CA"), Lang::Fr);
        assert_eq!(from_locale("en-US"), Lang::En);
        assert_eq!(from_locale("pt-BR"), Lang::En);
    }
}
