#![cfg(all(feature = "persistence", feature = "inventory"))]

mod common;

use salsa::Durability;

#[salsa::input(singleton)]
struct NonPersistedInput {
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked(returns(copy), persist)]
fn query(db: &dyn salsa::Database) -> usize {
    NonPersistedInput::get(db).value(db)
}

#[test]
#[should_panic(expected = "must be persistable")]
fn never_change_dependency_must_be_persistable() {
    let mut db = common::LoggerDatabase::default();
    let _ = NonPersistedInput::builder(0)
        .durability(Durability::NEVER_CHANGE)
        .new(&db);
    query(&db);

    let _serialized =
        serde_json::to_string_pretty(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();
}
