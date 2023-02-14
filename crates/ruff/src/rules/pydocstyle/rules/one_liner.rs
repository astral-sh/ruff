use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct FitsOnOneLine;
);
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
    for line in LinesWithTrailingNewline::from(docstring.body) {
        line_count += 1;
        if !line.trim().is_empty() {
            non_empty_line_count += 1;
        }
        if non_empty_line_count > 1 {
            return;
        }
    }

    if non_empty_line_count == 1 && line_count > 1 {
        let mut diagnostic = Diagnostic::new(FitsOnOneLine, Range::from_located(docstring.expr));
        if checker.patch(diagnostic.kind.rule()) {
            if let (Some(leading), Some(trailing)) = (
                helpers::leading_quote(docstring.contents),
                helpers::trailing_quote(docstring.contents),
            ) {
                // If removing whitespace would lead to an invalid string of quote
                // characters, avoid applying the fix.
                let trimmed = docstring.body.trim();
                if !trimmed.ends_with(trailing.chars().last().unwrap())
                    && !trimmed.starts_with(leading.chars().last().unwrap())
                {
                    diagnostic.amend(Fix::replacement(
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
