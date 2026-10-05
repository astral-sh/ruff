use salsa::prelude::*;

#[salsa::input]
struct MyInput {
    field: u32,
}

#[salsa::tracked]
struct MyTracked<'db> {
    field: u32,
}

#[salsa::tracked]
fn tracked_fn<'db>(db: &'db dyn salsa::Database, input: MyInput) -> MyTracked<'db> {
    MyTracked::new(db, input.field(db) / 2)
}

fn main() {
    let mut db = salsa::DatabaseImpl::new();
    let input = MyInput::new(&db, 22);
    let tracked = tracked_fn(&db, input);
    input.set_field(&mut db).to(24);
    tracked.field(&db); // tracked comes from prior revision
}
