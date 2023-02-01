use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::normalize_word;
use crate::violations;

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
        violations::NoThisPrefix,
        Range::from_located(docstring.expr),
    ));
}
