#![cfg(feature = "inventory")]

use salsa::plumbing::ZalsaDatabase;
use salsa::{Durability, Setter};
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
fn execute() {
    let mut db = salsa::DatabaseImpl::new();

    let input = MyInput::builder(true)
        .required_field_durability(Durability::HIGH)
        .new(&db);

    // Change the field value. It should preserve high durability.
    input.set_required_field(&mut db).to(false);

    let last_high_revision = db.zalsa().last_changed_revision(Durability::HIGH);

    // Changing the value again should **again** dump the high durability revision.
    input.set_required_field(&mut db).to(false);

    assert_ne!(
        db.zalsa().last_changed_revision(Durability::HIGH),
        last_high_revision
    );
}
