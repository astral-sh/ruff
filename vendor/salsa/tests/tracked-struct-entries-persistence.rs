#![cfg(all(feature = "inventory", feature = "persistence"))]

use salsa::Setter;

const DELETED_VALUE: &str = "deleted tracked entry sentinel";

#[salsa::input(persist)]
struct Input {
    #[returns(copy)]
    enabled: bool,
}

#[salsa::tracked(persist)]
struct Entity<'db> {
    value: String,
}

#[salsa::tracked(returns(copy), persist)]
fn maybe_entity(db: &dyn salsa::Database, input: Input) -> Option<Entity<'_>> {
    input
        .enabled(db)
        .then(|| Entity::new(db, DELETED_VALUE.to_owned()))
}

#[salsa::tracked(returns(copy), persist)]
fn consume(db: &dyn salsa::Database, entity: Entity<'_>) -> usize {
    entity.value(db).len()
}

#[test]
fn deleted_tracked_structs_are_not_persisted() {
    let mut db = salsa::DatabaseImpl::default();
    let input = Input::new(&db, true);
    assert!(maybe_entity(&db, input).is_some());

    input.set_enabled(&mut db).to(false);
    assert!(maybe_entity(&db, input).is_none());

    let serialized = serde_json::to_string(&<dyn salsa::Database>::as_serialize(&mut db)).unwrap();
    assert!(!serialized.contains(DELETED_VALUE));
}
