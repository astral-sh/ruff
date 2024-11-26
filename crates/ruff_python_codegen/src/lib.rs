pub use expression_generator::ExpressionGenerator;
pub use generator::Generator;
use ruff_python_parser::{parse_module, ParseError};
pub use stylist::Stylist;

mod expression_generator;
mod generator;
pub mod precedence;
mod stylist;

/// Run round-trip source code generation on a given Python code.
pub fn round_trip(code: &str) -> Result<String, ParseError> {
    let parsed = parse_module(code)?;
    let stylist = Stylist::from_tokens(parsed.tokens(), code);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(parsed.suite());
    Ok(generator.generate())
}
