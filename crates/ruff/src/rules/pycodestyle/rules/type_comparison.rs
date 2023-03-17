use itertools::izip;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
/// ```
///
/// Use instead:
/// ```python
/// if isinstance(obj, int):
/// if type(a1) is type(b1):
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
pub fn type_comparison(checker: &mut Checker, expr: &Expr, ops: &[Cmpop], comparators: &[Expr]) {
    for (op, right) in izip!(ops, comparators) {
        if !matches!(op, Cmpop::Is | Cmpop::IsNot | Cmpop::Eq | Cmpop::NotEq) {
            continue;
        }
        match &right.node {
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name { id, .. } = &func.node {
                    // Ex) `type(False)`
                    if id == "type" && checker.ctx.is_builtin("type") {
                        if let Some(arg) = args.first() {
                            // Allow comparison for types which are not obvious.
                            if !matches!(
                                arg.node,
                                ExprKind::Name { .. }
                                    | ExprKind::Constant {
                                        value: Constant::None,
                                        kind: None
                                    }
                            ) {
                                checker
                                    .diagnostics
                                    .push(Diagnostic::new(TypeComparison, Range::from(expr)));
                            }
                        }
                    }
                }
            }
            ExprKind::Attribute { value, .. } => {
                if let ExprKind::Name { id, .. } = &value.node {
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
                            .push(Diagnostic::new(TypeComparison, Range::from(expr)));
                    }
                }
            }
            _ => {}
        }
    }
}
