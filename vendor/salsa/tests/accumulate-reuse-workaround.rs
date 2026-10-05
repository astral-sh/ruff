#![cfg(all(feature = "inventory", feature = "accumulator"))]

//! Demonstrates the workaround of wrapping calls to
//! `accumulated` in a tracked function to get better
//! reuse.

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
#[derive(Copy, Clone, Debug)]
struct Integers(u32);

#[salsa::tracked(returns(copy))]
fn compute(db: &dyn LogDatabase, input: List) -> u32 {
    db.push_log(format!("compute({input:?})",));

    // always pushes 0
    Integers(0).accumulate(db);

    // return value changes
    if let Some(next) = input.next(db) {
        let next_integers = accumulated(db, next);
        input.value(db) + next_integers.iter().sum::<u32>()
    } else {
        input.value(db)
    }
}

#[salsa::tracked(returns(ref))]
fn accumulated(db: &dyn LogDatabase, input: List) -> Vec<u32> {
    db.push_log(format!("accumulated({input:?})"));
    compute::accumulated::<Integers>(db, input)
        .into_iter()
        .map(|a| a.0)
        .collect()
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
            "accumulated(List { [salsa id]: Id(0), value: 1, next: None })",
            "compute(List { [salsa id]: Id(0), value: 1, next: None })",
        ]"#]]);

    // When we mutate `l1`, we should re-execute `compute` for `l1`,
    // and we re-execute accumulated for `l1`, but we do NOT re-execute
    // `compute` for `l2`.
    l1.set_value(&mut db).to(2);
    assert_eq!(compute(&db, l2), 2);
    db.assert_logs(expect![[r#"
        [
            "accumulated(List { [salsa id]: Id(0), value: 2, next: None })",
            "compute(List { [salsa id]: Id(0), value: 2, next: None })",
        ]"#]]);
}
