use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UsesTripleQuotes;
);
impl Violation for UsesTripleQuotes {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use """triple double quotes""""#)
    }
}

/// D300
pub fn triple_quotes(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    let Some(first_line) = contents
        .lines()
        .next()
        .map(str::to_lowercase) else
    {
        return;
    };
    let starts_with_triple = if body.contains("\"\"\"") {
        first_line.starts_with("'''")
            || first_line.starts_with("u'''")
            || first_line.starts_with("r'''")
            || first_line.starts_with("ur'''")
    } else {
        first_line.starts_with("\"\"\"")
            || first_line.starts_with("u\"\"\"")
            || first_line.starts_with("r\"\"\"")
            || first_line.starts_with("ur\"\"\"")
    };
    if !starts_with_triple {
        checker.diagnostics.push(Diagnostic::new(
            UsesTripleQuotes,
            Range::from_located(docstring.expr),
        ));
    }
}
