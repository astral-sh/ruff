use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

/// ## What it does
/// Checks for `env.getenv` calls with invalid default values.
///
/// ## Why is this bad?
/// If an environment variable is set, `env.getenv` will return its value as
/// a string. If the environment variable is _not_ set, `env.getenv` will
/// return `None`, or the default value if one is provided.
///
/// If the default value is not a string or `None`, then it will be
/// inconsistent with the return type of `env.getenv`, which can lead to
/// confusing behavior.
///
/// ## Example
/// ```python
/// int(env.getenv("FOO", 1))
/// ```
///
/// Use instead:
/// ```python
/// int(env.getenv("FOO", "1"))
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

fn is_single_letter_variable(name: &str) -> bool {
    const ALLOWLIST: [&'static str; 3] = ["i", "_", "T"];
    if name.len() == 1 && !ALLOWLIST.contains(&name) {
        return true;
    }

    return false;
}

/// VN001
pub fn single_letter_variable_name(name: &str, range: Range) -> Option<Diagnostic> {
    if is_single_letter_variable(name) {
        Some(Diagnostic::new(
            SingleLetterVariableName(name.to_string()),
            range,
        ))
    } else {
        None
    }
}
