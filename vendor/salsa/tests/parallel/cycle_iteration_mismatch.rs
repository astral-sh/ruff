//! Test for iteration count mismatch bug where cycle heads have different iteration counts
//!
//! This test aims to reproduce the scenario where:
//! 1. A memo has multiple cycle heads with different iteration counts
//! 2. When validating, iteration counts mismatch causes re-execution
//! 3. After re-execution, the memo still has the same mismatched iteration counts

use crate::sync::thread;
use crate::{Knobs, KnobsDatabase};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

const MIN: CycleValue = CycleValue(0);
const MAX: CycleValue = CycleValue(5);

// Query A: First cycle head - will iterate multiple times
#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_a(db: &dyn KnobsDatabase) -> CycleValue {
    let b = query_b(db);
    CycleValue(b.0 + 1).min(MAX)
}

// Query B: Depends on C and D, creating complex dependencies
#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_b(db: &dyn KnobsDatabase) -> CycleValue {
    let c = query_c(db);
    let d = query_d(db);
    CycleValue(c.0.max(d.0) + 1).min(MAX)
}

// Query C: Creates a cycle back to A
#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_c(db: &dyn KnobsDatabase) -> CycleValue {
    let a = query_a(db);
    // Also depends on E to create more complex cycle structure
    let e = query_e(db);
    CycleValue(a.0.max(e.0))
}

// Query D: Part of a separate cycle with E
#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_d(db: &dyn KnobsDatabase) -> CycleValue {
    let e = query_e(db);
    CycleValue(e.0 + 1).min(MAX)
}

// Query E: Depends back on D and F
#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_e(db: &dyn KnobsDatabase) -> CycleValue {
    let d = query_d(db);
    let f = query_f(db);
    CycleValue(d.0.max(f.0) + 1).min(MAX)
}

// Query F: Creates another cycle that might have different iteration count
#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_f(db: &dyn KnobsDatabase) -> CycleValue {
    // Create a cycle that depends on earlier queries
    let b = query_b(db);
    let e = query_e(db);
    CycleValue(b.0.max(e.0))
}

fn initial(_db: &dyn KnobsDatabase, _id: salsa::Id) -> CycleValue {
    MIN
}

#[test_log::test]
fn test_iteration_mismatch() {
    crate::sync::check(|| {
        tracing::debug!("Starting new run");
        let db_t1 = Knobs::default();
        let db_t2 = db_t1.clone();
        let db_t3 = db_t1.clone();
        let db_t4 = db_t1.clone();

        // Thread 1: Starts with query_a - main cycle head
        let t1 = thread::spawn(move || {
            let _span = tracing::debug_span!("t1", thread_id = ?thread::current().id()).entered();
            query_a(&db_t1)
        });

        // Thread 2: Starts with query_d - separate cycle that will have different iteration
        let t2 = thread::spawn(move || {
            let _span = tracing::debug_span!("t2", thread_id = ?thread::current().id()).entered();
            query_d(&db_t2)
        });

        // Thread 3: Starts with query_f after others have started
        let t3 = thread::spawn(move || {
            let _span = tracing::debug_span!("t3", thread_id = ?thread::current().id()).entered();
            query_f(&db_t3)
        });

        // Thread 4: Queries b which depends on multiple cycles
        let t4 = thread::spawn(move || {
            let _span = tracing::debug_span!("t4", thread_id = ?thread::current().id()).entered();
            query_b(&db_t4)
        });

        let r_t1 = t1.join().unwrap();
        let r_t2 = t2.join().unwrap();
        let r_t3 = t3.join().unwrap();
        let r_t4 = t4.join().unwrap();

        // All queries should converge to the same value
        assert_eq!(r_t1, r_t2);
        assert_eq!(r_t2, r_t3);
        assert_eq!(r_t3, r_t4);

        // They should have computed a non-initial value
        assert!(r_t1.0 > MIN.0);
    });
}
