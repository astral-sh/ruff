#[salsa::interned]
struct UnknownAttributeInterned {
    /// Test doc comment
    field: bool,
    #[unknown_attr]
    field2: bool,
    #[salsa::tracked]
    wrong_tracked: bool,
}

fn main() {}
