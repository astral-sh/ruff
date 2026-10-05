#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Regression test for https://github.com/salsa-rs/salsa/issues/923.
//!
//! Accumulated values were silently dropped when a tracked fn that calls
//! another tracked fn (which does the actual accumulating) is re-used
//! without re-executing (its inputs haven't changed).

use expect_test::expect;
use salsa::{Accumulator, Setter};
use test_log::test;

#[salsa::input(debug)]
struct List {
    #[returns(copy)]
    value: u32,
    #[returns(copy)]
    next: Option<List>,
}

#[allow(unused)]
#[salsa::accumulator]
#[derive(Copy, Clone, Debug)]
struct Integers(u32);

/// Outer tracked fn: iterates the list and delegates accumulation to `compute_single`.
#[salsa::tracked(returns(copy))]
fn compute(db: &dyn salsa::Database, input: List) {
    compute_single(db, input);
    if let Some(next) = input.next(db) {
        compute(db, next);
    }
}

/// Inner tracked fn: performs the actual accumulation.
#[salsa::tracked(returns(copy))]
fn compute_single(db: &dyn salsa::Database, input: List) {
    Integers(input.value(db)).accumulate(db);
}

#[test]
fn accumulated_values_not_lost_after_partial_reuse() {
    let mut db = salsa::DatabaseImpl::new();

    let l0 = List::new(&db, 1, None);
    let l1 = List::new(&db, 10, Some(l0));

    compute(&db, l1);
    expect![[r#"
        [
            Integers(
                10,
            ),
            Integers(
                1,
            ),
        ]
    "#]]
    .assert_debug_eq(&compute::accumulated::<Integers>(&db, l1));

    // Mutate l1; l0 is unchanged so compute(l0) / compute_single(l0) should be reused.
    // Reused memos must still report their accumulated values so callers don't skip them.
    l1.set_value(&mut db).to(11);
    compute(&db, l1);
    expect![[r#"
        [
            Integers(
                11,
            ),
            Integers(
                1,
            ),
        ]
    "#]]
    .assert_debug_eq(&compute::accumulated::<Integers>(&db, l1));
}
