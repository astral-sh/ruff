use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, ExceptHandler, Expr};
use ruff_python_stdlib::builtins::is_builtin_exception_ancestor;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `except` handlers that catch a more specific exception
/// after a more general one has already been caught.
///
/// ## Why is this bad?
/// When multiple `except` handlers are present, they are evaluated in order,
/// and the first matching handler is executed. If a more general exception
/// class (e.g., `Exception`) is caught before a more specific one (e.g.,
/// `ValueError`), the specific handler will never be reached.
///
/// ## Example
/// ```python
/// try:
///     ...
/// except Exception:
///     ...
/// except ValueError:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     ...
/// except ValueError:
///     ...
/// except Exception:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
/// - [Python documentation: Exception hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct BadExceptOrder {
    child_name: String,
    parent_name: String,
}

impl Violation for BadExceptOrder {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadExceptOrder {
            child_name,
            parent_name,
        } = self;
        format!("`{parent_name}` is an ancestor class of `{child_name}`")
    }
}

/// PLE0701
pub(crate) fn bad_except_order(checker: &Checker, handlers: &[ExceptHandler]) {
    let mut caught: Vec<&str> = Vec::new();

    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_: Some(type_),
            ..
        }) = handler
        else {
            caught.push("BaseException");
            continue;
        };

        let type_exprs: &[Expr] = match type_.as_ref() {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.as_slice(),
            expr => std::slice::from_ref(expr),
        };

        // Resolve all exception names once, then check and add in separate passes
        // to avoid flagging co-occurring types within the same handler tuple
        let resolved: Vec<&str> = type_exprs
            .iter()
            .filter_map(|expr| checker.semantic().resolve_builtin_symbol(expr))
            .collect();

        for &builtin_name in &resolved {
            for &ancestor in &caught {
                if is_builtin_exception_ancestor(ancestor, builtin_name) {
                    checker.report_diagnostic(
                        BadExceptOrder {
                            child_name: builtin_name.to_string(),
                            parent_name: ancestor.to_string(),
                        },
                        handler.range(),
                    );
                    // Only report the first ancestor match per type_expr
                    break;
                }
            }
        }

        caught.extend(resolved);
    }
}
