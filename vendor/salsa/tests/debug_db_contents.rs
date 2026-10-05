#![cfg(feature = "inventory")]

#[salsa::interned(debug)]
struct InternedStruct<'db> {
    name: String,
}

#[salsa::input(debug)]
struct InputStruct {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(debug)]
struct TrackedStruct<'db> {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(returns(copy))]
fn tracked_fn(db: &dyn salsa::Database, input: InputStruct) -> TrackedStruct<'_> {
    TrackedStruct::new(db, input.field(db) * 2)
}

#[test]
fn execute() {
    use salsa::plumbing::ZalsaDatabase;
    let db = salsa::DatabaseImpl::new();

    let interned1 = InternedStruct::new(&db, "Salsa".to_string());
    let interned2 = InternedStruct::new(&db, "Salsa2".to_string());

    // test interned structs
    let interned = InternedStruct::ingredient(db.zalsa())
        .entries(db.zalsa())
        .collect::<Vec<_>>();

    assert_eq!(interned.len(), 2);
    assert_eq!(interned[0].as_struct(), interned1);
    assert_eq!(interned[1].as_struct(), interned2);
    assert_eq!(interned[0].value().fields().0, "Salsa");
    assert_eq!(interned[1].value().fields().0, "Salsa2");

    // test input structs
    let input1 = InputStruct::new(&db, 22);

    let inputs = InputStruct::ingredient(&db)
        .entries(db.zalsa())
        .collect::<Vec<_>>();

    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].as_struct(), input1);
    assert_eq!(inputs[0].value().fields().0, 22);

    // test tracked structs
    let tracked1 = tracked_fn(&db, input1);
    assert_eq!(tracked1.field(&db), 44);

    let tracked = TrackedStruct::ingredient(&db)
        .entries(db.zalsa())
        .collect::<Vec<_>>();

    assert_eq!(tracked.len(), 1);
    assert_eq!(tracked[0].as_struct(), tracked1);
    assert_eq!(tracked[0].value().fields().0, tracked1.field(&db));
}
