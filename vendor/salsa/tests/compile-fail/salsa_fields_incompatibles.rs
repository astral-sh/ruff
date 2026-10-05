// Banned field name: `from`
#[salsa::input]
struct InputWithBannedName1 {
    from: u32,
}

// Banned field name: `new`
#[salsa::input]
struct InputWithBannedName2 {
    new: u32,
}

fn main() {}
