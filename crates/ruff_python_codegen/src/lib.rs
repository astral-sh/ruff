mod generator;
mod stylist;

pub use generator::Generator;
use ruff_python_parser::{parse_module, ParseError};
use ruff_source_file::Locator;
pub use stylist::Stylist;

/// Run round-trip source code generation on a given Python code.
pub fn round_trip(code: &str) -> Result<String, ParseError> {
    let locator = Locator::new(code);
    let program = parse_module(code)?;
    let stylist = Stylist::from_tokens(program.tokens(), &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(program.suite());
    Ok(generator.generate())
}
