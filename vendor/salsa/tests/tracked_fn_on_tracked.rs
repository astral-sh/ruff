#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::input`
//! compiles and executes successfully.

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
fn tracked_fn(db: &dyn salsa::Database, input: MyInput) -> MyTracked<'_> {
    MyTracked::new(db, input.field(db) * 2)
}

#[test]
fn execute() {
    let db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    assert_eq!(tracked_fn(&db, input).field(&db), 44);
}
