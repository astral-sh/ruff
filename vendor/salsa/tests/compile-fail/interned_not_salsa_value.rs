use salsa::Database as Db;

#[salsa::input]
struct MyInput {}

#[salsa::tracked]
fn tracked_fn<'db>(_db: &'db dyn Db, _: (), _: &'db str) {}

#[salsa::interned]
struct Interned<'db> {
    _field: &'db str,
}

fn main() {}
