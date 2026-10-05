#![cfg(feature = "inventory")]

//! Test that `specify` only works if the key is a tracked struct created in the current query.
//! compilation succeeds but execution panics
#![allow(warnings)]

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
fn tracked_struct_created_in_another_query<'db>(
    db: &'db dyn salsa::Database,
    input: MyInput,
) -> MyTracked<'db> {
    MyTracked::new(db, input.field(db) * 2)
}

#[salsa::tracked(returns(copy))]
fn tracked_fn<'db>(db: &'db dyn salsa::Database, input: MyInput) -> MyTracked<'db> {
    let t = tracked_struct_created_in_another_query(db, input);
    if input.field(db) != 0 {
        tracked_fn_extra::specify(db, t, 2222);
    }
    t
}

#[salsa::tracked(returns(copy), specify)]
fn tracked_fn_extra<'db>(_db: &'db dyn salsa::Database, _input: MyTracked<'db>) -> u32 {
    0
}

#[test]
#[should_panic(
    expected = "can only use `specify` on salsa structs created during the current tracked fn"
)]
fn execute_when_specified() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    let tracked = tracked_fn(&db, input);
}
