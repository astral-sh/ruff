use std::cell::RefCell;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, ThreadId};
use std::time::Duration;

use super::{Input, Scenario};
use crate::function::SyncOwner;
use crate::plumbing::{AsId, FromId};
use crate::zalsa::ZalsaDatabase;
use crate::{Database, DatabaseImpl, Id};

const TIMEOUT: Duration = Duration::from_secs(5);

struct CreatorGates {
    request: Sender<Id>,
    acquired: Receiver<ThreadId>,
    release: Sender<()>,
    finished: Receiver<u32>,
}

struct BodyGates {
    acquired: Sender<ThreadId>,
    release: Receiver<()>,
}

thread_local! {
    static CREATOR: RefCell<Option<CreatorGates>> = const { RefCell::new(None) };
    static BODY: RefCell<Option<BodyGates>> = const { RefCell::new(None) };
}

#[crate::tracked]
struct RemoteItem<'db> {
    #[returns(copy)]
    value: (),
}

#[crate::tracked(returns(copy), specify)]
fn value(db: &dyn Database, item: RemoteItem<'_>) -> u32 {
    item.value(db);
    if let Some(gates) = BODY.with_borrow_mut(Option::take) {
        gates.acquired.send(thread::current().id()).unwrap();
        gates.release.recv_timeout(TIMEOUT).unwrap();
    }
    7
}

#[crate::tracked(returns(copy))]
fn creator(db: &dyn Database, _input: Input) -> RemoteItem<'_> {
    let item = RemoteItem::new(db, ());
    let gates = CREATOR.with_borrow_mut(Option::take).unwrap();
    gates.request.send(item.as_id()).unwrap();
    let remote = gates.acquired.recv_timeout(TIMEOUT).unwrap();
    assert_ne!(remote, thread::current().id());

    let ingredient = value::fn_ingredient_(db, db.zalsa());
    let slot = ingredient.memo_slot(
        db.zalsa(),
        item.as_id(),
        ingredient.memo_ingredient_index(db.zalsa(), item.as_id()),
    );
    assert!(slot.get_erased().is_none());
    let before = ingredient
        .sync_table
        .test_transfer_state(item.as_id())
        .unwrap();
    assert!(matches!(before.owner, SyncOwner::Thread(owner) if owner == remote));

    value::specify(db, item, 99);

    assert!(slot.get_erased().is_none());
    let after = ingredient
        .sync_table
        .test_transfer_state(item.as_id())
        .unwrap();
    assert!(matches!(after.owner, SyncOwner::Thread(owner) if owner == remote));
    assert!(!after.claimed_twice);
    gates.release.send(()).unwrap();
    assert_eq!(gates.finished.recv_timeout(TIMEOUT).unwrap(), 7);
    assert_eq!(value(db, item), 7);
    value::specify(db, item, 100);
    assert_eq!(value(db, item), 7);
    item
}

#[test]
fn remote_execution_retains_precedence_over_its_creator() {
    let db = DatabaseImpl::default();
    let remote_db = db.clone();
    let input = Input::new(&db, Scenario::LiveClaim);
    let (request, requests) = channel();
    let (acquired, acquisition) = channel();
    let (release, releases) = channel();
    let (finished, completion) = channel();
    let remote = thread::spawn(move || {
        let id = requests.recv_timeout(TIMEOUT).unwrap();
        BODY.set(Some(BodyGates {
            acquired,
            release: releases,
        }));
        let item = RemoteItem::from_id(id);
        finished.send(value(&remote_db, item)).unwrap();
        assert!(BODY.with_borrow(Option::is_none));
        assert!(remote_db.zalsa_local().active_query().is_none());
    });
    CREATOR.set(Some(CreatorGates {
        request,
        acquired: acquisition,
        release,
        finished: completion,
    }));
    let item = creator(&db, input);
    remote.join().unwrap();
    assert!(CREATOR.with_borrow(Option::is_none));
    assert_eq!(value(&db, item), 7);
    assert!(db.zalsa_local().active_query().is_none());
    let ingredient = value::fn_ingredient_(&db, db.zalsa());
    assert!(
        ingredient
            .sync_table
            .test_transfer_state(item.as_id())
            .is_none()
    );
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    assert!(graph.edges.entries.iter().all(Option::is_none) && !graph.edges.overflow);
    assert!(graph.transferred.entries.iter().all(Option::is_none) && !graph.transferred.overflow);
}
