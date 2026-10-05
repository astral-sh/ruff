//! Test a deeply nested-cycle scenario where cycles have changing query dependencies.
//!
//! The trick is that different threads call into the same cycle from different entry queries and
//! the cycle heads change over different iterations
//!
//! * Thread 1: `a` -> b -> c
//! * Thread 2: `b`
//! * Thread 3: `d` -> `c`
//! * Thread 4: `e` -> `c`
//!
//! `c` calls:
//! * `d` and `a` in the first few iterations
//! * `d`, `b` and `e` in the last iterations
use crate::sync::thread;
use crate::{Knobs, KnobsDatabase};

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

    if d_value > CycleValue(0) {
        let e_value = query_e(db);
        let b_value = query_b(db);
        CycleValue(d_value.0.max(e_value.0).max(b_value.0))
    } else {
        let a_value = query_a(db);
        CycleValue(d_value.0.max(a_value.0))
    }
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_d(db: &dyn KnobsDatabase) -> CycleValue {
    query_c(db)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_e(db: &dyn KnobsDatabase) -> CycleValue {
    query_c(db)
}

fn initial(_db: &dyn KnobsDatabase, _id: salsa::Id) -> CycleValue {
    MIN
}

#[test_log::test]
fn the_test() {
    crate::sync::check(|| {
        tracing::debug!("Starting new run");
        let db_t1 = Knobs::default();
        let db_t2 = db_t1.clone();
        let db_t3 = db_t1.clone();
        let db_t4 = db_t1.clone();

        let t1 = thread::spawn(move || {
            let _span = tracing::debug_span!("t1", thread_id = ?thread::current().id()).entered();
            let result = query_a(&db_t1);
            db_t1.signal(1);
            result
        });
        let t2 = thread::spawn(move || {
            let _span = tracing::debug_span!("t2", thread_id = ?thread::current().id()).entered();
            db_t4.wait_for(1);
            query_b(&db_t4)
        });
        let t3 = thread::spawn(move || {
            let _span = tracing::debug_span!("t3", thread_id = ?thread::current().id()).entered();
            db_t2.wait_for(1);
            query_d(&db_t2)
        });
        let t4 = thread::spawn(move || {
            let _span = tracing::debug_span!("t4", thread_id = ?thread::current().id()).entered();
            db_t3.wait_for(1);
            query_e(&db_t3)
        });

        let r_t1 = t1.join().unwrap();
        let r_t2 = t2.join().unwrap();
        let r_t3 = t3.join().unwrap();
        let r_t4 = t4.join().unwrap();

        assert_eq!((r_t1, r_t2, r_t3, r_t4), (MAX, MAX, MAX, MAX));
    });
}
