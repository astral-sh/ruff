//! Test a specific cycle scenario:
//!
//! 1. Thread T1 calls `a` which calls `b`
//! 2. Thread T2 calls `c` which calls `b` (blocks on T1 for `b`). The ordering here is important!
//! 3. Thread T1: `b` calls `c` and `a`, both trigger a cycle and Salsa returns a fixpoint initial values (with `c` and `a` as cycle heads).
//! 4. Thread T1: `b` is released (its not in its own cycle heads), `Memo::provisional_retry` blocks blocks on `T2` because `c` is in its cycle heads
//! 5. Thread T2: Iterates `c`, blocks on T1 when reading `a`.
//! 6. Thread T1: Completes the first itaration of `a`, inserting a provisional that depends on `c` and itself (`a`).
//!    Starts a new iteration where it executes `b`. Calling `query_a` hits a cycle:
//!
//!    1. `fetch_cold` returns the current provisional for `a` that depends both on `a` (owned by itself) and `c` (has no cycle heads).
//!    2. `Memo::provisional_retry`: Awaits `c` (which has no cycle heads anymore).
//!        - Before: it skipped over the dependency key `a` that it is holding itself. It sees that `c` is final, so it retries (which gets us back to 6.1)
//!        - Now: Return the provisional memo and allow the outer cycle to resolve.
//!
//! The desired behavior here is that:
//! 1. `t1`: completes the first iteration of b
//! 2. `t2`: completes the cycle `c`, up to where it only depends on `a`, now blocks on `a`
//! 3. `t1`: Iterates on `a`, finalizes the memo

use crate::sync::thread;

use crate::setup::{Knobs, KnobsDatabase};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

const MIN: CycleValue = CycleValue(0);
const MAX: CycleValue = CycleValue(1);

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_a(db: &dyn KnobsDatabase) -> CycleValue {
    query_b(db)
}

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_b(db: &dyn KnobsDatabase) -> CycleValue {
    // Wait for thread 2 to have entered `query_c`.
    tracing::debug!("Wait for signal 1 from thread 2");
    db.wait_for(1);

    // Unblock query_c on thread 2
    db.signal(2);
    tracing::debug!("Signal 2 for thread 2");

    let c_value = query_c(db);

    tracing::debug!("query_b: c = {:?}", c_value);

    let a_value = query_a(db);

    tracing::debug!("query_b: a = {:?}", a_value);

    CycleValue(a_value.0 + 1).min(MAX)
}

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_c(db: &dyn KnobsDatabase) -> CycleValue {
    tracing::debug!("query_c: signaling thread1 to call c");
    db.signal(1);

    tracing::debug!("query_c: waiting for signal");
    // Wait for thread 1 to acquire the lock on query_b
    db.wait_for(1);
    let b = query_b(db);
    tracing::debug!("query_c: b = {:?}", b);
    b
}

fn cycle_initial(_db: &dyn KnobsDatabase, _id: salsa::Id) -> CycleValue {
    MIN
}

#[test_log::test]
fn the_test() {
    crate::sync::check(|| {
        let db_t1 = Knobs::default();

        let db_t2 = db_t1.clone();

        let t1 = thread::spawn(move || {
            let _span = tracing::debug_span!("t1", thread_id = ?thread::current().id()).entered();
            query_a(&db_t1)
        });
        let t2 = thread::spawn(move || {
            let _span = tracing::debug_span!("t2", thread_id = ?thread::current().id()).entered();
            query_c(&db_t2)
        });

        let (r_t1, r_t2) = (t1.join().unwrap(), t2.join().unwrap());

        assert_eq!((r_t1, r_t2), (MAX, MAX));
    });
}
