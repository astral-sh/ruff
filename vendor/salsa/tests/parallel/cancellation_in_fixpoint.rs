// Shuttle doesn't like panics inside of its runtime.
#![cfg(not(feature = "shuttle"))]

//! A pending write must not leave reusable provisional memos behind.
//!
//! `cycle_a` and `cycle_b` establish a fixpoint cycle. The test interrupts that cycle
//! with an explicit cancellation that does not advance the revision. The retry must start from a
//! fresh cycle initial value.

use salsa::{Cancelled, Database};

use crate::setup::{Knobs, KnobsDatabase};
use crate::sync::thread;

#[salsa::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy), cycle_fn = cycle_fn, cycle_initial = cycle_initial)]
fn cycle_a(db: &dyn KnobsDatabase, input: Input) -> u32 {
    let value = cycle_b(db, input);

    db.signal(1);
    db.wait_for(2);
    cancellation_point(db, input);

    value
}

#[salsa::tracked(returns(copy), cycle_fn = cycle_fn, cycle_initial = cycle_initial)]
fn cycle_b(db: &dyn KnobsDatabase, input: Input) -> u32 {
    let value = cycle_a(db, input);
    value.saturating_add(1).min(input.value(db))
}

#[salsa::tracked]
fn cancellation_point(db: &dyn KnobsDatabase, input: Input) {
    input.value(db);
}

fn cycle_initial(_db: &dyn KnobsDatabase, _id: salsa::Id, _input: Input) -> u32 {
    0
}

fn cycle_fn(
    _db: &dyn KnobsDatabase,
    _cycle: &salsa::Cycle,
    _last_provisional_value: &u32,
    value: u32,
    _input: Input,
) -> u32 {
    value
}

#[test]
fn same_revision_cancellation_leaves_fixpoint_cycle_reusable() {
    let db = Knobs::default();
    let db_writer = db.clone();
    let db_t1 = db.clone();
    let db_waiter = db.clone();
    let input = Input::new(&db, 3);

    db.signal_on_did_cancel(2);
    drop(db);

    let t1 = thread::spawn(move || Cancelled::catch(|| cycle_a(&db_t1, input)));

    db_waiter.wait_for(1);
    drop(db_waiter);

    let t2 = thread::spawn({
        let mut db_writer = db_writer;
        move || {
            db_writer.trigger_cancellation();
            db_writer
        }
    });

    assert!(matches!(t1.join().unwrap(), Err(Cancelled::PendingWrite)));

    let db_after = t2.join().unwrap();

    assert_eq!(cycle_a(&db_after, input), 3);
}
