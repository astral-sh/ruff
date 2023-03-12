use strum::IntoEnumIterator;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::leading_quote;
use ruff_python_ast::types::Range;
use ruff_python_ast::whitespace::UniversalNewlineIterator;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::docstrings::sections::SectionKind;
use crate::message::Location;
use crate::registry::AsRule;
use crate::rules::pydocstyle::helpers::logical_line;

#[violation]
pub struct EndsInPunctuation;

impl AlwaysAutofixableViolation for EndsInPunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should end with a period, question mark, or exclamation point")
    }

    fn autofix_title(&self) -> String {
        "Add closing punctuation".to_string()
    }
}

/// D415
pub fn ends_with_punctuation(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if let Some(first_line) = UniversalNewlineIterator::from(body.trim()).next() {
        let trimmed = first_line.trim();

        // Avoid false-positives: `:param`, etc.
        for prefix in [":param", ":type", ":raises", ":return", ":rtype"] {
            if trimmed.starts_with(prefix) {
                return;
            }
        }

        // Avoid false-positives: `Args:`, etc.
        for section_kind in SectionKind::iter() {
            if let Some(suffix) = trimmed.strip_suffix(section_kind.as_str()) {
                if suffix.is_empty() {
                    return;
                }
                if suffix == ":" {
                    return;
                }
            }
        }
    }

    if let Some(index) = logical_line(body) {
        let line = UniversalNewlineIterator::from(body).nth(index).unwrap();
        let trimmed = line.trim_end();
        if !(trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?')) {
            let mut diagnostic = Diagnostic::new(EndsInPunctuation, Range::from(docstring.expr));
            // Best-effort autofix: avoid adding a period after other punctuation marks.
            if checker.patch(diagnostic.kind.rule())
                && !trimmed.ends_with(':')
                && !trimmed.ends_with(';')
            {
                if let Some((row, column)) = if index == 0 {
                    leading_quote(contents).map(|pattern| {
                        (
                            docstring.expr.location.row(),
                            docstring.expr.location.column()
                                + pattern.len()
                                + trimmed.chars().count(),
                        )
                    })
                } else {
                    Some((
                        docstring.expr.location.row() + index,
                        trimmed.chars().count(),
                    ))
                } {
                    diagnostic.amend(Fix::insertion(".".to_string(), Location::new(row, column)));
                }
            }
            checker.diagnostics.push(diagnostic);
        };
    }
}
