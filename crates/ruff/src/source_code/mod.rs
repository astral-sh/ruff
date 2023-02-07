mod generator;
mod indexer;
mod locator;
mod stylist;

pub(crate) use generator::Generator;
pub(crate) use indexer::Indexer;
pub(crate) use locator::Locator;
use rustpython_parser::error::ParseError;
use rustpython_parser::parser;
pub(crate) use stylist::{LineEnding, Stylist};

/// Run round-trip source code generation on a given Python code.
pub fn round_trip(code: &str, source_path: &str) -> Result<String, ParseError> {
    let locator = Locator::new(code);
    let python_ast = parser::parse_program(code, source_path)?;
    let stylist = Stylist::from_contents(code, &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(&python_ast);
    Ok(generator.generate())
}
