#[salsa::interned(field_view = read_fields, field_view = raw_fields)]
struct Duplicate {
    value: u32,
}

#[salsa::input(field_view = read_fields)]
struct Input {
    value: u32,
}

#[salsa::tracked(field_view = read_fields)]
struct Tracked {
    value: u32,
}

#[salsa::tracked(field_view = read_fields)]
fn query(db: &dyn salsa::Database) -> u32 {
    let _ = db;
    0
}

fn main() {}
