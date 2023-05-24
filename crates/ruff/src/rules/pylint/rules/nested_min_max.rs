use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Expr, Keyword, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::has_comments;
use ruff_python_semantic::model::SemanticModel;

use crate::{checkers::ast::Checker, registry::AsRule};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinMax {
    Min,
    Max,
}

/// ## What it does
/// Checks for nested `min` and `max` calls.
///
/// ## Why is this bad?
/// Nested `min` and `max` calls can be flattened into a single call to improve
/// readability.
///
/// ## Example
/// ```python
/// minimum = min(1, 2, min(3, 4, 5))
/// maximum = max(1, 2, max(3, 4, 5))
/// diff = maximum - minimum
/// ```
///
/// Use instead:
/// ```python
/// minimum = min(1, 2, 3, 4, 5)
/// maximum = max(1, 2, 3, 4, 5)
/// diff = maximum - minimum
/// ```
///
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3/library/functions.html#max)
#[violation]
pub struct NestedMinMax {
    func: MinMax,
}

impl Violation for NestedMinMax {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Nested `{}` calls can be flattened", self.func)
    }

    fn autofix_title(&self) -> Option<String> {
        let NestedMinMax { func } = self;
        Some(format!("Flatten nested `{func}` calls"))
    }
}

impl MinMax {
    /// Converts a function call [`Expr`] into a [`MinMax`] if it is a call to `min` or `max`.
    fn try_from_call(func: &Expr, keywords: &[Keyword], model: &SemanticModel) -> Option<MinMax> {
        if !keywords.is_empty() {
            return None;
        }
        let Expr::Name(ast::ExprName { id, .. }) = func else {
            return None;
        };
        if id.as_str() == "min" && model.is_builtin("min") {
            Some(MinMax::Min)
        } else if id.as_str() == "max" && model.is_builtin("max") {
            Some(MinMax::Max)
        } else {
            None
        }
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
fn collect_nested_args(model: &SemanticModel, min_max: MinMax, args: &[Expr]) -> Vec<Expr> {
    fn inner(model: &SemanticModel, min_max: MinMax, args: &[Expr], new_args: &mut Vec<Expr>) {
        for arg in args {
            if let Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range: _,
            }) = arg
            {
                if args.len() == 1 {
                    let new_arg = Expr::Starred(ast::ExprStarred {
                        value: Box::new(args[0].clone()),
                        ctx: ast::ExprContext::Load,
                        range: TextRange::default(),
                    });
                    new_args.push(new_arg);
                    continue;
                }
                if MinMax::try_from_call(func, keywords, model) == Some(min_max) {
                    inner(model, min_max, args, new_args);
                    continue;
                }
            }
            new_args.push(arg.clone());
        }
    }

    let mut new_args = Vec::with_capacity(args.len());
    inner(model, min_max, args, &mut new_args);
    new_args
}

/// W3301
pub(crate) fn nested_min_max(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(min_max) = MinMax::try_from_call(func, keywords, checker.semantic_model()) else {
        return;
    };

    if args.iter().any(|arg| {
        let Expr::Call(ast::ExprCall { func, keywords, ..} )= arg else {
            return false;
        };
        MinMax::try_from_call(func.as_ref(), keywords.as_ref(), checker.semantic_model())
            == Some(min_max)
    }) {
        let fixable = !has_comments(expr, checker.locator);
        let mut diagnostic = Diagnostic::new(NestedMinMax { func: min_max }, expr.range());
        if fixable && checker.patch(diagnostic.kind.rule()) {
            let flattened_expr = Expr::Call(ast::ExprCall {
                func: Box::new(func.clone()),
                args: collect_nested_args(checker.semantic_model(), min_max, args),
                keywords: keywords.to_owned(),
                range: TextRange::default(),
            });
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                checker.generator().expr(&flattened_expr),
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
