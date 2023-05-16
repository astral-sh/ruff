use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct HardcodedTempFile {
    string: String,
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
pub(crate) fn hardcoded_tmp_directory(
    expr: &Expr,
    value: &str,
    prefixes: &[String],
) -> Option<Diagnostic> {
    if prefixes.iter().any(|prefix| value.starts_with(prefix)) {
        Some(Diagnostic::new(
            HardcodedTempFile {
                string: value.to_string(),
            },
            expr.range(),
        ))
    } else {
        None
    }
}
