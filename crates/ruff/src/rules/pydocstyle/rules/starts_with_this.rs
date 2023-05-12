use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::rules::pydocstyle::helpers::normalize_word;

#[violation]
pub struct DocstringStartsWithThis;

impl Violation for DocstringStartsWithThis {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"First word of the docstring should not be "This""#)
    }
}

/// D404
pub(crate) fn starts_with_this(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

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
    checker
        .diagnostics
        .push(Diagnostic::new(DocstringStartsWithThis, docstring.range()));
}
