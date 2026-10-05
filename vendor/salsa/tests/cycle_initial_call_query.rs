#![cfg(feature = "inventory")]

//! It's possible to call a Salsa query from within a cycle initial fn.

#[salsa::tracked(returns(copy))]
fn initial_value(_db: &dyn salsa::Database) -> u32 {
    0
}

#[salsa::tracked(returns(copy), cycle_initial= |db, _| initial_value(db))]
fn query(db: &dyn salsa::Database) -> u32 {
    let val = query(db);
    if val < 5 { val + 1 } else { val }
}

#[test_log::test]
fn the_test() {
    let db = salsa::DatabaseImpl::default();

    assert_eq!(query(&db), 5);
}
