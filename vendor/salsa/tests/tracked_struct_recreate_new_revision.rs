#![cfg(feature = "inventory")]

//! Test that re-creating a `tracked` struct after it was deleted in a previous
//! revision doesn't panic.
#![allow(warnings)]

use salsa::Setter;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(debug)]
struct TrackedStruct<'db> {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn salsa::Database, input: MyInput) -> Option<TrackedStruct<'_>> {
    if input.field(db) == 1 {
        Some(TrackedStruct::new(db, 1))
    } else {
        None
    }
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 1);
    assert!(tracked_fn(&db, input).is_some());
    input.set_field(&mut db).to(0);
    assert_eq!(tracked_fn(&db, input), None);
    input.set_field(&mut db).to(1);
    assert!(tracked_fn(&db, input).is_some());
}
