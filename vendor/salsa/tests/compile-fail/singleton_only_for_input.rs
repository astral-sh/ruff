//! Compile Singleton struct test:
//!
//! Singleton flags are only allowed for input structs. If applied on any other Salsa struct compilation must fail

#[salsa::input(singleton)]
struct MyInput {
    #[returns(copy)]
    field: u32,
}

#[salsa::tracked(singleton)]
struct MyTracked<'db> {
    field: &'db str,
}

#[salsa::interned(singleton)]
struct MyInterned<'db> {
    field: &'db str,
}

#[salsa::tracked(singleton)]
fn tracked_fn(db: &dyn salsa::Database, input: MyInput) -> u32 {
    input.field(db)
}

#[salsa::accumulator(singleton)]
struct Integers(u32);

fn main() {}
