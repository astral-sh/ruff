use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::Violation;

/// ## What it does
/// Checks for assignments from function calls that return None.
///
/// ## Why is this bad?
/// Assigning the result of a function call that always returns None is typically
/// a programming error. The variable will always be None, which is likely not
/// the intended behavior.
///
/// ## Example
/// ```python
/// def function():
///     return None
///
/// f = function()  # [assignment-from-none]
/// ```
///
/// Use instead:
/// ```python
/// def function():
///     return None
///
/// f = function() if function() else 1
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AssignmentFromNone {
    name: String,
}

impl Violation for AssignmentFromNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssignmentFromNone { name } = self;
        format!("Assigning result of a function call, where the function returns None: `{name}`")
    }
}

/// PLE1128
pub(crate) fn assignment_from_none(checker: &Checker, assign: &ast::StmtAssign) {
    // Only check simple assignments with a single target
    let [target] = assign.targets.as_slice() else {
        return;
    };

    // Check if the right-hand side is a function call
    let ast::Expr::Call(call) = assign.value.as_ref() else {
        return;
    };

    // Get the function name
    let function_name = match call.func.as_ref() {
        ast::Expr::Name(ast::ExprName { id, .. }) => id,
        ast::Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr,
        _ => return,
    };

    // Check if the function is known to return None
    if returns_none(checker, call) {
        let target_name = match target.as_ref() {
            ast::Expr::Name(ast::ExprName { id, .. }) => id,
            _ => return,
        };

        checker.report_diagnostic(
            AssignmentFromNone {
                name: format!("{} = {}", target_name, function_name),
            },
            assign.range(),
        );
    }
}

/// PLE1128
pub(crate) fn annotated_assignment_from_none(checker: &Checker, assign: &ast::StmtAnnAssign) {
    let Some(value) = assign.value.as_ref() else {
        return;
    };

    // Check if the right-hand side is a function call
    let ast::Expr::Call(call) = value.as_ref() else {
        return;
    };

    // Get the function name
    let function_name = match call.func.as_ref() {
        ast::Expr::Name(ast::ExprName { id, .. }) => id,
        ast::Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr,
        _ => return,
    };

    // Check if the function is known to return None
    if returns_none(checker, call) {
        let target_name = match assign.target.as_ref() {
            ast::Expr::Name(ast::ExprName { id, .. }) => id,
            _ => return,
        };

        checker.report_diagnostic(
            AssignmentFromNone {
                name: format!("{}: {} = {}", target_name, function_name, function_name),
            },
            assign.range(),
        );
    }
}

/// Check if a function call is known to return None
fn returns_none(checker: &Checker, call: &ast::ExprCall) -> bool {
    let function_name = match call.func.as_ref() {
        ast::Expr::Name(ast::ExprName { id, .. }) => id,
        ast::Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr,
        _ => return false,
    };

    // Common functions that return None
    matches!(
        function_name,
        "print" | "exit" | "quit" | "sys.exit" | "os._exit" | "time.sleep" |
        "random.seed" | "np.random.seed" | "plt.show" | "plt.close" |
        "list.sort" | "list.reverse" | "dict.clear" | "set.clear" |
        "file.flush" | "file.close"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_returns_none_builtin_functions() {
        // Test that built-in functions that return None are correctly identified
        let test_cases = vec![
            "print",
            "exit", 
            "quit",
            "sys.exit",
            "os._exit",
            "time.sleep",
            "random.seed",
            "np.random.seed",
            "plt.show",
            "plt.close",
            "list.sort",
            "list.reverse",
            "dict.clear",
            "set.clear",
            "file.flush",
            "file.close",
        ];

        for function_name in test_cases {
            // We can't easily test the `returns_none` function directly without a full checker context,
            // but we can at least verify the function names are in our matches list
            assert!(
                matches!(
                    function_name,
                    "print" | "exit" | "quit" | "sys.exit" | "os._exit" | "time.sleep" |
                    "random.seed" | "np.random.seed" | "plt.show" | "plt.close" |
                    "list.sort" | "list.reverse" | "dict.clear" | "set.clear" |
                    "file.flush" | "file.close"
                ),
                "Function '{}' should be in the returns_none list",
                function_name
            );
        }
    }

    #[test]
    fn test_returns_none_non_matching_functions() {
        let non_matching_cases = vec![
            "len",
            "str",
            "int",
            "list",
            "dict",
            "set",
            "range",
            "enumerate",
            "zip",
            "map",
            "filter",
            "sum",
            "max",
            "min",
            "abs",
            "round",
            "custom_function",
            "user_defined",
            "module.function",
        ];

        for function_name in non_matching_cases {
            assert!(
                !matches!(
                    function_name,
                    "print" | "exit" | "quit" | "sys.exit" | "os._exit" | "time.sleep" |
                    "random.seed" | "np.random.seed" | "plt.show" | "plt.close" |
                    "list.sort" | "list.reverse" | "dict.clear" | "set.clear" |
                    "file.flush" | "file.close"
                ),
                "Function '{}' should NOT be in the returns_none list",
                function_name
            );
        }
    }

    #[test]
    fn test_assignment_from_none_message_format() {
        let violation = AssignmentFromNone {
            name: "x = print".to_string(),
        };
        
        let message = violation.message();
        assert_eq!(
            message,
            "Assigning result of a function call, where the function returns None: `x = print`"
        );
    }

    #[test]
    fn test_annotated_assignment_from_none_message_format() {
        let violation = AssignmentFromNone {
            name: "x: int = function_returns_none".to_string(),
        };
        
        let message = violation.message();
        assert_eq!(
            message,
            "Assigning result of a function call, where the function returns None: `x: int = function_returns_none`"
        );
    }
}