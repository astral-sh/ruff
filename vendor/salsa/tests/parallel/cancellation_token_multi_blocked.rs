// Shuttle doesn't like panics inside of its runtime.
#![cfg(not(feature = "shuttle"))]

//! Test for cancellation when multiple queries are blocked on the cancelled thread.
//!
//! This test verifies that:
//! 1. When a thread is cancelled, blocked threads recompute rather than propagate cancellation
//! 2. The final result is correctly computed by the remaining threads
use salsa::{Cancelled, Database};

use crate::setup::{Knobs, KnobsDatabase};

#[salsa::tracked(returns(copy))]
fn query_a(db: &dyn KnobsDatabase) -> u32 {
    query_b(db)
}

#[salsa::tracked(returns(copy))]
fn query_b(db: &dyn KnobsDatabase) -> u32 {
    // Signal that t1 has started computing query_b
    db.signal(1);
    // Wait for t2 and t3 to block on us
    db.wait_for(3);
    // Wait for cancellation to happen
    db.wait_for(4);
    query_c(db)
}

#[salsa::tracked(returns(copy))]
fn query_c(_db: &dyn KnobsDatabase) -> u32 {
    42
}

/// Test that when a thread is cancelled, other blocked threads successfully
/// recompute the query and get the correct result.
#[test]
fn multiple_threads_blocked_on_cancelled() {
    let db = Knobs::default();
    let db2 = db.clone();
    let db3 = db.clone();
    let db_signaler = db.clone();
    let token = db.cancellation_token();

    // Thread 1: Starts computing query_a -> query_b, will be cancelled
    let t1 = std::thread::spawn(move || query_a(&db));

    // Wait for t1 to start query_b
    db_signaler.wait_for(1);

    // Thread 2: Will block on query_a (which is blocked on query_b)
    db2.signal_on_will_block(2);
    let t2 = std::thread::spawn(move || query_a(&db2));

    // Wait for t2 to block
    db_signaler.wait_for(2);

    // Thread 3: Also blocks on query_a
    db3.signal_on_will_block(3);
    let t3 = std::thread::spawn(move || query_a(&db3));

    // Wait for t3 to block
    db_signaler.wait_for(3);

    // Now cancel t1
    token.cancel();

    // Let t1 continue and get cancelled
    db_signaler.signal(4);

    // Collect results
    let r1 = t1.join();
    let r2 = t2.join();
    let r3 = t3.join();

    // t1 should have been cancelled
    let r1_cancelled = r1.unwrap_err().downcast::<salsa::Cancelled>().map(|c| *c);
    assert!(
        matches!(r1_cancelled, Ok(Cancelled::Local)),
        "t1 should be locally cancelled, got: {:?}",
        r1_cancelled
    );

    // t2 and t3 should both succeed with the correct value
    assert_eq!(r2.unwrap(), 42, "t2 should compute the correct result");
    assert_eq!(r3.unwrap(), 42, "t3 should compute the correct result");
}
