use itertools::izip;
use rustpython_parser::ast::{self, Cmpop, Constant, Expr, ExprKind};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    for (op, right) in izip!(ops, comparators) {
        if !matches!(op, Cmpop::Is | Cmpop::IsNot | Cmpop::Eq | Cmpop::NotEq) {
            continue;
        }
        match &right.node {
            ExprKind::Call(ast::ExprCall { func, args, .. }) => {
                if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
                    // Ex) `type(False)`
                    if id == "type" && checker.ctx.is_builtin("type") {
                        if let Some(arg) = args.first() {
                            // Allow comparison for types which are not obvious.
                            if !matches!(
                                arg.node,
                                ExprKind::Name(_)
                                    | ExprKind::Constant(ast::ExprConstant {
                                        value: Constant::None,
                                        kind: None
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
            ExprKind::Attribute(ast::ExprAttribute { value, .. }) => {
                if let ExprKind::Name(ast::ExprName { id, .. }) = &value.node {
                    // Ex) `types.NoneType`
                    if id == "types"
                        && checker
                            .ctx
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
