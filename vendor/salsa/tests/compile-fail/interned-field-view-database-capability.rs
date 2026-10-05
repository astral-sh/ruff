#[salsa::interned(field_view = read_fields)]
struct Value<'db> {
    #[returns(copy)]
    number: u32,
}

#[salsa::tracked(returns(copy))]
fn ordinary(db: &dyn salsa::Database, value: Value<'_>) -> u32 {
    value.number(db)
}

fn rejected<'db>(fields: salsa::FieldReads<'db>, value: Value<'db>) {
    let view = value.read_fields(fields);
    let _ = ordinary(&fields, value);
    let _ = Value::new(&fields, 3u32);
    let _ = ordinary(&view, value);
    let _ = Value::new(&view, 3u32);
    let _: &dyn salsa::Database = &*fields;
    let _ = fields.zalsa;
}

fn main() {
    let db = salsa::DatabaseImpl::new();
    let value = Value::new(&db, 3u32);
    assert_eq!(ordinary(&db, value), 3);
    rejected(salsa::FieldReads::new(&db), value);
}
