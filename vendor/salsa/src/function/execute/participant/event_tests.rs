use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use crate::attempt_probe::transfer_test_support::{self as trace, Action, Kind, TraceConfig};
use crate::function::{ClaimResult, Reentrancy};
use crate::plumbing::AsId;
use crate::runtime::WaitResult;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, Event, EventKind, Id, Setter};

#[derive(Clone, Copy)]
enum Child {
    Acyclic,
    Final,
    IndependentCycle,
    DeadEnd { caught: bool },
    DifferentHandle,
}

thread_local! {
    static CALLBACK: RefCell<Option<(Db, Input, DatabaseKeyIndex, Child)>> = const { RefCell::new(None) };
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static VALUE: Cell<Option<u32>> = const { Cell::new(None) };
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[crate::db]
#[derive(Clone)]
struct Db {
    storage: crate::Storage<Self>,
}
#[crate::db]
impl Database for Db {}
impl Default for Db {
    fn default() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(event))),
        }
    }
}

#[crate::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy))]
fn leaf(db: &dyn Database, input: Input) -> u32 {
    input.value(db) % 2
}

#[crate::tracked(returns(copy), cycle_initial = initial, cycle_fn = recover)]
fn verified(db: &dyn Database, input: Input) -> u32 {
    leaf(db, input)
}

#[crate::tracked(returns(copy))]
fn dependent(db: &dyn Database, input: Input) -> u32 {
    verified(db, input) + 1
}

#[crate::tracked(returns(copy))]
fn acyclic(db: &dyn Database, input: Input) -> u32 {
    input.value(db) + 4
}

#[crate::tracked(returns(copy))]
fn final_child(_db: &dyn Database) -> u32 {
    7
}

#[crate::tracked(returns(copy), cycle_initial = initial, cycle_fn = recover)]
fn independent(db: &dyn Database, input: Input) -> u32 {
    (independent(db, input) + 1).min(3)
}

fn initial(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
    0
}
fn recover(_db: &dyn Database, _cycle: &Cycle<'_>, _last: &u32, value: u32, _input: Input) -> u32 {
    value
}

fn event(event: Event) {
    let EventKind::DidValidateMemoizedValue { database_key } = event.kind else {
        return;
    };
    if !ARMED.get()
        || !CALLBACK.with_borrow(|slot| {
            slot.as_ref()
                .is_some_and(|(_, _, key, _)| *key == database_key)
        })
    {
        return;
    }
    ARMED.set(false);
    CALLBACK.with_borrow(|slot| {
        let (different_handle, input, key, child) = slot.as_ref().unwrap();
        crate::attach::with_attached_database(|db| {
            assert!(db.zalsa_local().active_query().is_none());
            assert!(matches!(
                db.zalsa()
                    .lookup_ingredient(key.ingredient_index())
                    .as_function()
                    .unwrap()
                    .sync_table()
                    .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
                ClaimResult::Cycle { .. }
            ));
            let outcome = catch_unwind(AssertUnwindSafe(|| match child {
                Child::Acyclic => acyclic(db, *input),
                Child::Final => final_child(db),
                Child::IndependentCycle => independent(db, *input),
                Child::DeadEnd { .. } => dependent(db, *input),
                Child::DifferentHandle => acyclic(different_handle, *input),
            }));
            match outcome {
                Ok(value) => VALUE.set(Some(value)),
                Err(payload) => {
                    let message = payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("non-string panic");
                    PANIC.with_borrow_mut(|observed| *observed = Some(message.to_owned()));
                    if !matches!(child, Child::DeadEnd { caught: true }) {
                        resume_unwind(payload);
                    }
                }
            }
        })
        .expect("tracked verification has an attached database");
    });
}

struct Clear;
impl Drop for Clear {
    fn drop(&mut self) {
        ARMED.set(false);
        CALLBACK.with_borrow_mut(Option::take);
    }
}

fn run(child: Child) {
    let _clear = Clear;
    VALUE.set(None);
    PANIC.with_borrow_mut(|slot| *slot = None);
    let mut db = Db::default();
    let input = Input::new(&db, 0);
    assert_eq!(verified(&db, input), 0);
    assert_eq!(final_child(&db), 7);
    input.set_value(&mut db).to(2);
    let key = verified::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    let dependent_key =
        dependent::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    CALLBACK.with_borrow_mut(|slot| *slot = Some((db.clone(), input, key, child)));
    ARMED.set(true);
    let (result, observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || catch_unwind(AssertUnwindSafe(|| verified(&db, input))),
    );
    assert!(
        !ARMED.get(),
        "deep verification must call the installed event"
    );
    assert!(!observations.broken);
    if let Child::DeadEnd { caught } = child {
        assert_eq!(result.is_ok(), caught);
        assert_eq!(
            VALUE.get(),
            None,
            "the independent callback cannot receive a seed"
        );
        PANIC.with_borrow(|message| {
            assert!(message.as_ref().unwrap().contains("dependency graph cycle"))
        });
        assert_eq!(
            observations
                .records
                .iter()
                .filter(|record| record.event.key == Some(dependent_key)
                    && record.event.kind == Kind::Terminal
                    && record.event.action == Some(Action::Panic))
                .count(),
            1
        );
        assert!(
            !observations
                .records
                .iter()
                .any(|record| record.event.key == Some(dependent_key)
                    && record.event.kind == Kind::Terminal
                    && matches!(record.event.wait, Some(WaitResult::Completed)))
        );
    } else if matches!(child, Child::DifferentHandle) {
        assert!(result.is_err());
        assert_eq!(VALUE.get(), None);
        PANIC.with_borrow(|message| {
            assert!(
                message
                    .as_ref()
                    .unwrap()
                    .contains("Cannot change database mid-query")
            );
        });
    } else {
        assert_eq!(result.unwrap(), 0);
        assert_eq!(
            VALUE.get(),
            Some(match child {
                Child::Acyclic => 6,
                Child::Final => 7,
                Child::IndependentCycle => 3,
                Child::DeadEnd { .. } | Child::DifferentHandle => unreachable!(),
            })
        );
        PANIC.with_borrow(|message| assert!(message.is_none()));
    }
    CALLBACK.with_borrow_mut(Option::take);
    assert!(db.zalsa_local().active_query().is_none());
    for key in [key, dependent_key] {
        let function = db
            .zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap();
        assert!(matches!(
            function
                .sync_table()
                .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
    }
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    assert!(graph.edges.entries.iter().all(Option::is_none) && !graph.edges.overflow);
    assert!(graph.pending.entries.iter().all(Option::is_none) && !graph.pending.overflow);
    assert!(graph.transferred.entries.iter().all(Option::is_none) && !graph.transferred.overflow);
    // A new revision gives a caught or propagated callback failure a supported fresh retry.
    input.set_value(&mut db).to(4);
    assert_eq!(verified(&db, input), 0);
    assert_eq!(dependent(&db, input), 1);
}

#[test]
fn attached_database_callback_preserves_supported_children() {
    for child in [Child::Acyclic, Child::Final, Child::IndependentCycle] {
        run(child);
    }
}

#[test]
fn attached_database_callback_rejects_an_owned_synchronous_dead_end() {
    for caught in [false, true] {
        run(Child::DeadEnd { caught });
    }
}

#[test]
fn cloned_database_callback_preserves_the_existing_attachment_rejection() {
    run(Child::DifferentHandle);
}

#[cfg(feature = "accumulator")]
#[crate::accumulator]
struct Warning(u32);

#[cfg(feature = "accumulator")]
#[crate::tracked(returns(copy), cycle_initial = initial, cycle_fn = recover)]
fn metadata_head(db: &dyn Database, input: Input) -> u32 {
    metadata_reader(db, input)
}

#[cfg(feature = "accumulator")]
#[crate::tracked(returns(copy))]
fn metadata_reader(db: &dyn Database, input: Input) -> u32 {
    metadata_head::accumulated::<Warning>(db, input).len() as u32
}

#[cfg(feature = "accumulator")]
#[test]
fn active_cycle_cannot_reuse_its_value_read_for_accumulator_metadata() {
    let db = Db::default();
    let input = Input::new(&db, 0);
    let result = catch_unwind(AssertUnwindSafe(|| metadata_head(&db, input)));
    let payload = result.expect_err("metadata reads cannot inherit a provisional value");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic");
    assert!(message.contains("Fixpoint iteration doesn't support accumulated values"));
    assert!(db.zalsa_local().active_query().is_none());
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    assert!(graph.edges.entries.iter().all(Option::is_none) && !graph.edges.overflow);
    assert!(graph.transferred.entries.iter().all(Option::is_none) && !graph.transferred.overflow);
}
