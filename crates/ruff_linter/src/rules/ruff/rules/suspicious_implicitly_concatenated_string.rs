use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{AnyNodeRef, Expr};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for implicitly concatenated strings in the middle of a literal collection.
///
/// ## Why is this bad?
/// Such a string often suggests the lack of a comma.
/// If it is desired, parenthesize the string to make the intent explicit.
///
/// ## Example
///
/// ```python
/// a = [
///     "lorem",
///     "ipsum",
///     "dolor"  # No comma
///     "sit",
///     "amet"
/// ]
/// ```
///
/// Use instead:
///
/// ```python
/// a = [
///     "lorem",
///     "ipsum",
///     "dolor",  # Has comma
///     "sit",
///     "amet"
/// ]
/// ```
///
/// Alternatively:
///
/// ```python
/// a = [
///     "lorem",
///     "ipsum",
///     (
///         "dolor"
///         "sit"
///     ),
///     "amet"
/// ]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SuspiciousImplicitlyConcatenatedString;

impl Violation for SuspiciousImplicitlyConcatenatedString {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Implicitly concatenated string in literal collection".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add a comma or parenthesize the string".to_string())
    }
}

/// RUF060
pub(crate) fn suspicious_implicitly_concatenated_string(
    checker: &Checker,
    collection: AnyNodeRef,
    elements: &[Expr],
) {
    let comment_ranges = checker.comment_ranges();
    let source = checker.source();

    for element in elements {
        let (range, fragment_ranges): (TextRange, Vec<TextRange>) = match element {
            Expr::StringLiteral(string) if string.value.is_implicit_concatenated() => (
                string.range,
                string.value.iter().map(|it| it.range).collect(),
            ),
            Expr::FString(fstring) if fstring.value.is_implicit_concatenated() => (
                fstring.range,
                fstring.value.iter().map(|it| it.range()).collect(),
            ),
            _ => continue,
        };

        if parenthesized_range(element.into(), collection, comment_ranges, source).is_some() {
            continue;
        }

        if let [left, right] = &fragment_ranges[..] {
            // Single-line implicitly concatenated strings are already reported by ISC001
            if !source.contains_line_break(TextRange::new(left.end(), right.start())) {
                return;
            }
        }

        let diagnostic = Diagnostic::new(SuspiciousImplicitlyConcatenatedString, range);
        checker.report_diagnostic(diagnostic);
    }
}
