#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Accumulator re-use test.
//!
//! Tests behavior when a query's only inputs
//! are the accumulated values from another query.

mod common;
use common::{LogDatabase, LoggerDatabase};
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

#[salsa::accumulator]
struct Integers(u32);

#[salsa::tracked(returns(copy))]
fn compute(db: &dyn LogDatabase, input: List) -> u32 {
    db.push_log(format!("compute({input:?})",));

    // always pushes 0
    Integers(0).accumulate(db);

    // return value changes
    if let Some(next) = input.next(db) {
        let next_integers = compute::accumulated::<Integers>(db, next);
        input.value(db) + next_integers.iter().map(|i| i.0).sum::<u32>()
    } else {
        input.value(db)
    }
}

#[test]
fn test1() {
    let mut db = LoggerDatabase::default();

    let l1 = List::new(&db, 1, None);
    let l2 = List::new(&db, 2, Some(l1));

    assert_eq!(compute(&db, l2), 2);
    db.assert_logs(expect![[r#"
        [
            "compute(List { [salsa id]: Id(1), value: 2, next: Some(List { [salsa id]: Id(0), value: 1, next: None }) })",
            "compute(List { [salsa id]: Id(0), value: 1, next: None })",
        ]"#]]);

    // When we mutate `l1`, we should re-execute `compute` for `l1`,
    // but we should not have to re-execute `compute` for `l2`.
    // The only input for `compute(l1)` is the accumulated values from `l1`,
    // which have not changed.
    l1.set_value(&mut db).to(2);
    assert_eq!(compute(&db, l2), 2);
    db.assert_logs(expect![[r#"
        [
            "compute(List { [salsa id]: Id(1), value: 2, next: Some(List { [salsa id]: Id(0), value: 2, next: None }) })",
            "compute(List { [salsa id]: Id(0), value: 2, next: None })",
        ]"#]]);
}
