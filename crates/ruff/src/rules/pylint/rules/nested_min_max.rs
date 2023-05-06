use ruff_python_ast::helpers::{has_comments, unparse_expr};
use ruff_text_size::TextSize;
use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::{checkers::ast::Checker, registry::AsRule};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NestedMinMaxFunc {
    Min,
    Max,
}

impl NestedMinMaxFunc {
    /// Returns a value if this is a min() or max() call.
    fn from_func(func: &Expr, checker: &mut Checker) -> Option<NestedMinMaxFunc> {
        match func.node() {
            ExprKind::Name { id, .. } if id == "min" && checker.ctx.is_builtin("min") => {
                Some(NestedMinMaxFunc::Min)
            }
            ExprKind::Name { id, .. } if id == "max" && checker.ctx.is_builtin("max") => {
                Some(NestedMinMaxFunc::Max)
            }
            _ => None,
        }
    }

    /// Returns true if the passed expr is a call to the same function as self.
    fn is_call(self, expr: &Expr, checker: &mut Checker) -> bool {
        matches!(expr.node(), ExprKind::Call { func, keywords, ..} if NestedMinMaxFunc::from_func(func, checker) == Some(self) && keywords.is_empty())
    }
}

impl std::fmt::Display for NestedMinMaxFunc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NestedMinMaxFunc::Min => write!(f, "min()"),
            NestedMinMaxFunc::Max => write!(f, "max()"),
        }
    }
}

#[violation]
pub struct NestedMinMax {
    func: NestedMinMaxFunc,
    fixable: bool,
}

impl Violation for NestedMinMax {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Nested {} call", self.func)
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|NestedMinMax { func, .. }| format!("Flatten nested {} calls", func))
    }
}

/// Collect a new set of arguments to `target_func` by either accepting existing args as-is or
/// collecting child arguments if it is a call to the same function.
fn collect_nested_args(
    target_func: NestedMinMaxFunc,
    checker: &mut Checker,
    args: &[Expr],
    new_args: &mut Vec<Expr>,
) {
    for arg in args {
        match arg.node() {
            ExprKind::Call {
                func,
                args,
                keywords,
            } if NestedMinMaxFunc::from_func(func, checker) == Some(target_func)
                && keywords.is_empty() =>
            {
                collect_nested_args(target_func, checker, args, new_args);
            }
            _ => {
                new_args.push(arg.clone());
            }
        }
    }
}

/// W3301
pub fn nested_min_max(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(nested_func) = NestedMinMaxFunc::from_func(func, checker) else { return };
    // Do not analyze cases where keyword arguments are provided.
    if !keywords.is_empty() {
        return;
    };

    if args.iter().any(|arg| nested_func.is_call(arg, checker)) {
        let fixable = !has_comments(expr, checker.locator);
        let mut diagnostic = Diagnostic::new(
            NestedMinMax {
                func: nested_func,
                fixable,
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) && fixable {
            let mut new_args = Vec::with_capacity(args.len());
            collect_nested_args(nested_func, checker, args, &mut new_args);
            let flattened_expr = Expr::new(
                TextSize::default(),
                TextSize::default(),
                ExprKind::Call {
                    func: Box::new(func.clone()),
                    args: new_args,
                    keywords: keywords.to_owned(),
                },
            );
            diagnostic.set_fix(Edit::range_replacement(
                unparse_expr(&flattened_expr, checker.stylist),
                expr.range(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
