mod generator;
mod stylist;

pub use generator::Generator;
use ruff_python_parser::{lexer, parse_module, Mode, ParseError};
use ruff_source_file::Locator;
pub use stylist::Stylist;

/// Run round-trip source code generation on a given Python code.
pub fn round_trip(code: &str) -> Result<String, ParseError> {
    let locator = Locator::new(code);
    let stmts = parse_module(code)?.suite();
    let tokens: Vec<_> = lexer::lex(code, Mode::Module).collect();
    let stylist = Stylist::from_tokens(&tokens, &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(&stmts);
    Ok(generator.generate())
}
