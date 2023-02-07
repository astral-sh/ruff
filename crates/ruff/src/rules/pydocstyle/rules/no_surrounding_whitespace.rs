use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::leading_quote;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct NoSurroundingWhitespace;
);
impl AlwaysAutofixableViolation for NoSurroundingWhitespace {
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

    let mut lines = LinesWithTrailingNewline::from(body);
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
    let mut diagnostic =
        Diagnostic::new(NoSurroundingWhitespace, Range::from_located(docstring.expr));
    if checker.patch(diagnostic.kind.rule()) {
        if let Some(pattern) = leading_quote(contents) {
            // If removing whitespace would lead to an invalid string of quote
            // characters, avoid applying the fix.
            if !trimmed.ends_with(pattern.chars().last().unwrap())
                && !trimmed.starts_with(pattern.chars().last().unwrap())
            {
                diagnostic.amend(Fix::replacement(
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
