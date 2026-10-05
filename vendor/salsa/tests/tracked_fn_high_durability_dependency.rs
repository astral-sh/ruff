#![cfg(feature = "inventory")]
#![allow(warnings)]

use salsa::plumbing::HasStorage;
use salsa::{Database, Durability, Setter};

mod common;
#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn salsa::Database, input: MyInput) -> u32 {
    input.field(db) * 2
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::default();

    let input_high = MyInput::new(&mut db, 0);
    input_high
        .set_field(&mut db)
        .with_durability(Durability::HIGH)
        .to(2200);

    assert_eq!(tracked_fn(&db, input_high), 4400);

    // Changing the value should re-execute the query
    input_high
        .set_field(&mut db)
        .with_durability(Durability::HIGH)
        .to(2201);

    assert_eq!(tracked_fn(&db, input_high), 4402);
}
