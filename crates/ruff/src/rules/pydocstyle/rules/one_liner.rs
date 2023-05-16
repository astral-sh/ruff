use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::NewlineWithTrailingNewline;
use ruff_python_ast::str::{leading_quote, trailing_quote};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::AsRule;

#[violation]
pub struct FitsOnOneLine;

impl Violation for FitsOnOneLine {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("One-line docstring should fit on one line")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Reformat to one line".to_string())
    }
}

/// D200
pub(crate) fn one_liner(checker: &mut Checker, docstring: &Docstring) {
    let mut line_count = 0;
    let mut non_empty_line_count = 0;
    for line in NewlineWithTrailingNewline::from(docstring.body().as_str()) {
        line_count += 1;
        if !line.trim().is_empty() {
            non_empty_line_count += 1;
        }
        if non_empty_line_count > 1 {
            return;
        }
    }

    if non_empty_line_count == 1 && line_count > 1 {
        let mut diagnostic = Diagnostic::new(FitsOnOneLine, docstring.range());
        if checker.patch(diagnostic.kind.rule()) {
            if let (Some(leading), Some(trailing)) = (
                leading_quote(docstring.contents),
                trailing_quote(docstring.contents),
            ) {
                // If removing whitespace would lead to an invalid string of quote
                // characters, avoid applying the fix.
                let body = docstring.body();
                let trimmed = body.trim();
                if !trimmed.ends_with(trailing.chars().last().unwrap())
                    && !trimmed.starts_with(leading.chars().last().unwrap())
                {
                    #[allow(deprecated)]
                    diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                        format!("{leading}{trimmed}{trailing}"),
                        docstring.range(),
                    )));
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
