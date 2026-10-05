#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.
#![allow(warnings)]

use std::marker::PhantomData;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NotSalsaValue<'a>(PhantomData<fn() -> &'a ()>);

#[salsa::tracked(returns(copy), unsafe(non_salsa_values))]
fn tracked_fn(db: &dyn salsa::Database, input: MyInput) -> NotSalsaValue<'_> {
    NotSalsaValue(PhantomData)
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    tracked_fn(&db, input);
}
