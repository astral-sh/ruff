mod model {
    #[salsa::interned(field_view = read_fields)]
    pub struct Value<'db> {
        pub visible: u32,
        hidden: u32,
    }

    pub(super) fn hidden<'db>(fields: salsa::FieldReads<'db>, value: Value<'db>) -> &'db u32 {
        value.read_fields(fields).hidden()
    }
}

fn main() {
    let db = salsa::DatabaseImpl::new();
    let value = model::Value::new(&db, 1u32, 2u32);
    let fields = salsa::FieldReads::new(&db);
    let _: &u32 = model::hidden(fields, value);
    let _: &u32 = value.read_fields(fields).visible();
    let _: &u32 = value.read_fields(fields).hidden();
}
