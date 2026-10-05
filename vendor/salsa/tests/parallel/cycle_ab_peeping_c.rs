//! Test a specific cycle scenario:
//!
//! Thread T1 calls A which calls B which calls A.
//!
//! Thread T2 calls C which calls B.
//!
//! The trick is that the call from Thread T2 comes before B has reached a fixed point.
//! We want to be sure that C sees the final value (and blocks until it is complete).
use crate::sync::thread;
use crate::{Knobs, KnobsDatabase};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

const MIN: CycleValue = CycleValue(0);
const MID: CycleValue = CycleValue(5);
const MAX: CycleValue = CycleValue(10);

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_a(db: &dyn KnobsDatabase) -> CycleValue {
    let b_value = query_b(db);

    // When we reach the mid point, signal stage 1 (unblocking T2)
    // and then wait for T2 to signal stage 2.
    if b_value == MID {
        db.signal(1);
        db.wait_for(2);
    }

    b_value
}

fn cycle_initial(_db: &dyn KnobsDatabase, _id: salsa::Id) -> CycleValue {
    MIN
}

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn query_b(db: &dyn KnobsDatabase) -> CycleValue {
    let a_value = query_a(db);

    CycleValue(a_value.0 + 1).min(MAX)
}

#[salsa::tracked(returns(copy))]
fn query_c(db: &dyn KnobsDatabase) -> CycleValue {
    // Wait until T1 has reached MID then execute `query_b`.
    // This should block and (due to the configuration on our database) signal stage 2.
    db.wait_for(1);

    query_b(db)
}

#[test_log::test]
fn the_test() {
    crate::sync::check(|| {
        let db_t1 = Knobs::default();

        let db_t2 = db_t1.clone();
        db_t2.signal_on_will_block(2);

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
