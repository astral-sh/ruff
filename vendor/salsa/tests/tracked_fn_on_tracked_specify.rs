#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.
#![allow(warnings)]

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

#[salsa::tracked(returns(copy))]
fn tracked_fn<'db>(db: &'db dyn salsa::Database, input: MyInput) -> MyTracked<'db> {
    let t = MyTracked::new(db, input.field(db) * 2);
    if input.field(db) != 0 {
        tracked_fn_extra::specify(db, t, 2222);
    }
    t
}

#[salsa::tracked(returns(copy), specify)]
fn tracked_fn_extra<'db>(_db: &'db dyn salsa::Database, _input: MyTracked<'db>) -> u32 {
    0
}

#[test]
fn execute_when_specified() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    let tracked = tracked_fn(&db, input);
    assert_eq!(tracked.field(&db), 44);
    assert_eq!(tracked_fn_extra(&db, tracked), 2222);
}

#[test]
fn execute_when_not_specified() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 0);
    let tracked = tracked_fn(&db, input);
    assert_eq!(tracked.field(&db), 0);
    assert_eq!(tracked_fn_extra(&db, tracked), 0);
}
