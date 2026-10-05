#[derive(salsa::SalsaValue)]
struct ContainsRef<'db> {
    value: &'db str,
}

fn main() {}
