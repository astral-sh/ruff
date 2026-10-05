#![cfg(feature = "inventory")]

use salsa::{Database as Db, Setter};
use test_log::test;

#[salsa::input]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked]
struct MyTracked<'db> {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn Db, input: MyInput) -> MyTracked<'_> {
    MyTracked::new(db, input.field(db) / 2)
}

#[test]
fn execute() {
    let mut db = salsa::DatabaseImpl::new();

    let input1 = MyInput::new(&db, 22);
    let input2 = MyInput::new(&db, 44);
    let _tracked1 = tracked_fn(&db, input1);
    let _tracked2 = tracked_fn(&db, input2);

    // modify the input and change the revision
    input1.set_field(&mut db).to(24);
    let tracked2 = tracked_fn(&db, input2);

    // this should not panic
    tracked2.field(&db);
}
