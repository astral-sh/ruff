#![cfg(feature = "inventory")]

#[salsa::interned]
struct Interned<'db> {
    #[returns(copy)]
    field: i32,
}

#[salsa::tracked(returns(copy))]
fn foo<'a>(_db: &'a dyn salsa::Database, _: Interned<'_>, _: Interned<'a>) {}

#[test]
fn the_test() {
    let db = salsa::DatabaseImpl::new();
    let i = Interned::new(&db, 123);
    foo(&db, i, i);
}
