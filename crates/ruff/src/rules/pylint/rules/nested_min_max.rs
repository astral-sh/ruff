use ruff_python_ast::{self as ast, Arguments, Expr, Keyword};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::SemanticModel;

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
        let NestedMinMax { func } = self;
        format!("Nested `{func}` calls can be flattened")
    }

    fn autofix_title(&self) -> Option<String> {
        let NestedMinMax { func } = self;
        Some(format!("Flatten nested `{func}` calls"))
    }
}

impl MinMax {
    /// Converts a function call [`Expr`] into a [`MinMax`] if it is a call to `min` or `max`.
    fn try_from_call(
        func: &Expr,
        keywords: &[Keyword],
        semantic: &SemanticModel,
    ) -> Option<MinMax> {
        if !keywords.is_empty() {
            return None;
        }
        let Expr::Name(ast::ExprName { id, .. }) = func else {
            return None;
        };
        if id.as_str() == "min" && semantic.is_builtin("min") {
            Some(MinMax::Min)
        } else if id.as_str() == "max" && semantic.is_builtin("max") {
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
fn collect_nested_args(min_max: MinMax, args: &[Expr], semantic: &SemanticModel) -> Vec<Expr> {
    fn inner(min_max: MinMax, args: &[Expr], semantic: &SemanticModel, new_args: &mut Vec<Expr>) {
        for arg in args {
            if let Expr::Call(ast::ExprCall {
                func,
                arguments:
                    Arguments {
                        args,
                        keywords,
                        range: _,
                    },
                range: _,
            }) = arg
            {
                if let [arg] = args.as_slice() {
                    if arg.as_starred_expr().is_none() {
                        let new_arg = Expr::Starred(ast::ExprStarred {
                            value: Box::new(arg.clone()),
                            ctx: ast::ExprContext::Load,
                            range: TextRange::default(),
                        });
                        new_args.push(new_arg);
                        continue;
                    }
                }
                if MinMax::try_from_call(func, keywords, semantic) == Some(min_max) {
                    inner(min_max, args, semantic, new_args);
                    continue;
                }
            }
            new_args.push(arg.clone());
        }
    }

    let mut new_args = Vec::with_capacity(args.len());
    inner(min_max, args, semantic, &mut new_args);
    new_args
}

/// PLW3301
pub(crate) fn nested_min_max(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(min_max) = MinMax::try_from_call(func, keywords, checker.semantic()) else {
        return;
    };

    if matches!(&args, [Expr::Call(ast::ExprCall { arguments: Arguments {args, .. }, .. })] if args.len() == 1)
    {
        return;
    }

    if args.iter().any(|arg| {
        let Expr::Call(ast::ExprCall {
            func,
            arguments: Arguments { keywords, .. },
            ..
        }) = arg
        else {
            return false;
        };
        MinMax::try_from_call(func.as_ref(), keywords.as_ref(), checker.semantic()) == Some(min_max)
    }) {
        let mut diagnostic = Diagnostic::new(NestedMinMax { func: min_max }, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            if !checker.indexer().has_comments(expr, checker.locator()) {
                let flattened_expr = Expr::Call(ast::ExprCall {
                    func: Box::new(func.clone()),
                    arguments: Arguments {
                        args: collect_nested_args(min_max, args, checker.semantic()),
                        keywords: keywords.to_owned(),
                        range: TextRange::default(),
                    },
                    range: TextRange::default(),
                });
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    checker.generator().expr(&flattened_expr),
                    expr.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
