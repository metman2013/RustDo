//! Two RustDo devices syncing through a real RustDo Hub over HTTP.
//! Start a hub first, then:  RUSTDO_HUB_URL=http://127.0.0.1:7979 cargo test --test hub_e2e -- --ignored
use rustdo_app::model::{Store, View};
use rustdo_app::sync::{self, Done, Job};

fn run(job: Job) -> Done {
    let rx = sync::run(vec![job], eframe::egui::Context::default());
    rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap().pop().unwrap()
}

fn round(store: &mut Store, hub: &str) {
    let share = store.shares.remove(0);
    let out = sync::outgoing(store, &share, chrono::Utc::now().timestamp_millis());
    let mut share = share;
    match run(Job::Sync { hub: hub.into(), out }) {
        Done::Synced(out, Ok(resp)) => {
            sync::apply(store, &mut share, &out, &resp);
        }
        Done::Synced(_, Err(e)) => panic!("sync failed: {e}"),
        _ => unreachable!(),
    }
    store.shares.push(share);
}

#[test]
#[ignore = "needs a running hub (RUSTDO_HUB_URL)"]
fn two_devices_share_a_list() {
    let hub = sync::normalize_hub(&std::env::var("RUSTDO_HUB_URL").expect("set RUSTDO_HUB_URL")).unwrap();
    let now = chrono::Local::now().naive_local();

    let mut alice = Store::new();
    let list = alice.add_list("Groceries").unwrap();
    alice.add_task("Milk", View::List(list), now);
    let code = match run(Job::Share { hub: hub.clone(), list_id: list }) {
        Done::Shared { result, .. } => result.expect("share"),
        _ => unreachable!(),
    };
    alice.shares.push(sync::Share { list_id: list, hub: hub.clone(), code: code.clone(), rev: 0, known: Default::default(), last_sync: None });
    round(&mut alice, &hub);

    let mut bob = Store::new();
    let resp = match run(Job::Join { hub: hub.clone(), code: sync::normalize_code(&code.to_lowercase()).unwrap() }) {
        Done::Joined { result, .. } => result.expect("join"),
        _ => unreachable!(),
    };
    let share = sync::join(&mut bob, &hub, &code, &resp).expect("new list");
    bob.shares.push(share);
    assert_eq!(bob.list_name(list), "Groceries");
    assert!(bob.tasks.iter().any(|t| t.title == "Milk"));

    bob.add_task("Eggs", View::List(list), now);
    round(&mut bob, &hub);
    round(&mut alice, &hub);
    assert!(alice.tasks.iter().any(|t| t.title == "Eggs"));

    // A wrong code gets a readable error.
    match run(Job::Join { hub: hub.clone(), code: "ZZZZ-ZZZZ".into() }) {
        Done::Joined { result: Err(e), .. } => assert!(e.contains("code"), "{e}"),
        _ => panic!("expected an error"),
    }
}

// ------------------------------------------------------------------ Sync my devices

use rustdo_app::account;
use rustdo_app::notes::Notes;

fn account_done(rx: std::sync::mpsc::Receiver<account::Done>) -> account::Done {
    rx.recv_timeout(std::time::Duration::from_secs(30)).expect("the hub answered")
}

/// One account round for a device (its tasks and sticky notes).
fn account_round(store: &mut Store, notes: &mut Notes) {
    let mut acc = store.account.take().expect("sync is on");
    let out = account::outgoing(store, Some(notes), None, &acc, chrono::Utc::now().timestamp_millis());
    match account_done(account::sync(&acc.hub.clone(), out, &eframe::egui::Context::default())) {
        account::Done::Synced(out, Ok(resp)) => {
            account::apply(store, Some(notes), None, &mut acc, &out, &resp);
        }
        account::Done::Synced(_, Err(e)) => panic!("account sync failed: {e}"),
        _ => unreachable!(),
    }
    store.account = Some(acc);
}

#[test]
#[ignore = "needs a running hub (RUSTDO_HUB_URL)"]
fn two_devices_sync_an_account() {
    let hub = sync::normalize_hub(&std::env::var("RUSTDO_HUB_URL").expect("set RUSTDO_HUB_URL")).unwrap();
    let now = chrono::Local::now().naive_local();

    // The first device starts an account and uploads a list, a task and a sticky note.
    let key = match account_done(account::create(hub.clone(), &eframe::egui::Context::default())) {
        account::Done::Created { result, .. } => result.expect("new account"),
        _ => unreachable!(),
    };
    assert!(account::normalize_key(&key).is_some(), "{key}");
    let mut pc = Store::new();
    let mut pc_notes = Notes::default();
    let work = pc.add_list("Work").unwrap();
    pc.add_task("Quarterly report", View::List(work), now);
    let note = pc_notes.add(now, Some([10.0, 10.0]));
    pc_notes.get_mut(note).unwrap().text = "Wi-Fi: sunflower".into();
    pc.account = Some(account::Account::new(hub.clone(), key.clone()));
    account_round(&mut pc, &mut pc_notes);

    // A fresh phone joins with the key and gets everything, without an extra empty list.
    let mut phone = Store::new();
    let mut phone_notes = Notes::default();
    assert!(account::is_fresh(&phone, Some(&phone_notes)));
    account::clear_for_pull(&mut phone);
    let mut acc = account::Account::new(hub.clone(), key.to_lowercase());
    acc.key = account::normalize_key(&acc.key).unwrap();
    acc.pull_only = true;
    phone.account = Some(acc);
    account_round(&mut phone, &mut phone_notes);
    assert_eq!(phone.lists.len(), pc.lists.len());
    assert_eq!(phone.list_name(work), "Work");
    assert!(phone.tasks.iter().any(|t| t.title == "Quarterly report"));
    assert_eq!(phone_notes.notes.len(), 1);
    assert_eq!(phone_notes.notes[0].text, "Wi-Fi: sunflower");

    // Changes flow back, and list order follows.
    phone.add_task("Book flights", View::List(work), now);
    phone.move_list(work, 0);
    account_round(&mut phone, &mut phone_notes);
    account_round(&mut pc, &mut pc_notes);
    assert!(pc.tasks.iter().any(|t| t.title == "Book flights"));
    assert_eq!(pc.lists[0].id, work);

    // A wrong key gets a readable error.
    let mut stranger = Store::new();
    stranger.account = Some(account::Account::new(hub.clone(), "ABCDE-ABCDE-ABCDE-ABCDE".into()));
    let acc = stranger.account.clone().unwrap();
    let out = account::outgoing(&stranger, None, None, &acc, 1);
    match account_done(account::sync(&hub, out, &eframe::egui::Context::default())) {
        account::Done::Synced(_, Err(e)) => assert!(e.contains("key"), "{e}"),
        _ => panic!("expected an error"),
    }
}
