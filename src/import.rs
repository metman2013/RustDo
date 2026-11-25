//! Importing tasks from Microsoft To Do.
//!
//! Supported inputs:
//! * **Microsoft Graph JSON**, as written by `tools/Export-MicrosoftToDo.ps1`
//!   (`{"lists": [{displayName, wellknownListName, tasks: [...]}]}`), or a single
//!   Graph Explorer response for one list's tasks (`{"value": [task, ...]}`).
//! * **Outlook CSV**, from classic Outlook's export of a Tasks folder (English column names).

use crate::i18n::{plural, tr, trf};
use crate::model::{Recurrence, Step, Store, Task};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedTask {
    pub source_id: String,
    pub title: String,
    pub notes: String,
    pub done: bool,
    pub important: bool,
    pub due: Option<NaiveDate>,
    pub reminder: Option<NaiveDateTime>,
    pub recurrence: Recurrence,
    pub steps: Vec<(String, bool)>,
    pub created: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Default)]
pub struct ImportedList {
    pub name: String,
    /// Microsoft To Do's built-in "Tasks" list; merged into RustDo's default list.
    pub is_default: bool,
    pub tasks: Vec<ImportedTask>,
}

#[derive(Debug, Clone, Default)]
pub struct Import {
    pub source: &'static str,
    pub lists: Vec<ImportedList>,
    pub warnings: Vec<String>,
}

impl Import {
    pub fn task_count(&self) -> usize {
        self.lists.iter().map(|l| l.tasks.len()).sum()
    }

    pub fn completed_count(&self) -> usize {
        self.lists.iter().flat_map(|l| &l.tasks).filter(|t| t.done).count()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub lists_created: usize,
    pub tasks_added: usize,
    pub duplicates_skipped: usize,
    pub completed_skipped: usize,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        if self.tasks_added == 0 && self.duplicates_skipped > 0 {
            return trf("Nothing new to import: all {n} tasks were imported before.", &[("n", &self.duplicates_skipped)]);
        }
        let mut s = match self.lists_created {
            0 => plural(self.tasks_added, "Imported {n} task.", "Imported {n} tasks."),
            lists => trf("Imported {tasks} tasks into {lists} new lists.", &[("tasks", &self.tasks_added), ("lists", &lists)]),
        };
        if self.duplicates_skipped > 0 {
            s.push(' ');
            s += &trf("Skipped {n} already imported.", &[("n", &self.duplicates_skipped)]);
        }
        s
    }
}

/// Reads and parses an export file, picking the format from the extension/content.
pub fn parse_file(path: &Path) -> Result<Import, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    parse_bytes(&name, &bytes)
}

/// Parses an export given its file name (used for the format and list name) and contents.
/// The web app uses this directly, since browsers hand over file contents, not paths.
pub fn parse_bytes(file_name: &str, bytes: &[u8]) -> Result<Import, String> {
    let text = decode_text(bytes);
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| tr("Imported").into());
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "csv" || (ext != "json" && !text.trim_start().starts_with(['{', '['])) {
        parse_outlook_csv(&text, &stem)
    } else {
        parse_graph_json(&text, &stem, utc_to_local)
    }
}

/// UTF-8 (with or without BOM), falling back to Windows-1252, which older Outlook uses.
fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
    }
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
        '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

fn utc_to_local(n: NaiveDateTime) -> NaiveDateTime {
    Local.from_utc_datetime(&n).naive_local()
}

// ---------------------------------------------------------------------- Graph JSON

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

pub fn parse_graph_json(
    text: &str,
    fallback_name: &str,
    to_local: fn(NaiveDateTime) -> NaiveDateTime,
) -> Result<Import, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| trf("This file isn't valid JSON ({e}).", &[("e", &e)]))?;
    let items = v
        .get("lists")
        .or_else(|| v.get("value"))
        .unwrap_or(&v)
        .as_array()
        .ok_or(tr("This file doesn't look like a Microsoft To Do export."))?;

    let mut import = Import { source: "Microsoft To Do", ..Default::default() };
    let is_task = |x: &Value| x.get("title").is_some();

    if items.iter().any(is_task) {
        // A bare collection of tasks, e.g. one Graph Explorer response. Name the list after the file.
        let tasks = items
            .iter()
            .filter(|x| is_task(x))
            .map(|t| graph_task(t, to_local, &mut import.warnings))
            .collect();
        import.lists.push(ImportedList {
            name: fallback_name.to_string(),
            is_default: fallback_name.eq_ignore_ascii_case("tasks"),
            tasks,
        });
        return Ok(import);
    }

    let mut lists_without_tasks = 0;
    for l in items {
        let name = s(l, "displayName").unwrap_or(tr("Imported")).trim().to_string();
        let Some(tasks) = l.get("tasks").and_then(Value::as_array) else {
            lists_without_tasks += 1;
            continue;
        };
        let tasks = tasks.iter().map(|t| graph_task(t, to_local, &mut import.warnings)).collect();
        import.lists.push(ImportedList {
            is_default: s(l, "wellknownListName") == Some("defaultList"),
            name,
            tasks,
        });
    }
    if import.lists.is_empty() {
        return Err(if lists_without_tasks > 0 {
            tr("This file only has list names, not tasks. Export with tools/Export-MicrosoftToDo.ps1 instead.").into()
        } else {
            tr("No lists or tasks found in this file.").into()
        });
    }
    if lists_without_tasks > 0 {
        import.warnings.push(trf("{lists_without_tasks} list(s) in the file had no task data and were skipped.", &[("lists_without_tasks", &lists_without_tasks)]));
    }
    Ok(import)
}

/// Graph `dateTimeTimeZone` → local time. Graph returns UTC unless asked otherwise.
fn graph_dt(v: Option<&Value>, to_local: fn(NaiveDateTime) -> NaiveDateTime) -> Option<NaiveDateTime> {
    let v = v?;
    let raw = s(v, "dateTime")?;
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    let tz = s(v, "timeZone").unwrap_or("UTC");
    Some(if matches!(tz, "UTC" | "Etc/UTC" | "Coordinated Universal Time") { to_local(naive) } else { naive })
}

/// Due dates in To Do are calendar days, not instants. Graph reports them either as
/// midnight on that day (keep the date as-is) or as the user's local midnight expressed
/// in UTC, e.g. `05:00Z` for New York (convert to local time first).
fn graph_due_date(v: Option<&Value>, to_local: fn(NaiveDateTime) -> NaiveDateTime) -> Option<NaiveDate> {
    let raw = graph_dt(v, |n| n)?;
    if raw.time() == NaiveTime::MIN {
        Some(raw.date())
    } else {
        graph_dt(v, to_local).map(|d| d.date())
    }
}

fn graph_task(t: &Value, to_local: fn(NaiveDateTime) -> NaiveDateTime, warnings: &mut Vec<String>) -> ImportedTask {
    let title = s(t, "title").map(str::trim).filter(|x| !x.is_empty()).unwrap_or("(untitled)").to_string();
    let done = s(t, "status") == Some("completed");
    let reminder_on = t.get("isReminderOn").and_then(Value::as_bool).unwrap_or(false);
    let created = s(t, "createdDateTime")
        .and_then(|x| DateTime::parse_from_rfc3339(x).ok())
        .map(|d| to_local(d.naive_utc()));
    let steps = t
        .get("checklistItems")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|c| {
                    let name = s(c, "displayName")?.trim();
                    (!name.is_empty()).then(|| (name.to_string(), c.get("isChecked").and_then(Value::as_bool).unwrap_or(false)))
                })
                .collect()
        })
        .unwrap_or_default();
    let recurrence = t
        .get("recurrence")
        .filter(|r| !r.is_null())
        .map(|r| graph_recurrence(r, &title, warnings))
        .unwrap_or_default();
    ImportedTask {
        source_id: match s(t, "id") {
            Some(id) => format!("mstodo:{id}"),
            None => format!("mstodo-title:{title}"),
        },
        notes: t.get("body").map(body_text).unwrap_or_default(),
        done,
        important: s(t, "importance") == Some("high"),
        due: graph_due_date(t.get("dueDateTime"), to_local),
        reminder: if reminder_on { graph_dt(t.get("reminderDateTime"), to_local) } else { None },
        recurrence,
        steps,
        created,
        completed_at: graph_dt(t.get("completedDateTime"), to_local),
        title,
    }
}

fn graph_recurrence(r: &Value, title: &str, warnings: &mut Vec<String>) -> Recurrence {
    let Some(p) = r.get("pattern") else { return Recurrence::Never };
    let interval = p.get("interval").and_then(Value::as_u64).unwrap_or(1);
    let days: Vec<String> = p
        .get("daysOfWeek")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_lowercase).collect())
        .unwrap_or_default();
    let rec = match s(p, "type").unwrap_or("") {
        "daily" => Recurrence::Daily,
        "weekly" => {
            let weekend = days.iter().any(|d| d == "saturday" || d == "sunday");
            if days.len() == 5 && !weekend {
                Recurrence::Weekdays
            } else {
                if days.len() > 1 {
                    warnings.push(trf("“{title}” repeats on several weekdays; it will repeat weekly instead.", &[("title", &title)]));
                }
                Recurrence::Weekly
            }
        }
        "absoluteMonthly" | "relativeMonthly" => Recurrence::Monthly,
        "absoluteYearly" | "relativeYearly" => Recurrence::Yearly,
        other => {
            warnings.push(trf("“{title}” has an unsupported repeat pattern ({other}); it won't repeat.", &[("title", &title), ("other", &other)]));
            return Recurrence::Never;
        }
    };
    if interval > 1 && rec != Recurrence::Weekdays {
        warnings.push(trf(
            "“{title}” repeats every {interval} periods; RustDo will repeat it {repeat}.",
            &[("title", &title), ("interval", &interval), ("repeat", &rec.label().to_lowercase())],
        ));
    }
    rec
}

fn body_text(body: &Value) -> String {
    let content = s(body, "content").unwrap_or("");
    if s(body, "contentType").is_some_and(|t| t.eq_ignore_ascii_case("html")) {
        strip_html(content)
    } else {
        content.trim().to_string()
    }
}

fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    let mut skipping: Option<&str> = None; // inside <head>/<style>/<script>
    for c in html.chars() {
        if in_tag {
            if c != '>' {
                tag.push(c);
                continue;
            }
            in_tag = false;
            let lower = tag.trim().to_ascii_lowercase();
            let closing = lower.starts_with('/');
            let name: String = lower.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            tag.clear();
            match skipping {
                Some(until) => {
                    if closing && name == until {
                        skipping = None;
                    }
                }
                None => {
                    if !closing && ["head", "style", "script"].contains(&name.as_str()) {
                        skipping = Some(match name.as_str() { "head" => "head", "style" => "style", _ => "script" });
                    } else if name == "br" || (closing && matches!(name.as_str(), "p" | "div" | "li" | "tr")) {
                        out.push('\n');
                    }
                }
            }
        } else if c == '<' {
            in_tag = true;
        } else if skipping.is_none() {
            out.push(c);
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    // Trim each line and collapse runs of blank lines.
    let mut lines: Vec<&str> = Vec::new();
    for line in decoded.lines().map(str::trim) {
        if line.is_empty() && lines.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

// ---------------------------------------------------------------------- Outlook CSV

pub fn parse_outlook_csv(text: &str, list_name: &str) -> Result<Import, String> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let headers = rdr.headers().map_err(|e| trf("Couldn't read the CSV ({e}).", &[("e", &e)]))?.clone();
    let col = |name: &str| headers.iter().position(|h| h.trim().eq_ignore_ascii_case(name));
    let subject = col("Subject").ok_or(
        tr("This CSV has no “Subject” column. RustDo reads Microsoft To Do exports (JSON) and English task exports from classic Outlook (CSV)."),
    )?;
    let (due_c, rem_on_c, rem_date_c, rem_time_c) =
        (col("Due Date"), col("Reminder On/Off"), col("Reminder Date"), col("Reminder Time"));
    let (done_c, pct_c, notes_c, prio_c, status_c) =
        (col("Date Completed"), col("% Complete"), col("Notes"), col("Priority"), col("Status"));

    let records: Vec<csv::StringRecord> =
        rdr.records().collect::<Result<_, _>>().map_err(|e| trf("Couldn't read the CSV ({e}).", &[("e", &e)]))?;

    let get = |r: &csv::StringRecord, c: Option<usize>| -> Option<String> {
        c.and_then(|c| r.get(c)).map(str::trim).filter(|x| !x.is_empty()).map(str::to_string)
    };
    // Outlook writes dates in the PC's locale. If any slash date starts with a number > 12, it's day-first.
    let day_first = records.iter().any(|r| {
        [due_c, rem_date_c, done_c].iter().filter_map(|c| get(r, *c)).any(|d| {
            let parts: Vec<&str> = d.split_whitespace().next().unwrap_or("").split('/').collect();
            parts.len() == 3 && parts[0].parse::<u32>().is_ok_and(|n| n > 12)
        })
    });

    let mut tasks = Vec::new();
    for r in &records {
        let Some(title) = get(r, Some(subject)) else { continue };
        let due = get(r, due_c).and_then(|d| parse_date(&d, day_first));
        let reminder = if get(r, rem_on_c).is_some_and(|v| truthy(&v)) {
            get(r, rem_date_c).and_then(|d| parse_date(&d, day_first)).map(|d| {
                let t = get(r, rem_time_c).and_then(|t| parse_time(&t));
                d.and_time(t.unwrap_or(NaiveTime::from_hms_opt(9, 0, 0).unwrap()))
            })
        } else {
            None
        };
        let completed_at = get(r, done_c).and_then(|d| parse_date(&d, day_first)).and_then(|d| d.and_hms_opt(0, 0, 0));
        let done = completed_at.is_some()
            || get(r, status_c).is_some_and(|v| v.eq_ignore_ascii_case("completed"))
            || get(r, pct_c).is_some_and(|v| v.trim_end_matches('%').trim() == "100");
        tasks.push(ImportedTask {
            source_id: format!("outlook:{list_name}|{title}|{}", due.map(|d| d.to_string()).unwrap_or_default()),
            notes: get(r, notes_c).unwrap_or_default(),
            done,
            important: get(r, prio_c).is_some_and(|p| p.eq_ignore_ascii_case("high")),
            due,
            reminder,
            completed_at,
            title,
            ..Default::default()
        });
    }
    Ok(Import {
        source: "Outlook CSV",
        lists: vec![ImportedList {
            name: list_name.to_string(),
            is_default: list_name.eq_ignore_ascii_case("tasks"),
            tasks,
        }],
        warnings: Vec::new(),
    })
}

fn truthy(v: &str) -> bool {
    matches!(v.to_ascii_lowercase().as_str(), "true" | "yes" | "on" | "1")
}

fn parse_date(raw: &str, day_first: bool) -> Option<NaiveDate> {
    let d = raw.split_whitespace().next()?;
    let slash: &[&str] = if day_first { &["%d/%m/%Y", "%d/%m/%y"] } else { &["%m/%d/%Y", "%m/%d/%y"] };
    ["%Y-%m-%d"]
        .iter()
        .chain(slash)
        .chain(&["%d.%m.%Y", "%d-%m-%Y"])
        .find_map(|f| NaiveDate::parse_from_str(d, f).ok())
        // Outlook uses 1/1/4501 to mean "none".
        .filter(|d| (1900..4000).contains(&chrono::Datelike::year(d)))
}

fn parse_time(raw: &str) -> Option<NaiveTime> {
    ["%I:%M:%S %p", "%I:%M %p", "%H:%M:%S", "%H:%M"]
        .iter()
        .find_map(|f| NaiveTime::parse_from_str(raw.trim(), f).ok())
}

// ---------------------------------------------------------------------- applying

/// Adds the imported lists and tasks to `store`. Lists with the same name are merged,
/// and tasks imported before (same source id) are skipped.
pub fn apply(store: &mut Store, import: &Import, include_completed: bool, now: NaiveDateTime) -> ImportReport {
    let mut report = ImportReport::default();
    let mut seen: HashSet<String> = store.tasks.iter().filter_map(|t| t.source_id.clone()).collect();

    for list in &import.lists {
        let name = if list.name.trim().is_empty() { tr("Imported") } else { list.name.trim() };
        let list_id = if list.is_default {
            store.default_list
        } else if let Some(l) = store.lists.iter().find(|l| l.name.eq_ignore_ascii_case(name)) {
            l.id
        } else {
            report.lists_created += 1;
            store.add_list(name).unwrap_or(store.default_list)
        };

        for t in &list.tasks {
            if t.done && !include_completed {
                report.completed_skipped += 1;
                continue;
            }
            if !seen.insert(t.source_id.clone()) {
                report.duplicates_skipped += 1;
                continue;
            }
            let mut task = Task::new(t.title.clone(), list_id, t.created.unwrap_or(now));
            task.notes = t.notes.clone();
            task.done = t.done;
            task.important = t.important;
            task.due = t.due;
            task.reminder = t.reminder;
            // Don't pop up reminders that are already in the past.
            task.reminder_fired = t.done || t.reminder.is_some_and(|r| r <= now);
            task.recurrence = if t.done { Recurrence::Never } else { t.recurrence };
            if task.recurrence != Recurrence::Never && task.planned_date().is_none() {
                task.due = Some(now.date());
            }
            task.completed_at = if t.done { Some(t.completed_at.unwrap_or(now)) } else { None };
            task.steps = t
                .steps
                .iter()
                .map(|(title, done)| {
                    let mut s = Step::new(title.clone());
                    s.done = *done;
                    s
                })
                .collect();
            task.source_id = Some(t.source_id.clone());
            store.tasks.push(task);
            report.tasks_added += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::View;

    fn ident(n: NaiveDateTime) -> NaiveDateTime {
        n
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    const GRAPH: &str = r#"{
      "source": "microsoft-graph",
      "lists": [
        { "id": "L1", "displayName": "Tasks", "wellknownListName": "defaultList", "tasks": [
          { "id": "T1", "title": "Pay rent", "status": "notStarted", "importance": "high",
            "isReminderOn": true,
            "reminderDateTime": { "dateTime": "2026-12-01T08:00:00.0000000", "timeZone": "UTC" },
            "dueDateTime": { "dateTime": "2026-12-01T00:00:00.0000000", "timeZone": "UTC" },
            "createdDateTime": "2026-11-01T10:20:30.1234567Z",
            "body": { "content": "<html><head><style>p{}</style></head><body><p>Bank&nbsp;transfer</p><p>Ref: A&amp;B</p></body></html>", "contentType": "html" },
            "recurrence": { "pattern": { "type": "absoluteMonthly", "interval": 1, "dayOfMonth": 1 }, "range": { "type": "noEnd" } },
            "checklistItems": [ { "displayName": "Check balance", "isChecked": true }, { "displayName": "Send", "isChecked": false } ] },
          { "id": "T2", "title": "Old thing", "status": "completed", "importance": "normal",
            "completedDateTime": { "dateTime": "2026-10-05T00:00:00.0000000", "timeZone": "UTC" },
            "body": { "content": "", "contentType": "text" } }
        ]},
        { "id": "L2", "displayName": "Groceries", "wellknownListName": "none", "tasks": [
          { "id": "T3", "title": "Eggs", "status": "notStarted", "importance": "normal",
            "isReminderOn": false,
            "recurrence": { "pattern": { "type": "weekly", "interval": 1, "daysOfWeek": ["monday","tuesday","wednesday","thursday","friday"] } } }
        ]}
      ]
    }"#;

    #[test]
    fn parses_graph_export() {
        let imp = parse_graph_json(GRAPH, "x", ident).unwrap();
        assert_eq!(imp.lists.len(), 2);
        assert!(imp.lists[0].is_default);
        let rent = &imp.lists[0].tasks[0];
        assert_eq!(rent.title, "Pay rent");
        assert!(rent.important && !rent.done);
        assert_eq!(rent.due, Some(d(2026, 12, 1)));
        assert_eq!(rent.reminder, Some(d(2026, 12, 1).and_hms_opt(8, 0, 0).unwrap()));
        assert_eq!(rent.recurrence, Recurrence::Monthly);
        assert_eq!(rent.notes, "Bank transfer\nRef: A&B");
        assert_eq!(rent.steps, vec![("Check balance".into(), true), ("Send".into(), false)]);
        assert_eq!(rent.created, Some(d(2026, 11, 1).and_hms_opt(10, 20, 30).unwrap() + chrono::Duration::nanoseconds(123_456_700)));
        assert!(imp.lists[0].tasks[1].done);
        assert_eq!(imp.lists[1].tasks[0].recurrence, Recurrence::Weekdays);
        assert_eq!(imp.lists[1].tasks[0].reminder, None);
        assert_eq!(imp.task_count(), 3);
        assert_eq!(imp.completed_count(), 1);
    }

    #[test]
    fn due_dates_are_calendar_days() {
        fn new_york(n: NaiveDateTime) -> NaiveDateTime {
            n - chrono::Duration::hours(5)
        }
        fn berlin(n: NaiveDateTime) -> NaiveDateTime {
            n + chrono::Duration::hours(1)
        }
        let due = |dt: &str| serde_json::json!({ "dateTime": dt, "timeZone": "UTC" });
        // Midnight UTC means "that day" for everyone.
        assert_eq!(graph_due_date(Some(&due("2026-12-01T00:00:00.0000000")), new_york), Some(d(2026, 12, 1)));
        // Local midnight sent as UTC instants.
        assert_eq!(graph_due_date(Some(&due("2026-12-01T05:00:00.0000000")), new_york), Some(d(2026, 12, 1)));
        assert_eq!(graph_due_date(Some(&due("2026-11-30T23:00:00.0000000")), berlin), Some(d(2026, 12, 1)));
    }

    #[test]
    fn parses_graph_explorer_response_for_one_list() {
        let json = r#"{"@odata.context":"x","value":[{"id":"A","title":"Read book","status":"notStarted","importance":"normal"}]}"#;
        let imp = parse_graph_json(json, "Reading", ident).unwrap();
        assert_eq!(imp.lists.len(), 1);
        assert_eq!(imp.lists[0].name, "Reading");
        assert_eq!(imp.lists[0].tasks[0].title, "Read book");
    }

    #[test]
    fn lists_only_file_gives_helpful_error() {
        let json = r#"{"value":[{"id":"L1","displayName":"Tasks"}]}"#;
        let err = parse_graph_json(json, "x", ident).unwrap_err();
        assert!(err.contains("Export-MicrosoftToDo.ps1"), "{err}");
    }

    #[test]
    fn apply_merges_and_dedupes() {
        let mut store = Store::new();
        let now = d(2026, 11, 19).and_hms_opt(9, 0, 0).unwrap();
        store.add_list("groceries");
        let imp = parse_graph_json(GRAPH, "x", ident).unwrap();

        let r = apply(&mut store, &imp, false, now);
        assert_eq!(r, ImportReport { lists_created: 0, tasks_added: 2, duplicates_skipped: 0, completed_skipped: 1 });
        assert_eq!(store.lists.len(), 2, "Groceries merged case-insensitively, Tasks into default");
        let rent = store.tasks.iter().find(|t| t.title == "Pay rent").unwrap();
        assert_eq!(rent.list_id, store.default_list);
        assert!(!rent.reminder_fired, "future reminder still pending");
        assert_eq!(rent.steps.len(), 2);
        assert_eq!(store.open_count(View::Important, now.date()), 1);

        // Importing the same file again adds nothing.
        let again = apply(&mut store, &imp, true, now);
        assert_eq!(again.tasks_added, 1, "only the previously-skipped completed task");
        assert_eq!(again.duplicates_skipped, 2);
        let old = store.tasks.iter().find(|t| t.title == "Old thing").unwrap();
        assert!(old.done && old.completed_at.is_some());
    }

    #[test]
    fn past_reminders_do_not_fire_after_import() {
        let mut store = Store::new();
        let now = d(2027, 1, 1).and_hms_opt(9, 0, 0).unwrap();
        let imp = parse_graph_json(GRAPH, "x", ident).unwrap();
        apply(&mut store, &imp, false, now);
        assert!(store.fire_due_reminders(now).is_empty());
    }

    #[test]
    fn parses_outlook_csv() {
        let csv = "\"Subject\",\"Start Date\",\"Due Date\",\"Reminder On/Off\",\"Reminder Date\",\"Reminder Time\",\"Date Completed\",\"% Complete\",\"Notes\",\"Priority\",\"Status\"\n\
                   \"Renew passport\",\"\",\"25/11/2026\",\"True\",\"24/11/2026\",\"9:30:00 AM\",\"\",\"0%\",\"Bring photos\",\"High\",\"Not Started\"\n\
                   \"Fix bike\",\"\",\"\",\"False\",\"\",\"\",\"3/11/2026\",\"100%\",\"\",\"Normal\",\"Completed\"\n";
        let imp = parse_outlook_csv(csv, "Personal").unwrap();
        let t = &imp.lists[0].tasks;
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].due, Some(d(2026, 11, 25)), "day-first detected from 25/11");
        assert_eq!(t[0].reminder, Some(d(2026, 11, 24).and_hms_opt(9, 30, 0).unwrap()));
        assert!(t[0].important);
        assert_eq!(t[0].notes, "Bring photos");
        assert!(t[1].done);
        assert_eq!(t[1].completed_at.map(|c| c.date()), Some(d(2026, 11, 3)));
    }

    #[test]
    fn outlook_us_dates_and_sentinel() {
        let csv = "Subject,Due Date,Date Completed\nA,11/19/2026,1/1/4501\n";
        let imp = parse_outlook_csv(csv, "Tasks").unwrap();
        assert!(imp.lists[0].is_default);
        assert_eq!(imp.lists[0].tasks[0].due, Some(d(2026, 11, 19)));
        assert!(!imp.lists[0].tasks[0].done);
    }

    #[test]
    fn csv_without_subject_is_rejected() {
        assert!(parse_outlook_csv("Betreff,Fällig\nA,B\n", "x").is_err());
    }

    #[test]
    fn windows_1252_fallback() {
        assert_eq!(decode_text(&[b'C', b'a', b'f', 0xE9, b' ', 0x80]), "Café €");
        assert_eq!(decode_text("\u{feff}hi".as_bytes()), "hi");
    }
}
