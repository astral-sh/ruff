//! Test a nested-cycle scenario across three threads:
//!
//! ```text
//! Thread T1          Thread T2         Thread T3
//! ---------          ---------         ---------
//!    |                  |                  |
//!    v                  |                  |
//! query_a()             |                  |
//!  ^  |                 v                  |
//!  |  +------------> query_b()             |
//!  |                  ^   |                v
//!  |                  |   +------------> query_c()
//!  |                  |                    |
//!  +------------------+--------------------+
//! ```
//!
//! Specifically, the maybe_changed_after flow.

use crate::sync;
use crate::sync::thread;

use salsa::{DatabaseImpl, Setter as _};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
struct CycleValue(u32);

const MIN: CycleValue = CycleValue(0);
const MAX: CycleValue = CycleValue(3);

#[salsa::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

// Signal 1: T1 has entered `query_a`
// Signal 2: T2 has entered `query_b`
// Signal 3: T3 has entered `query_c`

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_a(db: &dyn salsa::Database, input: Input) -> CycleValue {
    query_b(db, input)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_b(db: &dyn salsa::Database, input: Input) -> CycleValue {
    let c_value = query_c(db, input);
    CycleValue(c_value.0 + input.value(db)).min(MAX)
}

#[salsa::tracked(returns(copy), cycle_initial=initial)]
fn query_c(db: &dyn salsa::Database, input: Input) -> CycleValue {
    let a_value = query_a(db, input);
    let b_value = query_b(db, input);
    CycleValue(a_value.0.max(b_value.0))
}

fn initial(_db: &dyn salsa::Database, _id: salsa::Id, _input: Input) -> CycleValue {
    MIN
}

#[test_log::test]
fn the_test() {
    crate::sync::check(move || {
        // This is a bit silly but it works around https://github.com/awslabs/shuttle/issues/192
        static INITIALIZE: sync::Mutex<Option<(salsa::DatabaseImpl, Input)>> =
            sync::Mutex::new(None);

        fn get_db(f: impl FnOnce(&salsa::DatabaseImpl, Input)) -> (salsa::DatabaseImpl, Input) {
            let mut shared = INITIALIZE.lock().unwrap();

            if let Some((db, input)) = shared.as_ref() {
                return (db.clone(), *input);
            }

            let mut db = DatabaseImpl::default();

            let input = Input::new(&db, 1);

            f(&db, input);

            input.set_value(&mut db).to(2);

            *shared = Some((db.clone(), input));

            (db, input)
        }

        let t1 = thread::spawn(|| {
            let (db, input) = get_db(|db, input| {
                query_a(db, input);
            });

            query_a(&db, input)
        });
        let t2 = thread::spawn(|| {
            let (db, input) = get_db(|db, input| {
                query_b(db, input);
            });
            query_b(&db, input)
        });
        let t3 = thread::spawn(|| {
            let (db, input) = get_db(|db, input| {
                query_c(db, input);
            });
            query_c(&db, input)
        });

        let r_t1 = t1.join().unwrap();
        let r_t2 = t2.join().unwrap();
        let r_t3 = t3.join().unwrap();

        assert_eq!((r_t1, r_t2, r_t3), (MAX, MAX, MAX));
    });
}
