pub use generator::{Generator, Mode};
use ruff_python_parser::{ParseError, parse_module};
pub use stylist::{Indentation, Stylist};

mod generator;
mod stylist;

/// Run round-trip source code generation on a given Python code.
pub fn round_trip(code: &str) -> Result<String, ParseError> {
    let parsed = parse_module(code)?;
    let stylist = Stylist::from_tokens(parsed.tokens(), code);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(parsed.suite());
    Ok(generator.generate())
}
