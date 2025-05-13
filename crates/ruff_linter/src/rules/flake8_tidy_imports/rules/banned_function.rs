use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprCall, ExprName};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for banned function calls.
///
/// ## Why is this bad?
/// Projects may want to ensure that specific functions within modules are
/// not called. This is useful for enforcing project-wide conventions or
/// preventing the use of deprecated or unsafe functions.
///
/// ## Example
/// ```python
/// # With banned_functions = {"os.system".msg: "Use subprocess.run instead"}
/// os.system("ls")  # Error: os.system is banned: Use subprocess.run instead
/// ```
///
/// ## Options
/// - `lint.flake8-tidy-imports.banned-functions`
#[derive(ViolationMetadata)]
pub(crate) struct BannedFunction {
    name: String,
    message: String,
}

impl Violation for BannedFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedFunction { name, message } = self;
        format!("`{name}` is banned: {message}")
    }
}

/// Attempt to get a string representation of a function call's module and attribute.
/// For example, given `os.system`, returns Some("os.system").
fn get_function_name(func: &Expr) -> Option<String> {
    match func {
        // Handle attribute access like `os.system`
        Expr::Attribute(attr) => {
            // First try to get the module name
            if let Expr::Name(ExprName { id, .. }) = &*attr.value {
                Some(format!("{}.{}", id, attr.attr))
            } else {
                None
            }
        }
        // Handle direct function calls like `system`
        Expr::Name(ExprName { id, .. }) => Some(id.to_string()),
        _ => None,
    }
}

/// TID254
pub(crate) fn banned_function(checker: &Checker, expr: &ExprCall) -> Option<Diagnostic> {
    let banned_functions = &checker.settings.flake8_tidy_imports.banned_functions;
    if banned_functions.is_empty() {
        return None;
    }

    // First try semantic resolution
    if let Some(qualified_name) = checker.semantic().resolve_qualified_name(&expr.func) {
        let name = qualified_name.to_string();
        if let Some(ban) = banned_functions.get(&name) {
            return Some(Diagnostic::new(
                BannedFunction {
                    name,
                    message: ban.msg.to_string(),
                },
                expr.range(),
            ));
        }
    }

    // Fallback to string matching if semantic resolution fails
    if let Some(function_name) = get_function_name(&expr.func) {
        // Check for direct matches (for aliased imports)
        if let Some(ban) = banned_functions.get(&function_name) {
            return Some(Diagnostic::new(
                BannedFunction {
                    name: function_name,
                    message: ban.msg.to_string(),
                },
                expr.range(),
            ));
        }

        // Check for matches against fully qualified names
        // This helps catch cases where the function is imported directly
        for (banned_name, ban) in banned_functions {
            if banned_name.ends_with(&function_name) {
                return Some(Diagnostic::new(
                    BannedFunction {
                        name: banned_name.clone(),
                        message: ban.msg.to_string(),
                    },
                    expr.range(),
                ));
            }
        }
    }

    None
}
