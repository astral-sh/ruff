#[salsa::tracked]
struct MyInput<'db> {
    field: NotPartialEq,
}

#[derive(Clone, Debug, Hash, salsa::SalsaValue)]
struct NotPartialEq;

fn main() {}
