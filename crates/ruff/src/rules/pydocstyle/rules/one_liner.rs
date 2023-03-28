use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::NewlineWithTrailingNewline;
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::AsRule;

#[violation]
pub struct FitsOnOneLine;

impl AlwaysAutofixableViolation for FitsOnOneLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("One-line docstring should fit on one line")
    }

    fn autofix_title(&self) -> String {
        "Reformat to one line".to_string()
    }
}

/// D200
pub fn one_liner(checker: &mut Checker, docstring: &Docstring) {
    let mut line_count = 0;
    let mut non_empty_line_count = 0;
    for line in NewlineWithTrailingNewline::from(docstring.body) {
        line_count += 1;
        if !line.trim().is_empty() {
            non_empty_line_count += 1;
        }
        if non_empty_line_count > 1 {
            return;
        }
    }

    if non_empty_line_count == 1 && line_count > 1 {
        let mut diagnostic = Diagnostic::new(FitsOnOneLine, Range::from(docstring.expr));
        if checker.patch(diagnostic.kind.rule()) {
            if let (Some(leading), Some(trailing)) = (
                leading_quote(docstring.contents),
                trailing_quote(docstring.contents),
            ) {
                // If removing whitespace would lead to an invalid string of quote
                // characters, avoid applying the fix.
                let trimmed = docstring.body.trim();
                if !trimmed.ends_with(trailing.chars().last().unwrap())
                    && !trimmed.starts_with(leading.chars().last().unwrap())
                {
                    diagnostic.set_fix(Edit::replacement(
                        format!("{leading}{trimmed}{trailing}"),
                        docstring.expr.location,
                        docstring.expr.end_location.unwrap(),
                    ));
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
