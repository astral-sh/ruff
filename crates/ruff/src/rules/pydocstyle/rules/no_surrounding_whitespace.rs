use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::NewlineWithTrailingNewline;
use ruff_python_ast::str::leading_quote;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::message::Location;
use crate::registry::AsRule;
use crate::rules::pydocstyle::helpers::ends_with_backslash;

#[violation]
pub struct SurroundingWhitespace;

impl AlwaysAutofixableViolation for SurroundingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No whitespaces allowed surrounding docstring text")
    }

    fn autofix_title(&self) -> String {
        "Trim surrounding whitespace".to_string()
    }
}

/// D210
pub fn no_surrounding_whitespace(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    let mut lines = NewlineWithTrailingNewline::from(body);
    let Some(line) = lines.next() else {
        return;
    };
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if line == trimmed {
        return;
    }
    let mut diagnostic = Diagnostic::new(SurroundingWhitespace, Range::from(docstring.expr));
    if checker.patch(diagnostic.kind.rule()) {
        if let Some(pattern) = leading_quote(contents) {
            // If removing whitespace would lead to an invalid string of quote
            // characters, avoid applying the fix.
            if !trimmed.ends_with(pattern.chars().last().unwrap())
                && !trimmed.starts_with(pattern.chars().last().unwrap())
                && !ends_with_backslash(trimmed)
            {
                diagnostic.set_fix(Edit::replacement(
                    trimmed.to_string(),
                    Location::new(
                        docstring.expr.location.row(),
                        docstring.expr.location.column() + pattern.len(),
                    ),
                    Location::new(
                        docstring.expr.location.row(),
                        docstring.expr.location.column() + pattern.len() + line.chars().count(),
                    ),
                ));
            }
        }
    }
    checker.diagnostics.push(diagnostic);
}
