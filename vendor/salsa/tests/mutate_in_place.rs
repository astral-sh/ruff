#![cfg(feature = "inventory")]

//! Test that a setting a field on a `#[salsa::input]`
//! overwrites and returns the old value.

use salsa::Setter;
use test_log::test;

#[salsa::input]
struct MyInput {
    field: String,
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();

    let input = MyInput::new(&db, "Hello".to_string());

    // Overwrite field with an empty String
    // and store the old value in my_string
    let mut my_string = input.set_field(&mut db).to(String::new());
    my_string.push_str(" World!");

    // Set the field back to out initial String,
    // expecting to get the empty one back
    assert_eq!(input.set_field(&mut db).to(my_string), "");

    // Check if the stored String is the one we expected
    assert_eq!(input.field(&db), "Hello World!");
}
