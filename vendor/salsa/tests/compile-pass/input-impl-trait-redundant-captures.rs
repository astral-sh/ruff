#![deny(impl_trait_redundant_captures)]

#[salsa::input]
struct Input {
    #[returns(copy)]
    field: u32,
}

fn main() {}
