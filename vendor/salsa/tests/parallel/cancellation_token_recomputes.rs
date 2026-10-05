// Shuttle doesn't like panics inside of its runtime.
#![cfg(not(feature = "shuttle"))]

//! Test for cancellation when another query is blocked on the cancelled thread.
use salsa::{Cancelled, Database};

use crate::setup::{Knobs, KnobsDatabase};

#[salsa::tracked(returns(copy))]
fn query_a(db: &dyn KnobsDatabase) -> u32 {
    query_b(db)
}

#[salsa::tracked(returns(copy))]
fn query_b(db: &dyn KnobsDatabase) -> u32 {
    db.signal(1);
    db.wait_for(3);
    query_c(db)
}

#[salsa::tracked(returns(copy))]
fn query_c(_db: &dyn KnobsDatabase) -> u32 {
    1
}

#[test]
fn execute() {
    let db = Knobs::default();
    let db2 = db.clone();
    let db_signaler = db.clone();
    let token = db.cancellation_token();

    let t1 = std::thread::spawn(move || query_a(&db));
    db_signaler.wait_for(1);
    db2.signal_on_will_block(2);
    let t2 = std::thread::spawn(move || query_a(&db2));
    db_signaler.wait_for(2);
    token.cancel();
    db_signaler.signal(3);
    let (r1, r2) = (t1.join(), t2.join());
    let r1 = *r1.unwrap_err().downcast::<salsa::Cancelled>().unwrap();
    assert!(matches!(r1, Cancelled::Local), "{r1:?}");
    assert_eq!(r2.unwrap(), 1);
}
