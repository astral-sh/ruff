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
use crate::KnobsDatabase;

const FALLBACK_A: u32 = 0b01;
const FALLBACK_B: u32 = 0b10;
const OFFSET_A: u32 = 0b0100;
const OFFSET_B: u32 = 0b1000;

// Signal 1: T1 has entered `query_a`
// Signal 2: T2 has entered `query_b`

#[salsa::tracked(returns(copy), cycle_result=cycle_result_a)]
fn query_a(db: &dyn KnobsDatabase) -> u32 {
    db.signal(1);

    // Wait for Thread T2 to enter `query_b` before we continue.
    db.wait_for(2);

    query_b(db) | OFFSET_A
}

#[salsa::tracked(returns(copy), cycle_result=cycle_result_b)]
fn query_b(db: &dyn KnobsDatabase) -> u32 {
    // Wait for Thread T1 to enter `query_a` before we continue.
    db.wait_for(1);

    db.signal(2);

    query_a(db) | OFFSET_B
}

fn cycle_result_a(_db: &dyn KnobsDatabase, _id: salsa::Id) -> u32 {
    FALLBACK_A
}

fn cycle_result_b(_db: &dyn KnobsDatabase, _id: salsa::Id) -> u32 {
    FALLBACK_B
}

#[test_log::test]
fn the_test() {
    use crate::Knobs;
    use crate::sync::thread;

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

        let (r_t1, r_t2) = (t1.join(), t2.join());

        assert_eq!((r_t1.unwrap(), r_t2.unwrap()), (FALLBACK_A, FALLBACK_B));
    });
}
