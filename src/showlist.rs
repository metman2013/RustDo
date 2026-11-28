//! "Show list": one list's open tasks, laid out to read or hand over (for example a list of
//! medications to show a doctor), and to copy as plain text or print / save as PDF.
//!
//! [`build`] gathers what to show; the app draws it, [`to_text`] makes the plain text and
//! [`to_html`] the printable page (opened in the browser, whose print dialog can also save a
//! PDF).

use crate::i18n::{tr, trf};
use crate::model::{format_time, relative_date, sort_tasks, Recurrence, Store, View};
use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub title: String,
    /// Due date, repeat and reminder, e.g. ["Due Tomorrow", "Daily", "Remind me 8:00 AM"].
    pub details: Vec<String>,
    pub notes: String,
    pub steps: Vec<(String, bool)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Printable {
    pub title: String,
    /// "As of Thursday, November 26".
    pub date: String,
    pub items: Vec<Item>,
}

/// The open tasks of `view`, in the order the app shows them.
pub fn build(store: &Store, view: View, title: &str, today: NaiveDate) -> Printable {
    let mut tasks: Vec<_> = store.tasks.iter().filter(|t| !t.done && store.view_contains(view, t, today)).collect();
    sort_tasks(&mut tasks, store.settings.sort);
    let clock_24h = store.settings.clock_24h;
    let items = tasks
        .into_iter()
        .map(|t| {
            let mut details = Vec::new();
            if let Some(due) = t.due {
                details.push(trf("Due {date}", &[("date", &relative_date(due, today))]));
            }
            if t.recurrence != Recurrence::Never {
                details.push(t.recurrence.label_every(t.repeat_every));
            }
            if let Some(r) = t.reminder {
                details.push(trf("Reminder {time}", &[("time", &format_time(r, clock_24h))]));
            }
            Item {
                title: t.title.clone(),
                details,
                notes: t.notes.trim().to_string(),
                steps: t.steps.iter().map(|s| (s.title.clone(), s.done)).collect(),
            }
        })
        .collect();
    Printable {
        title: title.to_string(),
        date: trf("As of {date}", &[("date", &crate::i18n::date_long(today))]),
        items,
    }
}

/// Plain text, to paste into an email or a message.
pub fn to_text(p: &Printable) -> String {
    let mut out = format!("{}\n{}\n", p.title, p.date);
    if p.items.is_empty() {
        out.push_str(&format!("\n{}\n", tr("Nothing in this list.")));
    }
    for item in &p.items {
        out.push_str(&format!("\n• {}\n", item.title));
        if !item.details.is_empty() {
            out.push_str(&format!("  {}\n", item.details.join(" · ")));
        }
        for line in item.notes.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str(&format!("  {}\n", line.trim()));
        }
        for (step, done) in &item.steps {
            out.push_str(&format!("  {} {}\n", if *done { "☑" } else { "☐" }, step));
        }
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A printable page (large, plain, black on white) with a Print button that hides itself
/// when printing.
pub fn to_html(p: &Printable) -> String {
    let mut items = String::new();
    if p.items.is_empty() {
        items.push_str(&format!("<p class=\"empty\">{}</p>", escape(tr("Nothing in this list."))));
    }
    for item in &p.items {
        items.push_str("<li><div class=\"title\">");
        items.push_str(&escape(&item.title));
        items.push_str("</div>");
        if !item.details.is_empty() {
            items.push_str(&format!("<div class=\"details\">{}</div>", escape(&item.details.join(" · "))));
        }
        if !item.notes.is_empty() {
            let notes = escape(&item.notes).replace('\n', "<br>");
            items.push_str(&format!("<div class=\"notes\">{notes}</div>"));
        }
        if !item.steps.is_empty() {
            items.push_str("<ul class=\"steps\">");
            for (step, done) in &item.steps {
                items.push_str(&format!("<li>{} {}</li>", if *done { "☑" } else { "☐" }, escape(step)));
            }
            items.push_str("</ul>");
        }
        items.push_str("</li>");
    }
    format!(
        r#"<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  body {{ font: 18px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; color: #111; background: #fff; margin: 0; }}
  main {{ max-width: 760px; margin: 0 auto; padding: 32px 24px; }}
  h1 {{ font-size: 2rem; margin: 0 0 4px; }}
  .date {{ color: #555; margin-bottom: 24px; }}
  ol {{ list-style: none; padding: 0; margin: 0; }}
  ol > li {{ padding: 14px 0; border-top: 1px solid #ccc; break-inside: avoid; }}
  .title {{ font-size: 1.35rem; font-weight: 600; }}
  .details {{ color: #444; font-size: 1rem; }}
  .notes {{ margin-top: 4px; }}
  .steps {{ margin: 6px 0 0; padding-left: 4px; list-style: none; }}
  .empty {{ color: #555; }}
  .print {{ font: inherit; font-size: 1rem; padding: 10px 18px; margin-bottom: 24px; cursor: pointer; }}
  .made {{ color: #888; font-size: 0.85rem; margin-top: 32px; }}
  @media print {{ .print {{ display: none; }} main {{ padding: 0; }} }}
</style></head>
<body><main>
<button class="print" onclick="window.print()">{print}</button>
<h1>{title}</h1>
<div class="date">{date}</div>
<ol>{items}</ol>
<div class="made">RustDo</div>
</main></body></html>
"#,
        lang = crate::i18n::code(),
        title = escape(&p.title),
        date = escape(&p.date),
        print = escape(tr("Print or save as PDF")),
    )
}

/// A file name for the printable page: the list's name, made safe.
pub fn file_name(title: &str) -> String {
    let safe: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    let safe = safe.trim();
    format!("RustDo - {}.html", if safe.is_empty() { "List" } else { safe })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Step, Task};

    fn sample() -> Printable {
        let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let mut store = Store::new();
        let list = store.add_list("Medications").unwrap();
        let now = today.and_hms_opt(9, 0, 0).unwrap();
        let mut a = Task::new("Metformin 500 mg", list, now);
        a.notes = "Twice a day with meals.\nDr. Smith".into();
        a.recurrence = Recurrence::Daily;
        a.steps = vec![Step::new("Morning"), Step::new("Evening")];
        let mut done = Task::new("Old prescription", list, now);
        done.done = true;
        store.tasks.push(a);
        store.tasks.push(done);
        store.tasks.push(Task::new("Vitamin D <1000 IU>", list, now));
        build(&store, View::List(list), "Medications", today)
    }

    #[test]
    fn shows_open_tasks_with_their_details() {
        let p = sample();
        assert_eq!(p.items.len(), 2, "completed tasks are left out");
        let m = p.items.iter().find(|i| i.title.starts_with("Metformin")).unwrap();
        assert_eq!(m.details, ["Daily"]);
        assert_eq!(m.steps.len(), 2);
        let text = to_text(&p);
        assert!(text.starts_with("Medications\nAs of Thursday, November 26\n"));
        assert!(text.contains("• Metformin 500 mg\n  Daily\n  Twice a day with meals.\n  Dr. Smith\n  ☐ Morning\n"));
    }

    #[test]
    fn html_is_escaped_and_printable() {
        let html = to_html(&sample());
        assert!(html.contains("Vitamin D &lt;1000 IU&gt;"));
        assert!(!html.contains("<1000"));
        assert!(html.contains("window.print()"));
        assert!(html.contains("Twice a day with meals.<br>Dr. Smith"));
    }

    #[test]
    fn safe_file_names() {
        assert_eq!(file_name("Meds/Pills: 2026"), "RustDo - Meds_Pills_ 2026.html");
        assert_eq!(file_name("///"), "RustDo - ___.html");
        assert_eq!(file_name(""), "RustDo - List.html");
    }
}
