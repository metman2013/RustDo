//! Pinned lists: a list shown as a card, as its own window on the desktop (Windows and
//! Linux) or on the sticky notes wall (web and Android).
//!
//! A pinned list is a sticky note that points at a list ([`StickyNote::list`]), so it
//! moves, sizes, syncs and sits on the wall like any note. Whoever shows the cards (the
//! main window, or the background companion while it runs) fills in each list's tasks
//! ([`CardList`]) and carries out what was done on a card ([`CardAction`]).
//!
//! The companion never edits the tasks while the main window is open: it passes the
//! actions over in a small file the main window picks up. With the main window closed it
//! saves them to the task file itself (sync sends them the next time RustDo opens).

use crate::model::{sort_tasks, Store, View};
use crate::notes::StickyNote;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// What a card shows of its list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CardList {
    pub name: String,
    pub icon: &'static str,
    /// Open tasks in the list's order, then completed ones (most recent first).
    pub tasks: Vec<CardTask>,
    pub open: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardTask {
    pub id: Uuid,
    pub title: String,
    pub done: bool,
    pub important: bool,
    pub due: Option<NaiveDate>,
    pub overdue: bool,
}

impl CardList {
    /// Shows an action on the card at once, before its owner has carried it out (the
    /// companion only looks every second).
    pub fn preview(&mut self, action: &CardAction) {
        match action {
            CardAction::SetDone(id, done) => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.done = *done;
                }
            }
            CardAction::SetImportant(id, on) => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                    t.important = *on;
                }
            }
            CardAction::ReminderFired(_) => {}
            CardAction::Add(_, title) => self.tasks.insert(
                0,
                CardTask { id: Uuid::new_v4(), title: title.clone(), done: false, important: false, due: None, overdue: false },
            ),
        }
        self.open = self.tasks.iter().filter(|t| !t.done).count();
    }
}

/// Something done on a card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CardAction {
    SetDone(Uuid, bool),
    SetImportant(Uuid, bool),
    Add(Uuid, String),
    /// Android: the reminder was shown as a notification while RustDo was in the background.
    ReminderFired(Uuid),
}

/// The note fallback text: older RustDo versions show a pinned list as this plain note.
pub fn fallback_text(name: &str) -> String {
    format!("📋 {name}")
}

/// A new card for `list`.
pub fn new_card(list: Uuid, name: &str, now: NaiveDateTime) -> StickyNote {
    let mut n = StickyNote::new(now, [0xFF, 0xFF, 0xFF]);
    n.list = Some(list);
    n.text = fallback_text(name);
    n.size = [300.0, 360.0];
    n
}

/// The icon a list has everywhere: 👥 shared, 🏠 the default list, 📋 any other.
pub fn list_icon(store: &Store, id: Uuid) -> &'static str {
    if store.shares.iter().any(|s| s.list_id == id) {
        "👥"
    } else if id == store.default_list {
        "🏠"
    } else {
        "📋"
    }
}

/// The cards' contents for every pinned list in `cards`.
pub fn cards_from_store(store: &Store, lists: impl IntoIterator<Item = Uuid>, today: NaiveDate) -> HashMap<Uuid, CardList> {
    let mut out = HashMap::new();
    for list in lists {
        if out.contains_key(&list) || !store.lists.iter().any(|l| l.id == list) {
            continue;
        }
        let mut open: Vec<&crate::model::Task> = store.tasks.iter().filter(|t| t.list_id == list && !t.done).collect();
        sort_tasks(&mut open, store.settings.sort);
        let mut done: Vec<&crate::model::Task> = store.tasks.iter().filter(|t| t.list_id == list && t.done).collect();
        done.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
        let card = |t: &crate::model::Task| CardTask {
            id: t.id,
            title: t.title.clone(),
            done: t.done,
            important: t.important,
            due: t.due,
            overdue: t.is_overdue(today),
        };
        out.insert(
            list,
            CardList {
                name: store.list_name(list).to_string(),
                icon: list_icon(store, list),
                open: open.len(),
                tasks: open.iter().map(|t| card(t)).chain(done.iter().map(|t| card(t))).collect(),
            },
        );
    }
    out
}

/// Carries out card actions on the tasks. Returns whether anything changed.
pub fn apply(store: &mut Store, actions: &[CardAction], now: NaiveDateTime) -> bool {
    let mut changed = false;
    for a in actions {
        match a {
            CardAction::SetDone(id, done) => {
                if store.task(*id).is_some_and(|t| t.done != *done) {
                    store.set_done(*id, *done, now);
                    changed = true;
                }
            }
            CardAction::SetImportant(id, on) => {
                if let Some(t) = store.task_mut(*id).filter(|t| t.important != *on) {
                    t.important = *on;
                    changed = true;
                }
            }
            CardAction::ReminderFired(id) => {
                if let Some(t) = store.task_mut(*id).filter(|t| !t.reminder_fired) {
                    t.reminder_fired = true;
                    changed = true;
                }
            }
            CardAction::Add(list, title) => {
                if store.lists.iter().any(|l| l.id == *list) && store.add_task(title, View::List(*list), now).is_some() {
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Desktop: the companion hands card actions to the open main window through this file.
/// Android: the home-screen widget and reminder notifications do the same while RustDo's
/// process is running (MainActivity.java writes the same lines).
#[cfg(not(target_arch = "wasm32"))]
pub mod handoff {
    use super::CardAction;
    use std::io::Write;

    fn path() -> Option<std::path::PathBuf> {
        #[cfg(target_os = "android")]
        return crate::platform::android_data_dir().map(|d| d.join("card-actions.jsonl"));
        #[cfg(not(target_os = "android"))]
        crate::companion::runtime_path("card-actions.jsonl")
    }

    /// Companion: queue actions for the main window.
    #[cfg_attr(target_os = "android", allow(dead_code))]
    pub fn send(actions: &[CardAction]) {
        let Some(path) = path() else { return };
        let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) else { return };
        for a in actions {
            if let Ok(line) = serde_json::to_string(a) {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    /// Main window: take the queued actions.
    pub fn take() -> Vec<CardAction> {
        let Some(path) = path() else { return Vec::new() };
        if !path.exists() {
            return Vec::new();
        }
        // Renamed first, so actions arriving meanwhile go to a fresh file.
        let taking = path.with_extension("taking");
        if std::fs::rename(&path, &taking).is_err() {
            return Vec::new();
        }
        let text = std::fs::read_to_string(&taking).unwrap_or_default();
        let _ = std::fs::remove_file(&taking);
        text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 28).unwrap().and_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn cards_show_open_tasks_then_done_and_actions_apply() {
        let mut store = Store::new();
        let list = store.add_list("Groceries").unwrap();
        let milk = store.add_task("Milk", View::List(list), now()).unwrap();
        let bread = store.add_task("Bread", View::List(list), now()).unwrap();
        store.add_task("Elsewhere", View::List(store.default_list), now());
        store.set_done(milk, true, now());

        let cards = cards_from_store(&store, [list], now().date());
        let c = &cards[&list];
        assert_eq!(c.name, "Groceries");
        assert_eq!(c.open, 1);
        assert_eq!(c.tasks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Bread", "Milk"]);

        let actions = vec![
            CardAction::SetDone(bread, true),
            CardAction::SetImportant(milk, true),
            CardAction::Add(list, "Eggs".into()),
            CardAction::Add(Uuid::new_v4(), "Nowhere".into()),
        ];
        assert!(apply(&mut store, &actions, now()));
        let c = &cards_from_store(&store, [list], now().date())[&list];
        assert_eq!(c.open, 1);
        assert_eq!(c.tasks[0].title, "Eggs");
        assert!(store.task(milk).unwrap().important);
        assert!(!store.tasks.iter().any(|t| t.title == "Nowhere"));
        assert!(!apply(&mut store, &actions[..2], now()), "already done");
    }

    #[test]
    fn actions_round_trip_as_json() {
        let a = CardAction::Add(Uuid::new_v4(), "Milk \"2%\"".into());
        let back: CardAction = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(a, back);
    }
}
