// Shuttle doesn't like panics inside of its runtime.
#![cfg(not(feature = "shuttle"))]

//! Test for panic in cycle recovery function, in cross-thread cycle.
use crate::setup::{Knobs, KnobsDatabase};

#[salsa::tracked(returns(copy), cycle_fn=cycle_fn, cycle_initial=initial)]
fn query_a(db: &dyn KnobsDatabase) -> u32 {
    db.signal(1);
    db.wait_for(2);
    query_b(db)
}

#[salsa::tracked(returns(copy), cycle_fn=cycle_fn, cycle_initial=initial)]
fn query_b(db: &dyn KnobsDatabase) -> u32 {
    db.wait_for(1);
    db.signal(2);
    query_a(db) + 1
}

fn cycle_fn(
    _db: &dyn KnobsDatabase,
    _cycle: &salsa::Cycle,
    _last_provisional_value: &u32,
    _value: u32,
) -> u32 {
    panic!("cancel!")
}

fn initial(_db: &dyn KnobsDatabase, _id: salsa::Id) -> u32 {
    0
}

#[test]
fn execute() {
    let db = Knobs::default();

    let db_t1 = db.clone();
    let t1 = std::thread::spawn(move || query_a(&db_t1));

    let db_t2 = db.clone();
    let t2 = std::thread::spawn(move || query_b(&db_t2));

    // The main thing here is that we don't deadlock.
    let (r1, r2) = (t1.join(), t2.join());
    assert!(r1.is_err());
    assert!(r2.is_err());
}
