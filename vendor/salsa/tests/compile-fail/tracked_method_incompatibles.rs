#[salsa::tracked]
struct Tracked<'db> {
    field: u32,
}

#[salsa::tracked]
impl<'db> Tracked<'db> {
    #[salsa::tracked]
    fn ref_self(&self, db: &dyn salsa::Database) {}
}

#[salsa::tracked]
impl<'db> Tracked<'db> {
    #[salsa::tracked]
    fn ref_mut_self(&mut self, db: &dyn salsa::Database) {}
}

#[salsa::tracked]
impl<'db> Tracked<'db> {
    #[salsa::tracked]
    fn multiple_lifetimes<'db1>(&mut self, db: &'db1 dyn salsa::Database) {}
}

#[salsa::tracked]
impl<'db> Tracked<'db> {
    #[salsa::tracked]
    fn type_generics<T>(&mut self, db: &dyn salsa::Database) -> T {
        panic!()
    }
}

fn main() {}
