#![cfg(feature = "inventory")]

//! Test that a setting a field on a `#[salsa::input]`
//! overwrites and returns the old value.

use test_log::test;

#[salsa::input]
struct MyInput {
    field: String,
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[tracked]
    #[returns(copy)]
    field: MyInterned<'db>,
}

#[salsa::interned]
struct MyInterned<'db> {
    field: String,
}

#[test]
fn execute() {}
