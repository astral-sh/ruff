
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
pub struct NonDescriptVariableName(pub String);

impl Violation for NonDescriptVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonDescriptVariableName(name) = self;
        format!("Non-descript variable name: `{name}`")
    }
}

fn is_non_descript_variable(name: &str) -> bool {
    const BLOCKLIST: [&'static str; 17] = ["val", "vals", "var", "vars", "variable", "contents", "handle", "file", "objs", "some", "do", "no", "true", "false", "foo", "bar", "baz"];
    
    if BLOCKLIST.contains(&name.to_lowercase().as_str()) {
        return true;
    }
    
    return false;
}

/// VN002
pub fn non_descript_variable_name(name: &str, range: Range) -> Option<Diagnostic> {
    if is_non_descript_variable(name) {
    Some(Diagnostic::new(
        NonDescriptVariableName(name.to_string()),
        range,
    ))
} else {
    None
}}