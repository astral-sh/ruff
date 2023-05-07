use ruff_text_size::TextSize;
use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_comments, unparse_expr};
use ruff_python_semantic::context::Context;

use crate::{checkers::ast::Checker, registry::AsRule};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinMax {
    Min,
    Max,
}

#[violation]
pub struct NestedMinMax {
    func: MinMax,
    fixable: bool,
}

impl Violation for NestedMinMax {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Nested `{}` calls can be flattened", self.func)
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|NestedMinMax { func, .. }| format!("Flatten nested `{func}` calls"))
    }
}

impl MinMax {
    /// Converts a function call [`Expr`] into a [`MinMax`] if it is a call to `min` or `max`.
    fn try_from_func(func: &Expr, context: &Context) -> Option<MinMax> {
        let ExprKind::Name { id, .. } = func.node() else {
            return None;
        };
        if id == "min" && context.is_builtin("min") {
            Some(MinMax::Min)
        } else if id == "max" && context.is_builtin("max") {
            Some(MinMax::Max)
        } else {
            None
        }
    }

    /// Returns `true` if the passed [`Expr`] is a call to the same built-in function.
    fn is_call(self, expr: &Expr, context: &Context) -> bool {
        let ExprKind::Call { func, keywords, ..} = expr.node() else {
            return false;
        };
        keywords.is_empty() && MinMax::try_from_func(func, context) == Some(self)
    }
}

impl std::fmt::Display for MinMax {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MinMax::Min => write!(f, "min"),
            MinMax::Max => write!(f, "max"),
        }
    }
}

/// Collect a new set of arguments to by either accepting existing args as-is or
/// collecting child arguments, if it's a call to the same function.
fn collect_nested_args(context: &Context, target_func: MinMax, args: &[Expr]) -> Vec<Expr> {
    fn inner(context: &Context, target_func: MinMax, args: &[Expr], new_args: &mut Vec<Expr>) {
        for arg in args {
            if let ExprKind::Call {
                func,
                args,
                keywords,
            } = arg.node()
            {
                if MinMax::try_from_func(func, context) == Some(target_func) && keywords.is_empty()
                {
                    inner(context, target_func, args, new_args);
                    continue;
                }
            }
            new_args.push(arg.clone());
        }
    }

    let mut new_args = Vec::with_capacity(args.len());
    inner(context, target_func, args, &mut new_args);
    new_args
}

/// W3301
pub fn nested_min_max(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !keywords.is_empty() {
        return;
    };

    let Some(min_max) = MinMax::try_from_func(func, &checker.ctx) else {
        return;
    };

    if args.iter().any(|arg| min_max.is_call(arg, &checker.ctx)) {
        let fixable = !has_comments(expr, checker.locator);
        let mut diagnostic = Diagnostic::new(
            NestedMinMax {
                func: min_max,
                fixable,
            },
            expr.range(),
        );
        if fixable && checker.patch(diagnostic.kind.rule()) {
            let flattened_expr = Expr::new(
                TextSize::default(),
                TextSize::default(),
                ExprKind::Call {
                    func: Box::new(func.clone()),
                    args: collect_nested_args(&checker.ctx, min_max, args),
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
