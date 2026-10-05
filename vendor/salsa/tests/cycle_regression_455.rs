#![cfg(feature = "inventory")]

use salsa::{Database, Setter};

#[salsa::tracked(returns(copy))]
fn memoized(db: &dyn Database, input: MyInput) -> u32 {
    memoized_a(db, MyTracked::new(db, input.field(db)))
}

#[salsa::tracked(returns(copy), cycle_initial=cycle_initial)]
fn memoized_a<'db>(db: &'db dyn Database, tracked: MyTracked<'db>) -> u32 {
    MyTracked::new(db, 0);
    memoized_b(db, tracked)
}
fn cycle_initial(_db: &dyn Database, _id: salsa::Id, _input: MyTracked) -> u32 {
    0
}

#[salsa::tracked(returns(copy))]
fn memoized_b<'db>(db: &'db dyn Database, tracked: MyTracked<'db>) -> u32 {
    let incr = tracked.field(db);
    let a = memoized_a(db, tracked);
    if a > 8 { a } else { a + incr }
}

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[returns(copy)]
    field: u32,
}

#[test]
fn cycle_memoized() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 2);
    assert_eq!(memoized(&db, input), 10);
    input.set_field(&mut db).to(3);
    assert_eq!(memoized(&db, input), 9);
}
