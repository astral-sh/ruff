use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `open` or similar calls without an explicit `encoding` argument.
///
/// ## Why is this bad?
/// Using `open` in text mode without an explicit encoding specified can lead to
/// unportable code that leads to different behaviour on different systems.
///
/// Instead, consider using the `encoding` parameter to explicitly enforce a specific encoding.
///
/// ## Example
/// ```python
/// open("file.txt")
/// ```
///
/// Use instead:
/// ```python
/// open("file.txt", encoding="utf-8")
/// ```
///
/// ## References
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
#[violation]
pub struct UnspecifiedEncoding {
    function_name: String,
}

impl Violation for UnspecifiedEncoding {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`{}` {}without explicit `encoding` argument",
            self.function_name,
            if self.function_name == "open" {
                "in text mode "
            } else {
                ""
            }
        )
    }
}

fn is_binary_mode(expr: &ast::Expr) -> Option<bool> {
    Some(expr.as_constant_expr()?.value.as_str()?.value.contains('b'))
}

fn is_violation(call: &ast::ExprCall, path: &[&str]) -> bool {
    // this checks if we have something like *args which might contain the encoding argument
    if call
        .arguments
        .args
        .iter()
        .any(ruff_python_ast::Expr::is_starred_expr)
    {
        return false;
    }
    // this checks if we have something like **kwargs which might contain the encoding argument
    if call.arguments.keywords.iter().any(|a| a.arg.is_none()) {
        return false;
    }
    match path {
        ["" | "codecs", "open"] => {
            if let Some(mode_arg) = call.arguments.find_argument("mode", 1) {
                if is_binary_mode(mode_arg).unwrap_or(true) {
                    // binary mode or unknown mode is no violation
                    return false;
                }
            }
            // else mode not specified, defaults to text mode
            call.arguments.find_argument("encoding", 3).is_none()
        }
        ["io", "TextIOWrapper"] => call.arguments.find_argument("encoding", 1).is_none(),
        ["tempfile", "TemporaryFile" | "NamedTemporaryFile" | "SpooledTemporaryFile"] => {
            let mode_pos = usize::from(path[1] == "SpooledTemporaryFile");
            if let Some(mode_arg) = call.arguments.find_argument("mode", mode_pos) {
                if is_binary_mode(mode_arg).unwrap_or(true) {
                    // binary mode or unknown mode is no violation
                    return false;
                }
            } else {
                // defaults to binary mode
                return false;
            }
            call.arguments
                .find_argument("encoding", mode_pos + 2)
                .is_none()
        }
        _ => false,
    }
}

/// PLW1514
pub(crate) fn unspecified_encoding(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(path) = checker.semantic().resolve_call_path(&call.func) else {
        return;
    };
    if is_violation(call, path.as_slice()) {
        let path_slice = if path[0].is_empty() {
            &path[1..]
        } else {
            &path[0..]
        };
        let result = Diagnostic::new(
            UnspecifiedEncoding {
                function_name: path_slice.join("."),
            },
            call.func.range(),
        );
        drop(path);
        checker.diagnostics.push(result);
    }
}
