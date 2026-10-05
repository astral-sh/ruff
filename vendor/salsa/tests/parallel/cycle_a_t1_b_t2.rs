//! Test a specific cycle scenario:
//!
//! ```text
//! Thread T1          Thread T2
//! ---------          ---------
//!    |                  |
//!    v                  |
//! query_a()             |
//!  ^  |                 v
//!  |  +------------> query_b()
//!  |                    |
//!  +--------------------+
//! ```

use crate::sync::thread;
use crate::{Knobs, KnobsDatabase};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

const MIN: CycleValue = CycleValue(0);
const MAX: CycleValue = CycleValue(3);

// Signal 1: T1 has entered `query_a`
// Signal 2: T2 has entered `query_b`

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_a(db: &dyn KnobsDatabase) -> CycleValue {
    db.signal(1);

    // Wait for Thread T2 to enter `query_b` before we continue.
    db.wait_for(2);

    query_b(db)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_b(db: &dyn KnobsDatabase) -> CycleValue {
    // Wait for Thread T1 to enter `query_a` before we continue.
    db.wait_for(1);

    db.signal(2);

    let a_value = query_a(db);
    CycleValue(a_value.0 + 1).min(MAX)
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

        let t1 = thread::spawn(move || {
            let _span = tracing::debug_span!("t1", thread_id = ?thread::current().id()).entered();
            query_a(&db_t1)
        });
        let t2 = thread::spawn(move || {
            let _span = tracing::debug_span!("t2", thread_id = ?thread::current().id()).entered();
            query_b(&db_t2)
        });

        let (r_t1, r_t2) = (t1.join().unwrap(), t2.join().unwrap());

        assert_eq!((r_t1, r_t2), (MAX, MAX));
    });
}
