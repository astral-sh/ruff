#[salsa::tracked(persist)]
struct Persistable<'db> {
    field: NotPersistable<'db>,
}

#[salsa::tracked]
struct NotPersistable<'db> {
    field: usize,
}

#[salsa::tracked(persist)]
fn query(_db: &dyn salsa::Database, _input: NotPersistable<'_>) {}

fn main() {}
