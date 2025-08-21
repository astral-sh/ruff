use regex::Regex;
use std::sync::LazyLock;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_trivia::PythonWhitespace;
use ruff_source_file::{UniversalNewlineIterator, UniversalNewlines};
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::Rule;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for docstrings on functions that are separated by one or more blank
/// lines from the function definition.
///
/// ## Why is this bad?
/// Remove any blank lines between the function definition and its docstring,
/// for consistency.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///
///     """Return the mean of the given values."""
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
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLineBeforeFunction {
    num_lines: usize,
}

impl Violation for BlankLineBeforeFunction {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineBeforeFunction { num_lines } = self;
        format!("No blank lines allowed before function docstring (found {num_lines})")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove blank line(s) before function docstring".to_string())
    }
}

/// ## What it does
/// Checks for docstrings on functions that are separated by one or more blank
/// lines from the function body.
///
/// ## Why is this bad?
/// Remove any blank lines between the function body and the function
/// docstring, for consistency.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
///
///     return sum(values) / len(values)
/// ```
///
/// Use instead:
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
///     return sum(values) / len(values)
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct BlankLineAfterFunction {
    num_lines: usize,
}

impl Violation for BlankLineAfterFunction {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterFunction { num_lines } = self;
        format!("No blank lines allowed after function docstring (found {num_lines})")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove blank line(s) after function docstring".to_string())
    }
}

static INNER_FUNCTION_OR_CLASS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s+(?:(?:class|def|async def)\s|@)").unwrap());

/// D201, D202
pub(crate) fn blank_before_after_function(checker: &Checker, docstring: &Docstring) {
    let Some(function) = docstring.definition.as_function_def() else {
        return;
    };

    if checker.is_rule_enabled(Rule::BlankLineBeforeFunction) {
        let before = checker
            .locator()
            .slice(TextRange::new(function.start(), docstring.start()));

        let mut lines = UniversalNewlineIterator::with_offset(before, function.start()).rev();
        let mut blank_lines_before = 0usize;
        let mut blank_lines_start = lines.next().map(|l| l.end()).unwrap_or_default();
        let mut start_is_line_continuation = false;

        for line in lines {
            if line.trim().is_empty() {
                blank_lines_before += 1;
                blank_lines_start = line.start();
            } else {
                start_is_line_continuation = line.ends_with('\\');
                break;
            }
        }

        if blank_lines_before != 0 {
            let mut diagnostic = checker.report_diagnostic(
                BlankLineBeforeFunction {
                    num_lines: blank_lines_before,
                },
                docstring.range(),
            );
            // Do not offer fix if a \ would cause it to be a syntax error
            if !start_is_line_continuation {
                // Delete the blank line before the docstring.
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                    blank_lines_start,
                    docstring.line_start(),
                )));
            }
        }
    }

    if checker.is_rule_enabled(Rule::BlankLineAfterFunction) {
        let after = checker
            .locator()
            .slice(TextRange::new(docstring.end(), function.end()));

        // If the docstring is only followed by blank and commented lines, abort.
        let all_blank_after = after.universal_newlines().skip(1).all(|line| {
            line.trim_whitespace().is_empty() || line.trim_whitespace_start().starts_with('#')
        });
        if all_blank_after {
            return;
        }

        // Count the number of blank lines after the docstring.
        let mut blank_lines_after = 0usize;
        let mut lines = UniversalNewlineIterator::with_offset(after, docstring.end()).peekable();
        let first_line = lines.next();
        let first_line_line_continuation = first_line.as_ref().is_some_and(|l| l.ends_with('\\'));
        let first_line_end = first_line.map(|l| l.end()).unwrap_or_default();
        let mut blank_lines_end = first_line_end;

        while let Some(line) = lines.peek() {
            if line.trim().is_empty() {
                blank_lines_after += 1;
                blank_lines_end = line.end();
                lines.next();
            } else {
                break;
            }
        }

        // Avoid violations for blank lines followed by inner functions or classes.
        if blank_lines_after == 1
            && lines
                .find(|line| !line.trim_whitespace_start().starts_with('#'))
                .is_some_and(|line| INNER_FUNCTION_OR_CLASS_REGEX.is_match(&line))
        {
            return;
        }

        if blank_lines_after != 0 {
            let mut diagnostic = checker.report_diagnostic(
                BlankLineAfterFunction {
                    num_lines: blank_lines_after,
                },
                docstring.range(),
            );
            // Do not offer fix if a \ would cause it to be a syntax error
            if !first_line_line_continuation {
                // Delete the blank line after the docstring.
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                    first_line_end,
                    blank_lines_end,
                )));
            }
        }
    }
}
