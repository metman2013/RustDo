//! Core data model and business rules. No UI code lives here so it can be unit-tested.

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime, Weekday};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Recurrence {
    #[default]
    Never,
    Daily,
    Weekdays,
    Weekly,
    Monthly,
    Yearly,
}

impl Recurrence {
    pub const ALL: [Recurrence; 6] = [
        Self::Never,
        Self::Daily,
        Self::Weekdays,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Never => "Doesn't repeat",
            Self::Daily => "Daily",
            Self::Weekdays => "Weekdays",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Yearly => "Yearly",
        }
    }

    /// The next occurrence strictly after `from`.
    pub fn next_after(self, from: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Never => None,
            Self::Daily => Some(from + Duration::days(1)),
            Self::Weekdays => {
                let mut d = from + Duration::days(1);
                while matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                    d += Duration::days(1);
                }
                Some(d)
            }
            Self::Weekly => Some(from + Duration::weeks(1)),
            // chrono clamps to the last valid day (Jan 31 -> Feb 28/29).
            Self::Monthly => from.checked_add_months(Months::new(1)),
            Self::Yearly => from.checked_add_months(Months::new(12)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortMode {
    #[default]
    Created,
    DueDate,
    Alphabetical,
    Importance,
}

impl SortMode {
    pub const ALL: [SortMode; 4] = [
        Self::Created,
        Self::DueDate,
        Self::Alphabetical,
        Self::Importance,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Created => "Newest first",
            Self::DueDate => "Due date",
            Self::Alphabetical => "Alphabetical",
            Self::Importance => "Importance",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub sort: SortMode,
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Look for a newer release on GitHub at most once a day (desktop and Android).
    #[serde(default = "yes")]
    pub check_updates: bool,
    #[serde(default)]
    pub last_update_check: Option<NaiveDateTime>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sort: SortMode::default(),
            theme: ThemeChoice::default(),
            check_updates: true,
            last_update_check: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
    pub done: bool,
}

impl Step {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            done: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskList {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub list_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub important: bool,
    /// The day this task was added to "My Day". My Day resets automatically each day.
    #[serde(default)]
    pub my_day: Option<NaiveDate>,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    #[serde(default)]
    pub reminder: Option<NaiveDateTime>,
    #[serde(default)]
    pub reminder_fired: bool,
    #[serde(default)]
    pub recurrence: Recurrence,
    #[serde(default)]
    pub steps: Vec<Step>,
    pub created: NaiveDateTime,
    #[serde(default)]
    pub completed_at: Option<NaiveDateTime>,
    /// Where an imported task came from (e.g. `mstodo:<graph id>`), so re-importing
    /// the same export doesn't create duplicates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

impl Task {
    pub fn new(title: impl Into<String>, list_id: Uuid, now: NaiveDateTime) -> Self {
        Self {
            id: Uuid::new_v4(),
            list_id,
            title: title.into(),
            notes: String::new(),
            done: false,
            important: false,
            my_day: None,
            due: None,
            reminder: None,
            reminder_fired: false,
            recurrence: Recurrence::Never,
            steps: Vec::new(),
            created: now,
            completed_at: None,
            source_id: None,
        }
    }

    pub fn in_my_day(&self, today: NaiveDate) -> bool {
        self.my_day == Some(today)
    }

    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.done && self.due.is_some_and(|d| d < today)
    }

    /// The date a task shows up under in the Planned view.
    pub fn planned_date(&self) -> Option<NaiveDate> {
        self.due.or(self.reminder.map(|r| r.date()))
    }

    /// `query` must already be lowercase.
    pub fn matches_query(&self, query: &str) -> bool {
        self.title.to_lowercase().contains(query)
            || self.notes.to_lowercase().contains(query)
            || self
                .steps
                .iter()
                .any(|s| s.title.to_lowercase().contains(query))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    MyDay,
    Important,
    Planned,
    All,
    Completed,
    List(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedBucket {
    Overdue,
    Today,
    Tomorrow,
    NextSevenDays,
    Later,
}

impl PlannedBucket {
    pub const ALL: [PlannedBucket; 5] = [
        Self::Overdue,
        Self::Today,
        Self::Tomorrow,
        Self::NextSevenDays,
        Self::Later,
    ];

    pub fn of(date: NaiveDate, today: NaiveDate) -> Self {
        let days = (date - today).num_days();
        match days {
            d if d < 0 => Self::Overdue,
            0 => Self::Today,
            1 => Self::Tomorrow,
            2..=7 => Self::NextSevenDays,
            _ => Self::Later,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Overdue => "Earlier",
            Self::Today => "Today",
            Self::Tomorrow => "Tomorrow",
            Self::NextSevenDays => "Next 7 days",
            Self::Later => "Later",
        }
    }
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "default_version")]
    pub version: u32,
    pub default_list: Uuid,
    pub lists: Vec<TaskList>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    pub fn new() -> Self {
        let id = Uuid::new_v4();
        Self {
            version: 1,
            default_list: id,
            lists: vec![TaskList {
                id,
                name: "Tasks".into(),
            }],
            tasks: Vec::new(),
            settings: Settings::default(),
        }
    }

    /// Make a loaded store internally consistent (e.g. after hand-editing the JSON).
    pub fn repair(&mut self) {
        if !self.lists.iter().any(|l| l.id == self.default_list) {
            if let Some(first) = self.lists.first() {
                self.default_list = first.id;
            } else {
                let id = Uuid::new_v4();
                self.lists.push(TaskList {
                    id,
                    name: "Tasks".into(),
                });
                self.default_list = id;
            }
        }
        let default = self.default_list;
        let known: Vec<Uuid> = self.lists.iter().map(|l| l.id).collect();
        for t in &mut self.tasks {
            if !known.contains(&t.list_id) {
                t.list_id = default;
            }
        }
    }

    pub fn list_name(&self, id: Uuid) -> &str {
        self.lists
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.name.as_str())
            .unwrap_or("Tasks")
    }

    pub fn add_list(&mut self, name: &str) -> Option<Uuid> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let id = Uuid::new_v4();
        self.lists.push(TaskList {
            id,
            name: name.to_string(),
        });
        Some(id)
    }

    pub fn rename_list(&mut self, id: Uuid, name: &str) -> bool {
        let name = name.trim();
        match self.lists.iter_mut().find(|l| l.id == id) {
            Some(l) if !name.is_empty() => {
                l.name = name.to_string();
                true
            }
            _ => false,
        }
    }

    /// Deletes a list and all of its tasks. The default list cannot be deleted.
    pub fn delete_list(&mut self, id: Uuid) -> bool {
        if id == self.default_list || !self.lists.iter().any(|l| l.id == id) {
            return false;
        }
        self.lists.retain(|l| l.id != id);
        self.tasks.retain(|t| t.list_id != id);
        true
    }

    pub fn task(&self, id: Uuid) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn task_mut(&mut self, id: Uuid) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    /// Adds a task in the context of `view`, the way Microsoft To Do does:
    /// adding from My Day puts it in My Day, from Important marks it important, etc.
    pub fn add_task(&mut self, title: &str, view: View, now: NaiveDateTime) -> Option<Uuid> {
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        let list_id = match view {
            View::List(id) if self.lists.iter().any(|l| l.id == id) => id,
            _ => self.default_list,
        };
        let mut task = Task::new(title, list_id, now);
        match view {
            View::MyDay => task.my_day = Some(now.date()),
            View::Important => task.important = true,
            View::Planned => task.due = Some(now.date()),
            _ => {}
        }
        let id = task.id;
        self.tasks.insert(0, task);
        Some(id)
    }

    /// Marks a task done or not done. Completing a repeating task creates its next
    /// occurrence, whose id is returned.
    pub fn set_done(&mut self, id: Uuid, done: bool, now: NaiveDateTime) -> Option<Uuid> {
        let idx = self.tasks.iter().position(|t| t.id == id)?;
        let task = &mut self.tasks[idx];
        if task.done == done {
            return None;
        }
        task.done = done;
        task.completed_at = done.then_some(now);
        if !done || task.recurrence == Recurrence::Never {
            return None;
        }

        let base = task.planned_date().unwrap_or(now.date());
        let next_date = task.recurrence.next_after(base)?;
        let shift = next_date - base;

        let mut next = task.clone();
        // The completed instance stops repeating so re-checking it can't spawn duplicates.
        task.recurrence = Recurrence::Never;

        next.id = Uuid::new_v4();
        next.done = false;
        next.completed_at = None;
        next.created = now;
        next.my_day = None;
        next.reminder_fired = false;
        next.due = next.due.map(|_| next_date);
        next.reminder = next.reminder.map(|r| r + shift);
        for s in &mut next.steps {
            s.id = Uuid::new_v4();
            s.done = false;
        }
        let next_id = next.id;
        self.tasks.insert(idx, next);
        Some(next_id)
    }

    /// Removes a task and returns it with its index so it can be restored (undo).
    pub fn remove_task(&mut self, id: Uuid) -> Option<(usize, Task)> {
        let idx = self.tasks.iter().position(|t| t.id == id)?;
        Some((idx, self.tasks.remove(idx)))
    }

    pub fn restore_task(&mut self, idx: usize, mut task: Task) {
        if !self.lists.iter().any(|l| l.id == task.list_id) {
            task.list_id = self.default_list;
        }
        let idx = idx.min(self.tasks.len());
        self.tasks.insert(idx, task);
    }

    /// Whether `task` belongs in `view` (ignoring completion, except for the Completed view).
    pub fn view_contains(&self, view: View, task: &Task, today: NaiveDate) -> bool {
        match view {
            View::MyDay => task.in_my_day(today),
            View::Important => task.important,
            View::Planned => task.planned_date().is_some(),
            View::All => true,
            View::Completed => task.done,
            View::List(id) => task.list_id == id,
        }
    }

    /// Number of open tasks shown next to a view in the sidebar.
    pub fn open_count(&self, view: View, today: NaiveDate) -> usize {
        self.tasks
            .iter()
            .filter(|t| !t.done && self.view_contains(view, t, today))
            .count()
    }

    /// Marks reminders that are due as fired and returns their task ids.
    pub fn fire_due_reminders(&mut self, now: NaiveDateTime) -> Vec<Uuid> {
        let mut fired = Vec::new();
        for t in &mut self.tasks {
            if !t.done && !t.reminder_fired && t.reminder.is_some_and(|r| r <= now) {
                t.reminder_fired = true;
                fired.push(t.id);
            }
        }
        fired
    }
}

pub fn sort_tasks(tasks: &mut [&Task], mode: SortMode) {
    match mode {
        SortMode::Created => tasks.sort_by_key(|t| Reverse(t.created)),
        SortMode::DueDate => tasks.sort_by_key(|t| (t.due.is_none(), t.due, Reverse(t.created))),
        SortMode::Alphabetical => tasks.sort_by_key(|t| t.title.to_lowercase()),
        SortMode::Importance => tasks.sort_by_key(|t| (!t.important, Reverse(t.created))),
    }
}

/// Human friendly date relative to today: "Today", "Tomorrow", "Friday", "Mon, Dec 1".
pub fn relative_date(date: NaiveDate, today: NaiveDate) -> String {
    let days = (date - today).num_days();
    match days {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        2..=6 => date.format("%A").to_string(),
        _ if date.year() == today.year() => date.format("%a, %b %-d").to_string(),
        _ => date.format("%a, %b %-d, %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(date: NaiveDate, h: u32, m: u32) -> NaiveDateTime {
        date.and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn recurrence_dates() {
        assert_eq!(Recurrence::Daily.next_after(d(2026, 11, 19)), Some(d(2026, 11, 20)));
        // Friday -> Monday
        assert_eq!(Recurrence::Weekdays.next_after(d(2026, 11, 20)), Some(d(2026, 11, 23)));
        assert_eq!(Recurrence::Weekly.next_after(d(2026, 11, 19)), Some(d(2026, 11, 26)));
        assert_eq!(Recurrence::Monthly.next_after(d(2026, 1, 31)), Some(d(2026, 2, 28)));
        assert_eq!(Recurrence::Yearly.next_after(d(2028, 2, 29)), Some(d(2029, 2, 28)));
        assert_eq!(Recurrence::Never.next_after(d(2026, 1, 1)), None);
    }

    #[test]
    fn add_task_respects_view_context() {
        let mut s = Store::new();
        let now = at(d(2026, 11, 19), 9, 0);
        let a = s.add_task("A", View::MyDay, now).unwrap();
        let b = s.add_task("B", View::Important, now).unwrap();
        let c = s.add_task("C", View::Planned, now).unwrap();
        assert!(s.add_task("   ", View::All, now).is_none());
        assert!(s.task(a).unwrap().in_my_day(now.date()));
        assert!(s.task(b).unwrap().important);
        assert_eq!(s.task(c).unwrap().due, Some(now.date()));
        assert_eq!(s.tasks[0].id, c, "new tasks go on top");
    }

    #[test]
    fn my_day_resets_next_day() {
        let mut s = Store::new();
        let now = at(d(2026, 11, 19), 9, 0);
        s.add_task("A", View::MyDay, now).unwrap();
        assert_eq!(s.open_count(View::MyDay, d(2026, 11, 19)), 1);
        assert_eq!(s.open_count(View::MyDay, d(2026, 11, 20)), 0);
    }

    #[test]
    fn completing_recurring_task_spawns_next() {
        let mut s = Store::new();
        let now = at(d(2026, 11, 19), 9, 0);
        let id = s.add_task("Water plants", View::All, now).unwrap();
        {
            let t = s.task_mut(id).unwrap();
            t.recurrence = Recurrence::Weekly;
            t.due = Some(d(2026, 11, 19));
            t.reminder = Some(at(d(2026, 11, 19), 8, 0));
            t.reminder_fired = true;
            t.steps.push(Step { id: Uuid::new_v4(), title: "Fern".into(), done: true });
        }
        let next = s.set_done(id, true, now).expect("next occurrence");
        let old = s.task(id).unwrap();
        assert!(old.done);
        assert_eq!(old.recurrence, Recurrence::Never);
        let n = s.task(next).unwrap();
        assert!(!n.done);
        assert_eq!(n.due, Some(d(2026, 11, 26)));
        assert_eq!(n.reminder, Some(at(d(2026, 11, 26), 8, 0)));
        assert!(!n.reminder_fired);
        assert!(!n.steps[0].done);
        assert_eq!(n.recurrence, Recurrence::Weekly);

        // Unchecking and re-checking the old one must not create another copy.
        s.set_done(id, false, now);
        assert!(s.set_done(id, true, now).is_none());
        assert_eq!(s.tasks.len(), 2);
    }

    #[test]
    fn reminders_fire_once() {
        let mut s = Store::new();
        let now = at(d(2026, 11, 19), 9, 0);
        let id = s.add_task("Call", View::All, now).unwrap();
        s.task_mut(id).unwrap().reminder = Some(at(d(2026, 11, 19), 10, 0));
        assert!(s.fire_due_reminders(now).is_empty());
        assert_eq!(s.fire_due_reminders(at(d(2026, 11, 19), 10, 0)), vec![id]);
        assert!(s.fire_due_reminders(at(d(2026, 11, 19), 11, 0)).is_empty());
    }

    #[test]
    fn delete_list_removes_tasks_but_not_default() {
        let mut s = Store::new();
        let now = at(d(2026, 11, 19), 9, 0);
        let groceries = s.add_list("Groceries").unwrap();
        s.add_task("Milk", View::List(groceries), now);
        s.add_task("Other", View::All, now);
        assert!(!s.delete_list(s.default_list));
        assert!(s.delete_list(groceries));
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.lists.len(), 1);
    }

    #[test]
    fn remove_and_restore() {
        let mut s = Store::new();
        let now = at(d(2026, 11, 19), 9, 0);
        let a = s.add_task("A", View::All, now).unwrap();
        let (idx, t) = s.remove_task(a).unwrap();
        assert!(s.task(a).is_none());
        s.restore_task(idx, t);
        assert!(s.task(a).is_some());
    }

    #[test]
    fn json_with_missing_fields_loads() {
        let list = Uuid::new_v4();
        let json = format!(
            r#"{{"default_list":"{list}","lists":[{{"id":"{list}","name":"Tasks"}}],
               "tasks":[{{"id":"{}","list_id":"{}","title":"Old","created":"2026-01-01T00:00:00"}}]}}"#,
            Uuid::new_v4(),
            Uuid::new_v4()
        );
        let mut s: Store = serde_json::from_str(&json).unwrap();
        s.repair();
        assert_eq!(s.tasks[0].list_id, list, "orphaned task moved to default list");
        assert_eq!(s.tasks[0].recurrence, Recurrence::Never);
    }

    #[test]
    fn planned_buckets_and_labels() {
        let today = d(2026, 11, 19); // Thursday
        assert_eq!(PlannedBucket::of(d(2026, 11, 1), today), PlannedBucket::Overdue);
        assert_eq!(PlannedBucket::of(today, today), PlannedBucket::Today);
        assert_eq!(PlannedBucket::of(d(2026, 11, 26), today), PlannedBucket::NextSevenDays);
        assert_eq!(PlannedBucket::of(d(2026, 12, 25), today), PlannedBucket::Later);
        assert_eq!(relative_date(d(2026, 11, 21), today), "Saturday");
        assert_eq!(relative_date(d(2026, 12, 25), today), "Fri, Dec 25");
    }
}
