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
