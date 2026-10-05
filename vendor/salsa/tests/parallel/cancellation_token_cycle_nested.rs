// Shuttle doesn't like panics inside of its runtime.
#![cfg(not(feature = "shuttle"))]

//! Test for cancellation with deeply nested cycles across multiple threads.
//!
//! These tests verify that local cancellation is disabled during cycle iteration,
//! allowing multi-threaded cycles to complete successfully before cancellation
//! can take effect.
use salsa::Database;

use crate::setup::{Knobs, KnobsDatabase};
use crate::sync::thread;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

const MIN: CycleValue = CycleValue(0);
const MAX: CycleValue = CycleValue(3);

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_a(db: &dyn KnobsDatabase) -> CycleValue {
    query_b(db)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_b(db: &dyn KnobsDatabase) -> CycleValue {
    let c_value = query_c(db);
    CycleValue(c_value.0 + 1).min(MAX)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_c(db: &dyn KnobsDatabase) -> CycleValue {
    let d_value = query_d(db);
    let e_value = query_e(db);
    let b_value = query_b(db);
    let a_value = query_a(db);
    CycleValue(d_value.0.max(e_value.0).max(b_value.0).max(a_value.0))
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_d(db: &dyn KnobsDatabase) -> CycleValue {
    query_c(db)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_e(db: &dyn KnobsDatabase) -> CycleValue {
    query_c(db)
}

#[salsa::tracked(returns(copy))]
fn query_f(db: &dyn KnobsDatabase) -> CycleValue {
    let c = query_c(db);
    // this should trigger cancellation again
    query_h(db);
    c
}

#[salsa::tracked(returns(copy))]
fn query_h(db: &dyn KnobsDatabase) {
    _ = db;
}

fn initial(db: &dyn KnobsDatabase, _id: salsa::Id) -> CycleValue {
    db.signal(1);
    db.wait_for(6);
    MIN
}

/// Test that a multi-threaded cycle completes successfully even when
/// cancellation is requested during the cycle.
///
/// This test is similar to cycle_nested_deep but adds cancellation during
/// the cycle to verify that cancellation is properly deferred.
#[test]
fn multi_threaded_cycle_completes_despite_cancellation() {
    let db = Knobs::default();
    let db_t1 = db.clone();
    let db_t2 = db.clone();
    let db_t3 = db.clone();
    let db_t4 = db.clone();
    let db_t5 = db.clone();
    let db_signaler = db;

    let token_t1 = db_t1.cancellation_token();
    let token_t2 = db_t2.cancellation_token();
    let token_t3 = db_t3.cancellation_token();
    let token_t5 = db_t5.cancellation_token();

    // Thread 1: Runs the main cycle, will have cancellation requested during it
    let t1 = thread::spawn(move || query_a(&db_t1));

    // Wait for t1 to start the cycle
    db_signaler.wait_for(1);

    // Spawn t2 and wait for it to block on the cycle
    db_signaler.signal_on_will_block(2);
    let t2 = thread::spawn(move || query_b(&db_t2));
    db_signaler.wait_for(2);

    // Spawn t3 and wait for it to block on the cycle
    db_signaler.signal_on_will_block(3);
    let t3 = thread::spawn(move || query_d(&db_t3));
    db_signaler.wait_for(3);

    // Spawn t4 - doesn't get cancelled
    db_signaler.signal_on_will_block(4);
    let t4 = thread::spawn(move || query_e(&db_t4));
    db_signaler.wait_for(4);

    // Spawn t5 - doesn't get cancelled
    db_signaler.signal_on_will_block(5);
    let t5 = thread::spawn(move || query_f(&db_t5));
    db_signaler.wait_for(5);

    // Request cancellation while t2 and t3 are blocked on the cycle
    // This should be deferred until after the cycle completes
    token_t1.cancel();
    token_t2.cancel();
    token_t3.cancel();
    token_t5.cancel();

    // Let t1 continue - the cycle should still complete because
    // cancellation is disabled during fixpoint iteration
    db_signaler.signal(6);

    // All threads should complete successfully
    let r_t1 = t1.join().unwrap();
    let r_t2 = t2.join().unwrap();
    let r_t3 = t3.join().unwrap();
    let r_t4 = t4.join().unwrap();

    let r_t5 = t5.join().unwrap_err();

    // All should get MAX because cycles defer cancellation
    assert_eq!(r_t1, MAX, "t1 should get MAX");
    assert_eq!(r_t2, MAX, "t2 should get MAX");
    assert_eq!(r_t3, MAX, "t3 should get MAX");
    assert_eq!(r_t4, MAX, "t4 should get MAX");
    assert!(
        matches!(
            *r_t5.downcast::<salsa::Cancelled>().unwrap(),
            salsa::Cancelled::Local
        ),
        "t5 should be cancelled as its blocked on the cycle, not participating in it and calling an uncomputed query after"
    );
}
