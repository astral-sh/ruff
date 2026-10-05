#![cfg(all(feature = "inventory", feature = "accumulator"))]

mod common;

use common::LogDatabase;
use salsa::{Accumulator, Setter};

#[salsa::input]
struct List {
    #[returns(copy)]
    value: u32,
    #[returns(copy)]
    next: Option<List>,
}

#[salsa::accumulator]
struct Values(u32);

#[salsa::tracked(returns(copy))]
fn compute_single(db: &dyn LogDatabase, input: List) {
    Values(input.value(db)).accumulate(db);
}

#[salsa::tracked(returns(copy))]
fn compute(db: &dyn LogDatabase, input: List) {
    compute_single(db, input);
    if let Some(next) = input.next(db) {
        compute(db, next);
    }
}

#[test]
fn shallow_verification_preserves_accumulated_values() {
    let mut db = common::LoggerDatabase::default();
    let tail = List::new(&db, 1, None);
    let head = List::new(&db, 10, Some(tail));

    let values = compute::accumulated::<Values>(&db, head);
    assert_eq!(
        values.iter().map(|value| value.0).collect::<Vec<_>>(),
        [10, 1]
    );

    head.set_value(&mut db).to(11);
    compute_single(&db, tail);

    let values = compute::accumulated::<Values>(&db, head);
    assert_eq!(
        values.iter().map(|value| value.0).collect::<Vec<_>>(),
        [11, 1]
    );
}
