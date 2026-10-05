#[salsa::input]
struct InputWithUnknownAttrs {
    /// Doc comment
    field: u32,
    #[anything]
    field2: u32,
}

fn main() {}
