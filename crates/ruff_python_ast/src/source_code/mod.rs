mod generator;
mod indexer;
mod locator;
mod stylist;

pub use generator::Generator;
pub use indexer::Indexer;
pub use locator::Locator;
use rustpython_parser as parser;
use rustpython_parser::{lexer, Mode, ParseError};
pub use stylist::{LineEnding, Stylist};

/// Run round-trip source code generation on a given Python code.
pub fn round_trip(code: &str, source_path: &str) -> Result<String, ParseError> {
    let locator = Locator::new(code);
    let python_ast = parser::parse_program(code, source_path)?;
    let tokens: Vec<_> = lexer::lex(code, Mode::Module).collect();
    let stylist = Stylist::from_tokens(&tokens, &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(&python_ast);
    Ok(generator.generate())
}
