#![cfg(feature = "inventory")]

mod common;

use salsa::{Database, Setter};

#[salsa::tracked]
struct Tracked<'db> {
    #[returns(copy)]
    untracked_1: usize,

    #[returns(copy)]
    untracked_2: usize,
}

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field1: usize,
    #[returns(copy)]
    field2: usize,
}

#[salsa::tracked(returns(copy))]
fn intermediate(db: &dyn salsa::Database, input: MyInput) -> Tracked<'_> {
    Tracked::new(db, input.field1(db), input.field2(db))
}

#[salsa::tracked(returns(copy))]
fn accumulate(db: &dyn salsa::Database, input: MyInput) -> (usize, usize) {
    let tracked = intermediate(db, input);
    let one = read_tracked_1(db, tracked);
    let two = read_tracked_2(db, tracked);

    (one, two)
}

#[salsa::tracked(returns(copy))]
fn read_tracked_1<'db>(db: &'db dyn Database, tracked: Tracked<'db>) -> usize {
    tracked.untracked_1(db)
}

#[salsa::tracked(returns(copy))]
fn read_tracked_2<'db>(db: &'db dyn Database, tracked: Tracked<'db>) -> usize {
    tracked.untracked_2(db)
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::default();
    let input = MyInput::new(&db, 1, 1);

    assert_eq!(accumulate(&db, input), (1, 1));

    // Should only re-execute `read_tracked_1`.
    input.set_field1(&mut db).to(2);
    assert_eq!(accumulate(&db, input), (2, 1));

    // Should only re-execute `read_tracked_2`.
    input.set_field2(&mut db).to(2);
    assert_eq!(accumulate(&db, input), (2, 2));
}
