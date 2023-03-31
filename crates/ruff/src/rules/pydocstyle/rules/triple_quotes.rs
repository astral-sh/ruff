use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;

#[violation]
pub struct TripleSingleQuotes;

impl Violation for TripleSingleQuotes {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use triple double quotes `"""`"#)
    }
}

/// D300
pub fn triple_quotes(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    let leading_quote = docstring.leading_quote().to_ascii_lowercase();

    let starts_with_triple = if body.contains("\"\"\"") {
        matches!(leading_quote.as_str(), "'''" | "u'''" | "r'''" | "ur'''")
    } else {
        matches!(
            leading_quote.as_str(),
            "\"\"\"" | "u\"\"\"" | "r\"\"\"" | "ur\"\"\""
        )
    };
    if !starts_with_triple {
        checker
            .diagnostics
            .push(Diagnostic::new(TripleSingleQuotes, docstring.range()));
    }
}
