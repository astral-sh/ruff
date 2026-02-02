use ruff_python_ast::{self as ast, Arguments, Expr, Keyword};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

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
///
/// ```python
/// minimum = min(1, 2, min(3, 4, 5))
/// maximum = max(1, 2, max(3, 4, 5))
/// diff = maximum - minimum
/// ```
///
/// Use instead:
///
/// ```python
/// minimum = min(1, 2, 3, 4, 5)
/// maximum = max(1, 2, 3, 4, 5)
/// diff = maximum - minimum
/// ```
///
/// ## Known issues
///
/// The resulting code may be slower and use more memory, especially for nested iterables. For
/// example, this code:
///
/// ```python
/// iterable = range(3)
/// min(1, min(iterable))
/// ```
///
/// will be fixed to:
///
/// ```python
/// iterable = range(3)
/// min(1, *iterable)
/// ```
///
/// At least on current versions of CPython, this allocates a collection for the whole iterable
/// before calling `min` and could cause performance regressions, at least for large iterables.
///
/// ## Fix safety
///
/// This fix is always unsafe and may change the program's behavior for types without full
/// equivalence relations, such as float comparisons involving `NaN`.
///
/// ```python
/// print(min(2.0, min(float("nan"), 1.0)))  # before fix: 2.0
/// print(min(2.0, float("nan"), 1.0))  # after fix: 1.0
///
/// print(max(1.0, max(float("nan"), 2.0)))  # before fix: 1.0
/// print(max(1.0, float("nan"), 2.0))  # after fix: 2.0
/// ```
///
/// The fix will also remove any comments within the outer call.
///
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3/library/functions.html#max)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.266")]
pub(crate) struct NestedMinMax {
    func: MinMax,
}

impl Violation for NestedMinMax {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]

    fn message(&self) -> String {
        let NestedMinMax { func } = self;
        format!("Nested `{func}` calls can be flattened")
    }

    fn fix_title(&self) -> Option<String> {
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
        match semantic.resolve_builtin_symbol(func)? {
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            _ => None,
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
                        node_index: _,
                    },
                range: _,
                node_index: _,
            }) = arg
            {
                if MinMax::try_from_call(func, keywords, semantic) == Some(min_max) {
                    if let [arg] = &**args {
                        if arg.as_starred_expr().is_none() {
                            let new_arg = Expr::Starred(ast::ExprStarred {
                                value: Box::new(arg.clone()),
                                ctx: ast::ExprContext::Load,
                                range: TextRange::default(),
                                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                            });
                            new_args.push(new_arg);
                            continue;
                        }
                    }
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
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(min_max) = MinMax::try_from_call(func, keywords, checker.semantic()) else {
        return;
    };
    // It's only safe to flatten nested calls if the outer call has more than one argument.
    // When the outer call has a single argument, flattening would change the semantics by
    // changing the shape of the call from treating the inner result as an iterable (or a scalar)
    // to passing multiple arguments directly, which can lead to behavioral changes.
    if args.len() < 2 {
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
        let mut diagnostic =
            checker.report_diagnostic(NestedMinMax { func: min_max }, expr.range());
        let flattened_expr = Expr::Call(ast::ExprCall {
            func: Box::new(func.clone()),
            arguments: Arguments {
                args: collect_nested_args(min_max, args, checker.semantic()).into_boxed_slice(),
                keywords: Box::from(keywords),
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            },
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        });
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker.generator().expr(&flattened_expr),
            expr.range(),
        )));
    }
}
