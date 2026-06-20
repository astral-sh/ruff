use std::borrow::Cow;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::Definition;
use ruff_source_file::{LineRanges, NewlineWithTrailingNewline};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::Rule;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
/// The fix is marked as unsafe because it could affect tools that parse
/// docstrings, documentation generators, or custom introspection utilities
/// that rely on specific docstring formatting.
///
/// ## Options
///
/// - `lint.pydocstyle.ignore-decorators`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.68")]
pub(crate) struct UnnecessaryMultilineDocstring;

impl Violation for UnnecessaryMultilineDocstring {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "One-line docstring should fit on one line".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Reformat to one line".to_string())
    }
}

/// ## What it does
/// Checks for single-line docstrings that are formatted on a single line.
///
/// ## Why is this bad?
/// Some projects prefer to use a multi-line layout for all docstrings, even
/// for those that would otherwise fit on a single line. Doing so can reduce
/// churn when docstrings are later expanded, and makes formatting consistent
/// across all docstrings.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
/// ```
///
/// Use instead (with `D213` enabled):
/// ```python
/// def average(values: list[float]) -> float:
///     """
///     Return the mean of the given values.
///     """
/// ```
///
/// Use instead (with `D212` enabled):
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values.
///     """
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe because it changes docstring formatting, which
/// can affect tools that parse docstrings, documentation generators, or custom
/// introspection utilities that rely on specific formatting.
///
/// ## Options
///
/// - `lint.pydocstyle.ignore-decorators`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.14")]
pub(crate) struct OneLineDocstringShouldBeMultiLine;

impl Violation for OneLineDocstringShouldBeMultiLine {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    fn message(&self) -> String {
        "One-line docstring should use multi-line quotes".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Reformat to multi-line".to_string())
    }
}

/// D200
pub(crate) fn one_liner(checker: &Checker, docstring: &Docstring) {
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
        let mut diagnostic =
            checker.report_diagnostic(UnnecessaryMultilineDocstring, docstring.range());

        // If removing whitespace would lead to an invalid string of quote
        // characters, avoid applying the fix.
        let body = docstring.body();
        let trimmed = body.trim();
        let quote_char = docstring.quote_style().as_char();
        if trimmed.chars().rev().take_while(|c| *c == '\\').count() % 2 == 0
            && !trimmed.ends_with(quote_char)
            && !trimmed.starts_with(quote_char)
        {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                format!(
                    "{leading}{trimmed}{trailing}",
                    leading = docstring.opener(),
                    trailing = docstring.closer()
                ),
                docstring.range(),
            )));
        }
    }
}

/// D219
pub(crate) fn multi_line_docstring(checker: &Checker, docstring: &Docstring) {
    // If not a triple-quoted docstring, nothing to do.
    if !docstring.is_triple_quoted() {
        return;
    }

    // If the docstring already spans multiple lines, nothing to do.
    if NewlineWithTrailingNewline::from(docstring.contents())
        .nth(1)
        .is_some()
    {
        return;
    }

    let body = docstring.body();

    // If the body is empty, nothing to do.
    if body.is_empty() {
        return;
    }

    // Let's now check whether we can fix it.
    let mut diagnostic =
        checker.report_diagnostic(OneLineDocstringShouldBeMultiLine, docstring.range());

    let mut indentation = Cow::Borrowed(docstring.compute_indentation());
    let mut fixable = true;

    // If the docstring is indented, ensure that it's only indented with
    // whitespace.
    if !indentation.chars().all(char::is_whitespace) {
        fixable = false;

        // If the docstring isn't on its own line, look at the statement
        // indentation, and add the default indentation to get the "right"
        // level.
        if let Definition::Member(member) = &docstring.definition {
            // Get the statement indentation.
            let stmt_line_start = checker.locator().line_start(member.start());
            let stmt_indentation = checker
                .locator()
                .slice(TextRange::new(stmt_line_start, member.start()));

            // If the statement indentation is all whitespace, use that plus
            // the default indentation.
            if stmt_indentation.chars().all(char::is_whitespace) {
                let indentation = indentation.to_mut();
                indentation.clear();
                indentation.push_str(stmt_indentation);
                indentation.push_str(checker.stylist().indentation());
                fixable = true;
            }
        }
    }

    if fixable {
        // Construct the replacement, depending on D212 (default) or D213 being
        // enabled (read from MultiLineSummarySecondLine).
        let line_ending = checker.stylist().line_ending().as_str();
        let replacement = if checker.is_rule_enabled(Rule::MultiLineSummarySecondLine) {
            format!(
                "{}{}{}{}{}{}",
                docstring.opener(),
                line_ending,
                indentation,
                body.as_str(),
                line_ending,
                format!("{}{}", indentation, docstring.closer())
            )
        } else {
            format!(
                "{}{}{}{}{}",
                docstring.opener(),
                body.as_str(),
                line_ending,
                indentation,
                docstring.closer()
            )
        };

        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            replacement,
            docstring.range(),
        )));
    }
}
