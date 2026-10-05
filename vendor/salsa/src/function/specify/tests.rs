use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use crate::attempt_probe::transfer_test_support::{
    self as trace, Event, Kind, Mode as Release, TraceConfig,
};
use crate::function::{ClaimResult, Reentrancy, SyncOwner};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Database, DatabaseImpl, DatabaseKeyIndex, Id};

mod remote;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Scenario {
    Reassign,
    LiveClaim,
    WrongCreator,
    DerivedDropPanic,
}

#[crate::tracked]
struct Item<'db> {
    #[returns(copy)]
    input: Input,
}

#[crate::input]
struct Input {
    #[returns(copy)]
    scenario: Scenario,
}

#[derive(Debug, PartialEq, Eq)]
struct Value {
    value: u32,
    panic: Option<Arc<()>>,
}

impl Value {
    fn ordinary(value: u32) -> Self {
        Self { value, panic: None }
    }
}

thread_local! {
    static DROP_OWNER: Cell<Option<(DatabaseKeyIndex, DatabaseKeyIndex)>> = const { Cell::new(None) };
}

#[derive(Debug)]
struct NativeFailure(Arc<()>);

impl Drop for Value {
    fn drop(&mut self) {
        if let Some(identity) = self.panic.take() {
            let (source, owner) = DROP_OWNER.get().unwrap();
            crate::with_attached_database(|db| {
                assert_eq!(db.zalsa_local().active_query().unwrap().0, owner);
                assert_transferred(db, source, owner);
            })
            .unwrap();
            panic_any(NativeFailure(identity));
        }
    }
}

#[crate::tracked(returns(ref), specify)]
fn specified<'db>(db: &'db dyn Database, item: Item<'db>) -> Value {
    // This read makes the default implementation a real participant of its creator's cycle.
    owner(db, item.input(db));
    Value::ordinary(7)
}

#[crate::tracked(returns(copy))]
fn read_specified<'db>(db: &'db dyn Database, item: Item<'db>) -> u32 {
    specified(db, item).value
}

#[crate::tracked]
fn wrong_creator<'db>(db: &'db dyn Database, item: Item<'db>) {
    specified::specify(db, item, Value::ordinary(99));
}

fn assert_transferred(db: &dyn Database, source: DatabaseKeyIndex, owner: DatabaseKeyIndex) {
    let state = db
        .zalsa()
        .lookup_ingredient(source.ingredient_index())
        .as_function()
        .unwrap()
        .sync_table()
        .test_transfer_state(source.key_index())
        .unwrap();
    assert!(matches!(state.owner, SyncOwner::Transferred));
    assert!(!state.claimed_twice);
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    assert!(!graph.transferred.overflow);
    assert!(
        graph
            .transferred
            .entries
            .iter()
            .flatten()
            .any(|&(key, thread, target)| {
                key == source && thread == std::thread::current().id() && target == owner
            })
    );
}

#[crate::tracked(returns(copy), cycle_initial = initial)]
fn owner(db: &dyn Database, input: Input) -> Option<(Item<'_>, u32)> {
    let previous = owner(db, input);
    let item = previous.map_or_else(|| Item::new(db, input), |(item, _)| item);
    let iteration = previous.map_or(1, |(_, iteration)| (iteration + 1).min(3));
    let ingredient = specified::fn_ingredient_(db, db.zalsa());
    let source = ingredient.database_key_index(item.as_id());
    let owner = db.zalsa_local().active_query().unwrap().0;
    match input.scenario(db) {
        Scenario::Reassign => {
            let value = 41 + iteration;
            specified::specify(db, item, Value::ordinary(value));
            assert_eq!(specified(db, item).value, value);
            assert_transferred(db, source, owner);
            assert_eq!(read_specified(db, item), value);
            specified::specify(db, item, Value::ordinary(value));
            assert_transferred(db, source, owner);
            assert_eq!(specified(db, item).value, value);
            assert_eq!(read_specified(db, item), value);
        }
        Scenario::LiveClaim => {
            let before = ingredient
                .memo_slot(
                    db.zalsa(),
                    item.as_id(),
                    ingredient.memo_ingredient_index(db.zalsa(), item.as_id()),
                )
                .get_erased()
                .map(|memo| std::ptr::from_ref(memo.header()));
            let ClaimResult::Claimed(claim) = ingredient.sync_table.try_claim(
                db.zalsa(),
                db.zalsa_local(),
                item.as_id(),
                Reentrancy::Allow,
            ) else {
                panic!("fixture source must be claimable")
            };
            specified::specify(db, item, Value::ordinary(99));
            let after = ingredient
                .memo_slot(
                    db.zalsa(),
                    item.as_id(),
                    ingredient.memo_ingredient_index(db.zalsa(), item.as_id()),
                )
                .get_erased()
                .map(|memo| std::ptr::from_ref(memo.header()));
            assert_eq!(
                before, after,
                "a live writer retains its selected allocation"
            );
            assert!(!claim.drop());
            specified::specify(db, item, Value::ordinary(42));
            assert_eq!(specified(db, item).value, 42);
        }
        Scenario::WrongCreator => {
            let mut begin = Event::new(Kind::Gate).key(source);
            begin.phase = Some("specify.wrong.begin");
            trace::record(begin);
            let failure = catch_unwind(AssertUnwindSafe(|| wrong_creator(db, item))).unwrap_err();
            let message = failure
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| failure.downcast_ref::<String>().map(String::as_str))
                .unwrap();
            assert!(message.contains("created during the current tracked fn"));
            let mut end = Event::new(Kind::Gate).key(source);
            end.phase = Some("specify.wrong.end");
            trace::record(end);
            specified::specify(db, item, Value::ordinary(42));
            assert_eq!(specified(db, item).value, 42);
        }
        Scenario::DerivedDropPanic => {
            assert_eq!(specified(db, item).value, 7);
            assert_transferred(db, source, owner);
            let identity = Arc::new(());
            DROP_OWNER.set(Some((source, owner)));
            let failure = catch_unwind(AssertUnwindSafe(|| {
                specified::specify(
                    db,
                    item,
                    Value {
                        value: 99,
                        panic: Some(identity.clone()),
                    },
                );
            }))
            .unwrap_err();
            DROP_OWNER.set(None);
            let failure = failure.downcast::<NativeFailure>().unwrap();
            assert!(Arc::ptr_eq(&failure.0, &identity));
            assert_transferred(db, source, owner);
            assert_eq!(specified(db, item).value, 7);
        }
    }
    Some((item, iteration))
}

fn initial(_db: &dyn Database, _id: Id, _input: Input) -> Option<(Item<'_>, u32)> {
    None
}

fn run(scenario: Scenario, expected: u32) {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, scenario);
    let ((item, iteration), observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || owner(&db, input).unwrap(),
    );
    assert_eq!(iteration, 3);
    assert!(!observations.broken);
    assert_eq!(specified(&db, item).value, expected);
    assert!(db.zalsa_local().active_query().is_none());
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    assert!(graph.edges.entries.iter().all(Option::is_none) && !graph.edges.overflow);
    assert!(graph.transferred.entries.iter().all(Option::is_none) && !graph.transferred.overflow);
    let source = specified::fn_ingredient_(&db, db.zalsa()).database_key_index(item.as_id());
    if matches!(scenario, Scenario::Reassign | Scenario::DerivedDropPanic) {
        assert!(
            observations
                .records
                .iter()
                .any(|record| record.event.key == Some(source)
                    && record.event.kind == Kind::Claim
                    && record.event.mode == Some(Release::SelfOnly))
        );
        assert!(
            observations.records.iter().any(
                |record| record.event.key == Some(source) && record.event.kind == Kind::Restore
            )
        );
    }
    if scenario == Scenario::WrongCreator {
        for begin in observations
            .records
            .iter()
            .filter(|record| record.event.phase == Some("specify.wrong.begin"))
        {
            let end = observations
                .records
                .iter()
                .find(|record| {
                    record.ordinal > begin.ordinal
                        && record.event.phase == Some("specify.wrong.end")
                })
                .unwrap();
            assert!(
                !observations
                    .records
                    .iter()
                    .any(|record| record.ordinal > begin.ordinal
                        && record.ordinal < end.ordinal
                        && record.event.key == begin.event.key
                        && record.event.kind == Kind::Claim)
            );
        }
    }
}

#[test]
fn active_creator_reassigns_a_transferred_output() {
    run(Scenario::Reassign, 44);
}

#[test]
fn live_execution_retains_precedence_over_its_creator() {
    run(Scenario::LiveClaim, 42);
}

#[test]
fn wrong_creator_is_rejected_before_claiming_the_output() {
    run(Scenario::WrongCreator, 42);
}

#[test]
fn current_derived_value_and_transfer_survive_rejected_output_drop() {
    run(Scenario::DerivedDropPanic, 7);
}
