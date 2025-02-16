use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_trivia::{indentation_at_offset, PythonWhitespace};
use ruff_source_file::{Line, LineRanges, UniversalNewlineIterator};
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::Rule;

/// ## What it does
/// Checks for docstrings on class definitions that are not preceded by a
/// blank line.
///
/// ## Why is this bad?
/// Use a blank line to separate the docstring from the class definition, for
/// consistency.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is disabled when using the `google`,
/// `numpy`, and `pep257` conventions.
///
/// For an alternative, see [D211].
///
/// ## Example
///
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
/// ```
///
/// Use instead:
///
/// ```python
/// class PhotoMetadata:
///
///     """Metadata about a photo."""
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// [D211]: https://docs.astral.sh/ruff/rules/blank-line-before-class
#[derive(ViolationMetadata)]
pub(crate) struct IncorrectBlankLineBeforeClass;

impl AlwaysFixableViolation for IncorrectBlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "1 blank line required before class docstring".to_string()
    }

    fn fix_title(&self) -> String {
        "Insert 1 blank line before class docstring".to_string()
    }
}

/// ## What it does
/// Checks for class methods that are not separated from the class's docstring
/// by a blank line.
///
/// ## Why is this bad?
/// [PEP 257] recommends the use of a blank line to separate a class's
/// docstring from its methods.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is enabled when using the `numpy` and `pep257`
/// conventions, and disabled when using the `google` convention.
///
/// ## Example
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
///     def __init__(self, file: Path):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
///
///     def __init__(self, file: Path):
///         ...
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[derive(ViolationMetadata)]
pub(crate) struct IncorrectBlankLineAfterClass;

impl AlwaysFixableViolation for IncorrectBlankLineAfterClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "1 blank line required after class docstring".to_string()
    }

    fn fix_title(&self) -> String {
        "Insert 1 blank line after class docstring".to_string()
    }
}

/// ## What it does
/// Checks for docstrings on class definitions that are preceded by a blank
/// line.
///
/// ## Why is this bad?
/// Avoid introducing any blank lines between a class definition and its
/// docstring, for consistency.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is enabled when using the `google`,
/// `numpy`, and `pep257` conventions.
///
/// For an alternative, see [D203].
///
/// ## Example
///
/// ```python
/// class PhotoMetadata:
///
///     """Metadata about a photo."""
/// ```
///
/// Use instead:
///
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
///
/// [D203]: https://docs.astral.sh/ruff/rules/incorrect-blank-line-before-class
#[derive(ViolationMetadata)]
pub(crate) struct BlankLineBeforeClass;

impl AlwaysFixableViolation for BlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "No blank lines allowed before class docstring".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove blank line(s) before class docstring".to_string()
    }
}

/// D203, D204, D211
pub(crate) fn blank_before_after_class(checker: &Checker, docstring: &Docstring) {
    let Some(class) = docstring.definition.as_class_def() else {
        return;
    };

    // Special-case: the docstring is on the same line as the class. For example:
    // ```python
    // class PhotoMetadata: """Metadata about a photo."""
    // ```
    let between_range = TextRange::new(class.start(), docstring.start());
    if !checker.locator().contains_line_break(between_range) {
        return;
    }

    if checker.enabled(Rule::IncorrectBlankLineBeforeClass)
        || checker.enabled(Rule::BlankLineBeforeClass)
    {
        let mut lines = UniversalNewlineIterator::with_offset(
            checker.locator().slice(between_range),
            between_range.start(),
        )
        .rev();

        let mut blank_lines_before = 0usize;
        let mut blank_lines_start = lines.next().map(|line| line.start()).unwrap_or_default();

        for line in lines {
            if line.trim().is_empty() {
                blank_lines_before += 1;
                blank_lines_start = line.start();
            } else {
                break;
            }
        }

        if checker.enabled(Rule::BlankLineBeforeClass) {
            if blank_lines_before != 0 {
                let mut diagnostic = Diagnostic::new(BlankLineBeforeClass, docstring.range());
                // Delete the blank line before the class.
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                    blank_lines_start,
                    docstring.line_start(),
                )));
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::IncorrectBlankLineBeforeClass) {
            if blank_lines_before != 1 {
                let mut diagnostic =
                    Diagnostic::new(IncorrectBlankLineBeforeClass, docstring.range());
                // Insert one blank line before the class.
                diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                    checker.stylist().line_ending().to_string(),
                    blank_lines_start,
                    docstring.line_start(),
                )));
                checker.report_diagnostic(diagnostic);
            }
        }
    }

    if checker.enabled(Rule::IncorrectBlankLineAfterClass) {
        let class_after_docstring_range = TextRange::new(docstring.end(), class.end());
        let class_after_docstring = checker.locator().slice(class_after_docstring_range);
        let mut lines = UniversalNewlineIterator::with_offset(
            class_after_docstring,
            class_after_docstring_range.start(),
        );

        // If the class is empty except for comments, we don't need to insert a newline between
        // docstring and no content
        let all_blank_after = lines.clone().all(|line| {
            line.trim_whitespace().is_empty() || line.trim_whitespace_start().starts_with('#')
        });
        if all_blank_after {
            return;
        }

        let first_line = lines.next();
        let mut replacement_start = first_line.as_ref().map(Line::start).unwrap_or_default();

        // Edge case: There is trailing end-of-line content after the docstring, either a statement
        // separated by a semicolon or a comment.
        if let Some(first_line) = &first_line {
            let trailing = first_line.as_str().trim_whitespace_start();
            if let Some(next_statement) = trailing.strip_prefix(';') {
                let indentation = indentation_at_offset(docstring.start(), checker.source())
                    .expect("Own line docstring must have indentation");
                let mut diagnostic =
                    Diagnostic::new(IncorrectBlankLineAfterClass, docstring.range());
                let line_ending = checker.stylist().line_ending().as_str();
                // We have to trim the whitespace twice, once before the semicolon above and
                // once after the semicolon here, or we get invalid indents:
                // ```rust
                // class Priority:
                //     """Has priorities"""   ;   priorities=1
                // ```
                let next_statement = next_statement.trim_whitespace_start();
                diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                    line_ending.to_string() + line_ending + indentation + next_statement,
                    replacement_start,
                    first_line.end(),
                )));
                checker.report_diagnostic(diagnostic);
                return;
            } else if trailing.starts_with('#') {
                // Keep the end-of-line comment, start counting empty lines after it
                replacement_start = first_line.end();
            }
        }

        let mut blank_lines_after = 0usize;
        let mut blank_lines_end = first_line.as_ref().map_or(docstring.end(), Line::end);

        for line in lines {
            if line.trim_whitespace().is_empty() {
                blank_lines_end = line.end();
                blank_lines_after += 1;
            } else {
                break;
            }
        }

        if blank_lines_after != 1 {
            let mut diagnostic = Diagnostic::new(IncorrectBlankLineAfterClass, docstring.range());
            // Insert a blank line before the class (replacing any existing lines).
            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                checker.stylist().line_ending().to_string(),
                replacement_start,
                blank_lines_end,
            )));
            checker.report_diagnostic(diagnostic);
        }
    }
}
