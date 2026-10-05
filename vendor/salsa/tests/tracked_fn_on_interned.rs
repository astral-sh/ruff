#![cfg(feature = "inventory")]

//! Test that a `tracked` fn on a `salsa::interned`
//! compiles and executes successfully.

#[salsa::interned]
struct Name<'db> {
    name: String,
}

#[salsa::tracked(returns(clone))]
fn tracked_fn<'db>(db: &'db dyn salsa::Database, name: Name<'db>) -> String {
    name.name(db).clone()
}

#[test]
fn execute() {
    let db = salsa::DatabaseImpl::new();
    let name = Name::new(&db, "Salsa".to_string());

    assert_eq!(tracked_fn(&db, name), "Salsa");
}
