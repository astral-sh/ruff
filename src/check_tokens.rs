//! Lint rules based on token traversal.

use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::operations::SourceCodeLocator;
use crate::checks::{Check, CheckCode};
use crate::{pycodestyle, Settings};

pub fn check_tokens(
    checks: &mut Vec<Check>,
    contents: &str,
    tokens: &[LexResult],
    settings: &Settings,
) {
    // TODO(charlie): Use a shared SourceCodeLocator between this site and the AST traversal.
    let locator = SourceCodeLocator::new(contents);
    let enforce_invalid_escape_sequence = settings.enabled.contains(&CheckCode::W605);
    for (start, tok, end) in tokens.iter().flatten() {
        if enforce_invalid_escape_sequence {
            if matches!(tok, Tok::String { .. }) {
                checks.extend(pycodestyle::checks::invalid_escape_sequence(
                    &locator, start, end,
                ));
            }
        }
    }
}
