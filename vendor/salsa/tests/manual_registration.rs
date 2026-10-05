#![cfg(not(feature = "inventory"))]

mod ingredients {
    #[salsa::input]
    pub(super) struct MyInput {
        #[returns(copy)]
        field: u32,
    }

    #[salsa::tracked]
    pub(super) struct MyTracked<'db> {
        #[returns(copy)]
        pub(super) field: u32,
    }

    #[salsa::interned(field_view = read_fields)]
    pub(super) struct MyInterned<'db> {
        #[returns(copy)]
        pub(super) field: u32,
    }

    #[salsa::tracked(returns(copy))]
    pub(super) fn intern<'db>(db: &'db dyn salsa::Database, input: MyInput) -> MyInterned<'db> {
        MyInterned::new(db, input.field(db))
    }

    #[salsa::tracked(returns(copy))]
    pub(super) fn track<'db>(db: &'db dyn salsa::Database, input: MyInput) -> MyTracked<'db> {
        MyTracked::new(db, input.field(db))
    }
}

#[salsa::db]
#[derive(Clone, Default)]
pub struct DatabaseImpl {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for DatabaseImpl {}

#[test]
fn single_database() {
    let db = DatabaseImpl {
        storage: salsa::Storage::builder()
            .ingredient::<ingredients::track>()
            .ingredient::<ingredients::intern>()
            .ingredient::<ingredients::MyInput>()
            .ingredient::<ingredients::MyTracked<'_>>()
            .ingredient::<ingredients::MyInterned<'_>>()
            .build(),
    };

    let input = ingredients::MyInput::new(&db, 1);

    let tracked = ingredients::track(&db, input);
    let interned = ingredients::intern(&db, input);

    assert_eq!(tracked.field(&db), 1);
    assert_eq!(interned.field(&db), 1);
}

#[test]
fn multiple_databases() {
    let db1 = DatabaseImpl {
        storage: salsa::Storage::builder()
            .ingredient::<ingredients::intern>()
            .ingredient::<ingredients::MyInput>()
            .ingredient::<ingredients::MyInterned<'_>>()
            .build(),
    };

    let input = ingredients::MyInput::new(&db1, 1);
    let interned = ingredients::intern(&db1, input);
    assert_eq!(interned.field(&db1), 1);
    let first = interned;
    let first_raw = first.read_fields(salsa::FieldReads::new(&db1)).field();
    assert_eq!(*first_raw, 1);

    // Create a second database with different ingredient indices.
    let db2 = DatabaseImpl {
        storage: salsa::Storage::builder()
            .ingredient::<ingredients::track>()
            .ingredient::<ingredients::intern>()
            .ingredient::<ingredients::MyInput>()
            .ingredient::<ingredients::MyTracked<'_>>()
            .ingredient::<ingredients::MyInterned<'_>>()
            .build(),
    };

    let input = ingredients::MyInput::new(&db2, 2);
    let interned = ingredients::intern(&db2, input);
    assert_eq!(interned.field(&db2), 2);
    let second_raw = interned.read_fields(salsa::FieldReads::new(&db2)).field();
    assert_eq!(*second_raw, 2);
    assert!(std::ptr::eq(
        first_raw,
        first.read_fields(salsa::FieldReads::new(&db1)).field(),
    ));
    assert!(std::ptr::eq(
        second_raw,
        interned.read_fields(salsa::FieldReads::new(&db2)).field(),
    ));

    let input = ingredients::MyInput::new(&db2, 3);
    let tracked = ingredients::track(&db2, input);
    assert_eq!(tracked.field(&db2), 3);
}
