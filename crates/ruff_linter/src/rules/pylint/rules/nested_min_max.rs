use ruff_python_ast::{BuildId, Builder, Expr, ExprKind, NodeList};
use ruff_text_size::Ranged;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
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
#[violation_metadata(stable_since = "v0.0.266", category = Category::Pedantic)]
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
        func: Expr<'_>,
        keywords: NodeList<'_, ruff_python_ast::kind::Keyword>,
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

/// Flattens calls to the same builtin, unpacking the iterable in a single-argument call.
fn collect_nested_args(
    builder: &mut Builder,
    min_max: MinMax,
    args: NodeList<'_, ruff_python_ast::kind::Expr>,
    semantic: &SemanticModel,
    flattened: &mut Vec<BuildId<ruff_python_ast::kind::Expr>>,
) {
    for arg in args {
        if let Some(call) = arg.as_call_expr()
            && MinMax::try_from_call(call.func(), call.arguments().keywords(), semantic)
                == Some(min_max)
        {
            let args = call.arguments().args();
            if let Some([single]) = args.as_array()
                && !single.is_starred_expr()
            {
                let value = builder.copy_node(single);
                flattened.push(syntax::starred(builder, value));
            } else {
                collect_nested_args(builder, min_max, args, semantic, flattened);
            }
        } else {
            flattened.push(builder.copy_node(arg));
        }
    }
}
/// PLW3301
pub(crate) fn nested_min_max(
    checker: &Checker,
    &expr: &Expr<'_>,
    func: Expr<'_>,
    args: NodeList<'_, ruff_python_ast::kind::Expr>,
    keywords: NodeList<'_, ruff_python_ast::kind::Keyword>,
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
        let ExprKind::Call(call_node) = arg.classify() else {
            return false;
        };
        let func = call_node.func();
        let arguments = call_node.arguments();
        let keywords = arguments.keywords();
        MinMax::try_from_call(func, keywords, checker.semantic()) == Some(min_max)
    }) {
        let mut diagnostic =
            checker.report_diagnostic(NestedMinMax { func: min_max }, expr.range());
        let mut builder = Builder::default();
        let mut flattened = Vec::with_capacity(args.len());
        collect_nested_args(
            &mut builder,
            min_max,
            args,
            checker.semantic(),
            &mut flattened,
        );
        let args = builder.list(flattened);
        let keywords = builder.copy_nodes(keywords);
        let func = builder.copy_node(func);
        let replacement = syntax::call_with_keywords(&mut builder, func, args, keywords);
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker.generator().expr(builder.get(replacement)),
            expr.range(),
        )));
    }
}
