mod generator;
mod stylist;

pub use generator::Generator;

pub use stylist::Stylist;

/// Run round-trip source code generation on a given Python code.
#[cfg(feature = "round_trip")]
pub fn round_trip(code: &str) -> Result<String, ruff_python_parser::ParseError> {
    use ruff_source_file::Locator;

    let locator = Locator::new(code);
    let allocator = ruff_allocator::Allocator::new();
    let parsed = ruff_python_parser::parse_module(code, &allocator)?;
    let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(parsed.suite());
    Ok(generator.generate())
}
