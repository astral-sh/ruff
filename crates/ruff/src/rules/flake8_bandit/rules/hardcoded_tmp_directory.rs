use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct HardcodedTempFile {
    pub string: String,
}

impl Violation for HardcodedTempFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedTempFile { string } = self;
        format!(
            "Probable insecure usage of temporary file or directory: \"{}\"",
            string.escape_debug()
        )
    }
}

/// S108
pub fn hardcoded_tmp_directory(
    expr: &Expr,
    value: &str,
    prefixes: &[String],
) -> Option<Diagnostic> {
    if prefixes.iter().any(|prefix| value.starts_with(prefix)) {
        Some(Diagnostic::new(
            HardcodedTempFile {
                string: value.to_string(),
            },
            Range::from(expr),
        ))
    } else {
        None
    }
}
