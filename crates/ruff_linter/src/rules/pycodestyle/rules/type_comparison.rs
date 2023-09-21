use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for object type comparisons without using `isinstance()`.
///
/// ## Why is this bad?
/// Do not compare types directly.
///
/// When checking if an object is a instance of a certain type, keep in mind
/// that it might be subclassed. For example, `bool` inherits from `int`, and
/// `Exception` inherits from `BaseException`.
///
/// ## Example
/// ```python
/// if type(obj) is type(1):
///     pass
///
/// if type(obj) is int:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if isinstance(obj, int):
///     pass
/// ```
#[violation]
pub struct TypeComparison;

impl Violation for TypeComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not compare types, use `isinstance()`")
    }
}

/// E721
pub(crate) fn type_comparison(checker: &mut Checker, compare: &ast::ExprCompare) {
    for ((left, right), op) in std::iter::once(compare.left.as_ref())
        .chain(compare.comparators.iter())
        .tuple_windows()
        .zip(compare.ops.iter())
    {
        if !matches!(op, CmpOp::Is | CmpOp::IsNot | CmpOp::Eq | CmpOp::NotEq) {
            continue;
        }

        // Left-hand side must be, e.g., `type(obj)`.
        let Expr::Call(ast::ExprCall { func, .. }) = left else {
            continue;
        };

        let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
            continue;
        };

        if !(id == "type" && checker.semantic().is_builtin("type")) {
            continue;
        }

        // Right-hand side must be, e.g., `type(1)` or `int`.
        match right {
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                // Ex) `type(obj) is type(1)`
                let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
                    continue;
                };

                if id == "type" && checker.semantic().is_builtin("type") {
                    // Allow comparison for types which are not obvious.
                    if arguments
                        .args
                        .first()
                        .is_some_and(|arg| !arg.is_name_expr() && !is_const_none(arg))
                    {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(TypeComparison, compare.range()));
                    }
                }
            }
            Expr::Attribute(ast::ExprAttribute { value, .. }) => {
                // Ex) `type(obj) is types.NoneType`
                if checker
                    .semantic()
                    .resolve_call_path(value.as_ref())
                    .is_some_and(|call_path| matches!(call_path.as_slice(), ["types", ..]))
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(TypeComparison, compare.range()));
                }
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                // Ex) `type(obj) is int`
                if matches!(
                    id.as_str(),
                    "int"
                        | "str"
                        | "float"
                        | "bool"
                        | "complex"
                        | "bytes"
                        | "list"
                        | "dict"
                        | "set"
                ) && checker.semantic().is_builtin(id)
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(TypeComparison, compare.range()));
                }
            }
            _ => {}
        }
    }
}
