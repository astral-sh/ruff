use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_source_file::NewlineWithTrailingNewline;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for single-line docstrings that are broken across multiple lines.
///
/// ## Why is this bad?
/// [PEP 257] recommends that docstrings that _can_ fit on one line should be
/// formatted on a single line, for consistency and readability.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///     """
///     Return the mean of the given values.
///     """
/// ```
///
/// Use instead:
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[violation]
pub struct FitsOnOneLine;

impl Violation for FitsOnOneLine {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("One-line docstring should fit on one line")
    }

    fn fix_title(&self) -> Option<String> {
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
        if let (Some(leading), Some(trailing)) = (
            leading_quote(docstring.contents),
            trailing_quote(docstring.contents),
        ) {
            // If removing whitespace would lead to an invalid string of quote
            // characters, avoid applying the fix.
            let body = docstring.body();
            let trimmed = body.trim();
            if trimmed.chars().rev().take_while(|c| *c == '\\').count() % 2 == 0
                && !trimmed.ends_with(trailing.chars().last().unwrap())
                && !trimmed.starts_with(leading.chars().last().unwrap())
            {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    format!("{leading}{trimmed}{trailing}"),
                    docstring.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
