use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Diagnostic;
use crate::violations;

/// D404
pub fn starts_with_this(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body;

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }

    let Some(first_word) = body.split(' ').next() else {
        return
    };
    if first_word
        .replace(|c: char| !c.is_alphanumeric(), "")
        .to_lowercase()
        != "this"
    {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        violations::NoThisPrefix,
        Range::from_located(docstring.expr),
    ));
}
