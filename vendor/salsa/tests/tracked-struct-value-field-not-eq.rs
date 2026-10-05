#![cfg(feature = "inventory")]

//! Test a field whose `PartialEq` impl is always true.
//! This can our "last changed" data to be wrong
//! but we *should* always reflect the final values.

use salsa::{Database, Setter};
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: bool,
}

#[derive(Hash, Debug, Clone, salsa::SalsaValue)]
struct NotEq {
    field: bool,
}

impl From<bool> for NotEq {
    fn from(value: bool) -> Self {
        Self { field: value }
    }
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[tracked]
    #[no_eq]
    field: NotEq,
}

#[salsa::tracked(returns(copy))]
fn the_fn(db: &dyn Database, input: MyInput) {
    let tracked0 = MyTracked::new(db, NotEq::from(input.field(db)));
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
