use salsa::Database as Db;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct StaticField;

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticQueryResult;

#[derive(salsa::SalsaValue)]
struct ContainsStaticField {
    field: StaticField,
}

#[salsa::input]
struct MyInput {
    field: StaticField,
}

#[salsa::interned]
struct MyInterned<'db> {
    field: StaticField,
}

#[salsa::tracked]
struct MyTracked<'db> {
    field: StaticField,
}

#[salsa::tracked]
fn tracked_fn(db: &dyn Db, input: MyInput, _field: StaticField) -> StaticQueryResult {
    _ = input.field(db);
    StaticQueryResult
}

fn main() {
    let _ = ContainsStaticField { field: StaticField }.field;
}
