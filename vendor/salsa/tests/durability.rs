#![cfg(feature = "inventory")]

//! Tests that code using the builder's durability methods compiles.

use salsa::{Database, Durability, Setter};
use test_log::test;

#[salsa::input]
struct N {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked(returns(copy))]
fn add3(db: &dyn Database, a: N, b: N, c: N) -> u32 {
    add(db, a, b) + c.value(db)
}

#[salsa::tracked(returns(copy))]
fn add(db: &dyn Database, a: N, b: N) -> u32 {
    a.value(db) + b.value(db)
}

#[test]
fn durable_to_less_durable() {
    let mut db = salsa::DatabaseImpl::new();

    let a = N::builder(11).value_durability(Durability::HIGH).new(&db);
    let b = N::builder(22).value_durability(Durability::HIGH).new(&db);
    let c = N::builder(33).value_durability(Durability::HIGH).new(&db);

    // Here, `add3` invokes `add(a, b)`, which yields 33.
    assert_eq!(add3(&db, a, b, c), 66);

    a.set_value(&mut db).with_durability(Durability::LOW).to(11);

    // Here, `add3` invokes `add`, which *still* yields 33, but which
    // is no longer of high durability. Since value didn't change, we might
    // preserve `add3` unchanged, not noticing that it is no longer
    // of high durability.

    assert_eq!(add3(&db, a, b, c), 66);

    // In that case, we would not get the correct result here, when
    // 'a' changes *again*.

    a.set_value(&mut db).to(22);

    assert_eq!(add3(&db, a, b, c), 77);
}
