use ruff_diagnostics::{Diagnostic, FixAvailability};
use ruff_diagnostics::{Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Expr, Keyword};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary list or set comprehensions passed to builtin functions that take an iterable.
///
/// Set comprehensions are only a violation in the case where the builtin function does not care about
/// duplication of elements in the passed iterable.
///
/// ## Why is this bad?
/// Many builtin functions (this rule currently covers `any` and `all` in stable, along with `min`,
/// `max`, and `sum` in [preview]) accept any iterable, including a generator. Constructing a
/// temporary list via list comprehension is unnecessary and wastes memory for large iterables.
///
/// `any` and `all` can also short-circuit iteration, saving a lot of time. The unnecessary
/// comprehension forces a full iteration of the input iterable, giving up the benefits of
/// short-circuiting. For example, compare the performance of `all` with a list comprehension
/// against that of a generator in a case where an early short-circuit is possible (almost 40x
/// faster):
///
/// ```console
/// In [1]: %timeit all([i for i in range(1000)])
/// 8.14 µs ± 25.4 ns per loop (mean ± std. dev. of 7 runs, 100,000 loops each)
///
/// In [2]: %timeit all(i for i in range(1000))
/// 212 ns ± 0.892 ns per loop (mean ± std. dev. of 7 runs, 1,000,000 loops each)
/// ```
///
/// This performance improvement is due to short-circuiting. If the entire iterable has to be
/// traversed, the comprehension version may even be a bit faster: list allocation overhead is not
/// necessarily greater than generator overhead.
///
/// Applying this rule simplifies the code and will usually save memory, but in the absence of
/// short-circuiting it may not improve performance. (It may even slightly regress performance,
/// though the difference will usually be small.)
///
/// ## Examples
/// ```python
/// any([x.id for x in bar])
/// all([x.id for x in bar])
/// sum([x.val for x in bar])
/// min([x.val for x in bar])
/// max([x.val for x in bar])
/// ```
///
/// Use instead:
/// ```python
/// any(x.id for x in bar)
/// all(x.id for x in bar)
/// sum(x.val for x in bar)
/// min(x.val for x in bar)
/// max(x.val for x in bar)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it can change the behavior of the code if the iteration
/// has side effects (due to laziness and short-circuiting). The fix may also drop comments when
/// rewriting some comprehensions.
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[violation]
pub struct UnnecessaryComprehensionInCall {
    comprehension_kind: ComprehensionKind,
}

impl Violation for UnnecessaryComprehensionInCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.comprehension_kind {
            ComprehensionKind::List => format!("Unnecessary list comprehension"),
            ComprehensionKind::Set => format!("Unnecessary set comprehension"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove unnecessary comprehension".to_string())
    }
}

/// C419
pub(crate) fn unnecessary_comprehension_in_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !keywords.is_empty() {
        return;
    }
    let Some(arg) = args.first() else {
        return;
    };
    let (Expr::ListComp(ast::ExprListComp {
        elt, generators, ..
    })
    | Expr::SetComp(ast::ExprSetComp {
        elt, generators, ..
    })) = arg
    else {
        return;
    };
    if contains_await(elt) {
        return;
    }
    if generators.iter().any(|generator| generator.is_async) {
        return;
    }
    let Some(Ok(builtin_function)) = checker
        .semantic()
        .resolve_builtin_symbol(func)
        .map(SupportedBuiltins::try_from)
    else {
        return;
    };
    if !(matches!(
        builtin_function,
        SupportedBuiltins::Any | SupportedBuiltins::All
    ) || (checker.settings.preview.is_enabled()
        && matches!(
            builtin_function,
            SupportedBuiltins::Sum | SupportedBuiltins::Min | SupportedBuiltins::Max
        )))
    {
        return;
    }

    let mut diagnostic = match (arg, builtin_function.duplication_variance()) {
        (Expr::ListComp(_), _) => Diagnostic::new(
            UnnecessaryComprehensionInCall {
                comprehension_kind: ComprehensionKind::List,
            },
            arg.range(),
        ),
        (Expr::SetComp(_), DuplicationVariance::Invariant) => Diagnostic::new(
            UnnecessaryComprehensionInCall {
                comprehension_kind: ComprehensionKind::Set,
            },
            arg.range(),
        ),
        _ => {
            return;
        }
    };
    if args.len() == 1 {
        // If there's only one argument, remove the list or set brackets.
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_comprehension_in_call(expr, checker.locator(), checker.stylist())
        });
    } else {
        // If there are multiple arguments, replace the list or set brackets with parentheses.
        // If a function call has multiple arguments, one of which is a generator, then the
        // generator must be parenthesized.

        // Replace `[` with `(`.
        let collection_start = Edit::replacement(
            "(".to_string(),
            arg.start(),
            arg.start() + TextSize::from(1),
        );

        // Replace `]` with `)`.
        let collection_end =
            Edit::replacement(")".to_string(), arg.end() - TextSize::from(1), arg.end());

        diagnostic.set_fix(Fix::unsafe_edits(collection_start, [collection_end]));
    }
    checker.diagnostics.push(diagnostic);
}

/// Return `true` if the [`Expr`] contains an `await` expression.
fn contains_await(expr: &Expr) -> bool {
    any_over_expr(expr, &Expr::is_await_expr)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum DuplicationVariance {
    Invariant,
    Variant,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ComprehensionKind {
    List,
    Set,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SupportedBuiltins {
    All,
    Any,
    Sum,
    Min,
    Max,
}

impl TryFrom<&str> for SupportedBuiltins {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "all" => Ok(Self::All),
            "any" => Ok(Self::Any),
            "sum" => Ok(Self::Sum),
            "min" => Ok(Self::Min),
            "max" => Ok(Self::Max),
            _ => Err("Unsupported builtin for `unnecessary-comprehension-in-call`"),
        }
    }
}

impl SupportedBuiltins {
    fn duplication_variance(self) -> DuplicationVariance {
        match self {
            SupportedBuiltins::All
            | SupportedBuiltins::Any
            | SupportedBuiltins::Min
            | SupportedBuiltins::Max => DuplicationVariance::Invariant,
            SupportedBuiltins::Sum => DuplicationVariance::Variant,
        }
    }
}
