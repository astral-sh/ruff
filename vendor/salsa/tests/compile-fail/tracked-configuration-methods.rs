#[salsa::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[salsa::tracked]
impl Input {
    #[salsa::tracked(returns(copy), configuration = (MethodConfiguration))]
    fn method(self, db: &dyn salsa::Database) -> u32 {
        self.value(db)
    }
}

#[salsa::tracked]
impl Input {
    #[salsa::tracked(returns(copy), configuration = (AssociatedConfiguration))]
    fn associated(db: &dyn salsa::Database, value: Self) -> u32 {
        value.value(db)
    }
}

fn main() {}
