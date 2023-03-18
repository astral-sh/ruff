use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

/// ## What it does
/// Checks for variable names to be single letter values
///
/// ## Why is this bad?
/// Single letter variable names are non-descript and do not help the reader know what they contain
///
/// ## Example
/// ```python
/// a = "hi"
/// ```
///
/// Use instead:
/// ```python
/// descriptName = "hi"
/// ```
#[violation]
pub struct SingleLetterVariableName(pub String);

impl Violation for SingleLetterVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SingleLetterVariableName(name) = self;
        format!("Single letter variable name: `{name}`")
    }
}

fn is_single_letter_variable(name: &str, strict_mode: bool) -> bool {
    const ALLOWLIST: [&str; 3] = ["i", "_", "T"];
    const ALLOWLIST_STRICT: [&str; 2] = ["_", "T"];
    if name.len() == 1
        && (!ALLOWLIST.contains(&name) || (strict_mode && !ALLOWLIST_STRICT.contains(&name)))
    {
        return true;
    }

    false
}

/// VN001
pub fn single_letter_variable_name(
    name: &str,
    range: Range,
    strict_mode: bool,
) -> Option<Diagnostic> {
    if is_single_letter_variable(name, strict_mode) {
        Some(Diagnostic::new(
            SingleLetterVariableName(name.to_string()),
            range,
        ))
    } else {
        None
    }
}
