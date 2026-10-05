#[salsa::tracked]
pub struct MyTracked<'db> {
    field: u32,
}

#[salsa::tracked]
fn my_fn(db: &dyn salsa::Database) {
    let x = MyTracked::new(db, 22);
    x.field(22);
}

fn main() {
    let mut db = salsa::DatabaseImpl::new();
    my_fn(&db);
}
