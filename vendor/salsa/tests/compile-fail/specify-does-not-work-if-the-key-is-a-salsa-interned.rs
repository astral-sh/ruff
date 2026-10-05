//! Test that `specify` does not work if the key is a `salsa::interned`
//! compilation fails
#![allow(warnings)]

#[salsa::interned]
struct MyInterned<'db> {
    field: u32,
}

#[salsa::tracked]
struct MyTracked<'db> {
    field: u32,
}

#[salsa::tracked(specify)]
fn tracked_fn<'db>(db: &'db dyn salsa::Database, input: MyInterned<'db>) -> MyTracked<'db> {
    MyTracked::new(db, input.field(db) * 2)
}

fn main() {}
