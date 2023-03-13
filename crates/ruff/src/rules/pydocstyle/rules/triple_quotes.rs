use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::types::Range;

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
    let contents = docstring.contents;
    let body = docstring.body;

    let Some(first_line) = contents.universal_newlines()
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
            TripleSingleQuotes,
            Range::from(docstring.expr),
        ));
    }
}
