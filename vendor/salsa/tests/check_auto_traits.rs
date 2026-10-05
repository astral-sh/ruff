#![cfg(feature = "inventory")]

//! Test that auto trait impls exist as expected.

use std::panic::UnwindSafe;

use salsa::Database;
use test_log::test;

#[salsa::input]
struct MyInput {
    field: String,
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[returns(copy)]
    field: MyInterned<'db>,
}

#[salsa::interned]
struct MyInterned<'db> {
    field: String,
}

#[salsa::tracked(returns(copy))]
fn test(db: &dyn Database, input: MyInput) {
    let input = is_send(is_sync(input));
    let interned = is_send(is_sync(MyInterned::new(db, input.field(db).clone())));
    let _tracked_struct = is_send(is_sync(MyTracked::new(db, interned)));
}

fn is_send<T: Send>(t: T) -> T {
    t
}

fn is_sync<T: Send>(t: T) -> T {
    t
}

fn is_unwind_safe<T: UnwindSafe>(t: T) -> T {
    t
}

#[test]
fn execute() {
    let db = is_send(salsa::DatabaseImpl::new());
    let _handle = is_send(is_sync(is_unwind_safe(
        db.storage().clone().into_zalsa_handle(),
    )));
    let input = MyInput::new(&db, "Hello".to_string());
    test(&db, input);
}
