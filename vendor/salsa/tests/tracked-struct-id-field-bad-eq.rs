#![cfg(feature = "inventory")]

//! Test an id field whose `PartialEq` impl is always true.

use salsa::{Database, Setter};
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: bool,
}

#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Eq, Hash, Debug, Clone, salsa::SalsaValue)]
struct BadEq {
    field: bool,
}

impl PartialEq for BadEq {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl From<bool> for BadEq {
    fn from(value: bool) -> Self {
        Self { field: value }
    }
}

#[salsa::tracked]
struct MyTracked<'db> {
    field: BadEq,
}

#[salsa::tracked(returns(copy))]
fn the_fn(db: &dyn Database, input: MyInput) {
    let tracked0 = MyTracked::new(db, BadEq::from(input.field(db)));
    assert_eq!(tracked0.field(db).field, input.field(db));
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, true);
    the_fn(&db, input);
    input.set_field(&mut db).to(false);
    the_fn(&db, input);
}
