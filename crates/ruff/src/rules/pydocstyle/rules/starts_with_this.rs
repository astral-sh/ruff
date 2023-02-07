use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::normalize_word;
use crate::violation::Violation;

define_violation!(
    pub struct NoThisPrefix;
);
impl Violation for NoThisPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"First word of the docstring should not be "This""#)
    }
}

/// D404
pub fn starts_with_this(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body;

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }

    let Some(first_word) = trimmed.split(' ').next() else {
        return
    };
    if normalize_word(first_word) != "this" {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        NoThisPrefix,
        Range::from_located(docstring.expr),
    ));
}
