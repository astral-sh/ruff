use ruff_python_ast::{self as ast, Expr, Keyword};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, FixAvailability};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary list comprehensions passed to builtin functions that take an iterable.
///
/// ## Why is this bad?
/// Many builtin functions (this rule currently covers `any`, `all`, `min`, `max`, and `sum`) take
/// any iterable, including a generator. Constructing a temporary list via list comprehension is
/// unnecessary and wastes memory for large iterables.
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
#[violation]
pub struct UnnecessaryComprehensionInCall;

impl Violation for UnnecessaryComprehensionInCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary list comprehension")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove unnecessary list comprehension".to_string())
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
    let [arg] = args else {
        return;
    };
    let (Expr::ListComp(ast::ExprListComp { elt, .. })
    | Expr::SetComp(ast::ExprSetComp { elt, .. })) = arg
    else {
        return;
    };
    if contains_await(elt) {
        return;
    }
    let Some(builtin_function) = checker.semantic().resolve_builtin_symbol(func) else {
        return;
    };
    if !(matches!(builtin_function, "any" | "all")
        || (checker.settings.preview.is_enabled()
            && matches!(builtin_function, "sum" | "min" | "max")))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryComprehensionInCall, arg.range());
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_comprehension_in_call(expr, checker.locator(), checker.stylist())
    });
    checker.diagnostics.push(diagnostic);
}

/// Return `true` if the [`Expr`] contains an `await` expression.
fn contains_await(expr: &Expr) -> bool {
    any_over_expr(expr, &Expr::is_await_expr)
}
