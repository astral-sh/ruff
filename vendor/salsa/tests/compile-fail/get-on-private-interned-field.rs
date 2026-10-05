mod a {
    #[salsa::interned]
    pub struct MyInterned<'db> {
        field: u32,
    }
}

fn test<'db>(db: &'db dyn salsa::Database, interned: a::MyInterned<'db>) {
    interned.field(db);
}

fn main() {}
