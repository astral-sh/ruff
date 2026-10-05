// Ensure the `salsa::tracked` attribute macro doesn't conflict with local
// redefinition of the `Result` type.
//
// See: https://github.com/salsa-rs/salsa/pull/1025

type Result<T> = std::result::Result<T, String>;

#[salsa::tracked(returns(clone))]
fn example_query(_db: &dyn salsa::Database) -> Result<()> {
    Ok(())
}

fn main() {
    println!("Hello, world!");
}
