use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::docstrings::styles::SectionStyle;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::{leading_quote, logical_line};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct EndsInPeriod;
);
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
        for style in [SectionStyle::Google, SectionStyle::Numpy] {
            for section_name in style.section_names().iter() {
                if let Some(suffix) = trimmed.strip_suffix(section_name) {
                    if suffix.is_empty() {
                        return;
                    }
                    if suffix == ":" {
                        return;
                    }
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
