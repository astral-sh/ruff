use strum::IntoEnumIterator;

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::strings::leading_quote;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::docstrings::sections::SectionKind;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::logical_line;
use crate::violation::AlwaysAutofixableViolation;

#[violation]
pub struct EndsInPeriod;

impl AlwaysAutofixableViolation for EndsInPeriod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should end with a period")
    }

    fn autofix_title(&self) -> String {
        "Add period".to_string()
    }
}

/// D400
pub fn ends_with_period(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if let Some(first_line) = body.trim().lines().next() {
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
        let line = body.lines().nth(index).unwrap();
        let trimmed = line.trim_end();

        if !trimmed.ends_with('.') {
            let mut diagnostic = Diagnostic::new(EndsInPeriod, Range::from_located(docstring.expr));
            // Best-effort autofix: avoid adding a period after other punctuation marks.
            if checker.patch((&diagnostic.kind).into())
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
