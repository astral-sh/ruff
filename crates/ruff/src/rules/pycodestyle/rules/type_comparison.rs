use itertools::izip;
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for object type comparisons without using isinstance().
///
/// ## Why is this bad?
/// Do not compare types directly.
/// When checking if an object is a instance of a certain type, keep in mind that it might
/// be subclassed. E.g. `bool` inherits from `int` or `Exception` inherits from `BaseException`.
///
/// ## Example
/// ```python
/// if type(obj) is type(1):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if isinstance(obj, int):
///     pass
/// if type(a1) is type(b1):
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
pub(crate) fn type_comparison(
    checker: &mut Checker,
    expr: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    for (op, right) in izip!(ops, comparators) {
        if !matches!(op, CmpOp::Is | CmpOp::IsNot | CmpOp::Eq | CmpOp::NotEq) {
            continue;
        }
        match right {
            Expr::Call(ast::ExprCall { func, args, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                    // Ex) `type(False)`
                    if id == "type" && checker.semantic().is_builtin("type") {
                        if let Some(arg) = args.first() {
                            // Allow comparison for types which are not obvious.
                            if !matches!(
                                arg,
                                Expr::Name(_)
                                    | Expr::Constant(ast::ExprConstant {
                                        value: Constant::None,
                                        kind: None,
                                        range: _
                                    })
                            ) {
                                checker
                                    .diagnostics
                                    .push(Diagnostic::new(TypeComparison, expr.range()));
                            }
                        }
                    }
                }
            }
            Expr::Attribute(ast::ExprAttribute { value, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                    // Ex) `types.NoneType`
                    if id == "types"
                        && checker
                            .semantic()
                            .resolve_call_path(value)
                            .map_or(false, |call_path| {
                                call_path.first().map_or(false, |module| *module == "types")
                            })
                    {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(TypeComparison, expr.range()));
                    }
                }
            }
            _ => {}
        }
    }
}
