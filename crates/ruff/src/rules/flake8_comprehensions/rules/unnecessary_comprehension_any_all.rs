use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary list comprehensions passed to `any` and `all`.
///
/// ## Why is this bad?
/// `any` and `all` take any iterators, including generators. Converting a generator to a list
/// by way of a list comprehension is unnecessary and reduces performance due to the
/// overhead of creating the list.
///
/// For example, compare the performance of `all` with a list comprehension against that
/// of a generator (~40x faster here):
///
/// ```console
/// In [1]: %timeit all([i for i in range(1000)])
/// 8.14 µs ± 25.4 ns per loop (mean ± std. dev. of 7 runs, 100,000 loops each)
///
/// In [2]: %timeit all(i for i in range(1000))
/// 212 ns ± 0.892 ns per loop (mean ± std. dev. of 7 runs, 1,000,000 loops each)
/// ```
///
/// ## Examples
/// ```python
/// any([x.id for x in bar])
/// all([x.id for x in bar])
/// ```
///
/// Use instead:
/// ```python
/// any(x.id for x in bar)
/// all(x.id for x in bar)
/// ```
#[violation]
pub struct UnnecessaryComprehensionAnyAll;

impl AlwaysAutofixableViolation for UnnecessaryComprehensionAnyAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary list comprehension.")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary list comprehension".to_string()
    }
}

/// C419
pub fn unnecessary_comprehension_any_all(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !keywords.is_empty() {
        return;
    }
    let ExprKind::Name { id, .. } = &func.node  else {
        return;
    };
    if (matches!(id.as_str(), "all" | "any")) && args.len() == 1 {
        let (ExprKind::ListComp { elt, .. } | ExprKind::SetComp { elt, .. }) = &args[0].node else {
            return;
        };
        if is_async_generator(elt) {
            return;
        }
        if !checker.ctx.is_builtin(id) {
            return;
        }
        let mut diagnostic = Diagnostic::new(UnnecessaryComprehensionAnyAll, Range::from(&args[0]));
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                fixes::fix_unnecessary_comprehension_any_all(checker.locator, checker.stylist, expr)
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// Return `true` if the `Expr` contains an `await` expression.
fn is_async_generator(expr: &Expr) -> bool {
    any_over_expr(expr, &|expr| matches!(expr.node, ExprKind::Await { .. }))
}
