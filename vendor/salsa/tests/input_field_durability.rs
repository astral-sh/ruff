#![cfg(feature = "inventory")]

//! Tests that code using the builder's durability methods compiles.

use salsa::Durability;
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    required_field: bool,

    #[default]
    #[returns(copy)]
    optional_field: usize,
}

#[test]
fn required_field_durability() {
    let db = salsa::DatabaseImpl::new();

    let input = MyInput::builder(true)
        .required_field_durability(Durability::HIGH)
        .new(&db);

    assert!(input.required_field(&db));
    assert_eq!(input.optional_field(&db), 0);
}

#[test]
fn optional_field_durability() {
    let db = salsa::DatabaseImpl::new();

    let input = MyInput::builder(true)
        .optional_field(20)
        .optional_field_durability(Durability::HIGH)
        .new(&db);

    assert!(input.required_field(&db));
    assert_eq!(input.optional_field(&db), 20);
}
