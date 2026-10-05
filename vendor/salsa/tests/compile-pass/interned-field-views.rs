mod model {
    #[salsa::interned(constructor = make, field_view = raw_fields)]
    pub struct Text<'db> {
        #[get(text)]
        #[returns(deref)]
        pub label: String,
        #[returns(copy)]
        pub(super) read_fields: u32,
        hidden: bool,
    }

    pub(super) fn hidden<'db>(fields: salsa::FieldReads<'db>, value: Text<'db>) -> &'db bool {
        value.raw_fields(fields).hidden()
    }

    #[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, field_view = read_fields)]
    pub struct Immortal {
        #[returns(clone)]
        pub text: String,
    }

    #[salsa::interned]
    pub struct Unopted<'db> {
        pub value: u32,
    }

    impl<'db> Unopted<'db> {
        pub fn read_fields(self, db: &'db dyn salsa::Database) -> &'db u32 {
            self.value(db)
        }
    }
}

fn text<'db>(db: &'db dyn salsa::Database, value: model::Text<'db>) -> &'db String {
    let fields = salsa::FieldReads::new(db);
    let view = value.raw_fields(fields);
    view.text()
}

fn immortal_text<'db>(db: &'db dyn salsa::Database, value: model::Immortal) -> &'db String {
    let fields = salsa::FieldReads::new(db);
    let view = value.read_fields(fields);
    view.text()
}

fn main() {
    let db = salsa::DatabaseImpl::new();
    let value = model::Text::make(&db, String::from("text"), 7u32, true);
    let ordinary: &str = value.text(&db);
    let raw: &String = text(&db, value);
    assert!(std::ptr::eq(raw.as_str(), ordinary));
    assert_eq!(value.read_fields(&db), 7);
    assert_eq!(
        *value.raw_fields(salsa::FieldReads::new(&db)).read_fields(),
        7
    );
    assert!(*model::hidden(salsa::FieldReads::new(&db), value));

    let immortal = model::Immortal::new(&db, String::from("immortal"));
    let ordinary: String = immortal.text(&db);
    let raw: &String = immortal_text(&db, immortal);
    assert_eq!(*raw, ordinary);
    let unopted = model::Unopted::new(&db, 11u32);
    assert_eq!(*unopted.read_fields(&db), 11);
}
