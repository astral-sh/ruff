#![cfg(feature = "inventory")]

//! Tests that fields attributed with `#[default]` are initialized with `Default::default()`.

use salsa::Durability;
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    required: bool,
    #[default]
    #[returns(copy)]
    optional: usize,
}

#[test]
fn new_constructor() {
    let db = salsa::DatabaseImpl::new();

    let input = MyInput::new(&db, true);

    assert!(input.required(&db));
    assert_eq!(input.optional(&db), 0);
}

#[test]
fn builder_specify_optional() {
    let db = salsa::DatabaseImpl::new();

    let input = MyInput::builder(true).optional(20).new(&db);

    assert!(input.required(&db));
    assert_eq!(input.optional(&db), 20);
}

#[test]
fn builder_default_optional_value() {
    let db = salsa::DatabaseImpl::new();

    let input = MyInput::builder(true)
        .required_durability(Durability::HIGH)
        .new(&db);

    assert!(input.required(&db));
    assert_eq!(input.optional(&db), 0);
}
