use rustpython_parser::ast::{Mod, Suite};
use rustpython_parser::error::ParseError;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::mode::Mode;
use rustpython_parser::{lexer, parser};

/// Collect tokens up to and including the first error.
pub fn tokenize(contents: &str) -> Vec<LexResult> {
    let mut tokens: Vec<LexResult> = vec![];
    for tok in lexer::make_tokenizer(contents) {
        let is_err = tok.is_err();
        tokens.push(tok);
        if is_err {
            break;
        }
    }
    tokens
}

/// Parse a full Python program from its tokens.
pub(crate) fn parse_program_tokens(
    lxr: Vec<LexResult>,
    source_path: &str,
) -> anyhow::Result<Suite, ParseError> {
    parser::parse_tokens(lxr, Mode::Module, source_path).map(|top| match top {
        Mod::Module { body, .. } => body,
        _ => unreachable!(),
    })
}
