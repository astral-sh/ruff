mod a {
    #[salsa::tracked]
    pub struct MyTracked<'db> {
        field: u32,
    }
}

fn test<'db>(db: &'db dyn salsa::Database, tracked: a::MyTracked<'db>) {
    tracked.field(db);
}

fn main() {}
