#[salsa::input(configuration = (InputConfiguration))]
struct Input {
    value: u32,
}

#[salsa::interned(configuration = (InternedConfiguration))]
struct Interned<'db> {
    value: u32,
}

#[salsa::tracked(configuration = (StructConfiguration))]
struct Tracked<'db> {
    value: &'db str,
}

#[salsa::tracked(configuration = (First), configuration = (Second))]
fn duplicate(_db: &dyn salsa::Database) -> u32 {
    0
}

#[salsa::tracked(configuration = (pub))]
fn missing_name(_db: &dyn salsa::Database) -> u32 {
    0
}

#[salsa::tracked(configuration = (Name extra))]
fn extra_tokens(_db: &dyn salsa::Database) -> u32 {
    0
}

mod outer {
    mod inner {
        #[salsa::tracked(configuration = (pub(in crate::outer) QueryConfiguration))]
        pub(super) fn incomparable(_db: &dyn salsa::Database) -> u32 {
            0
        }
    }
}

fn main() {}
