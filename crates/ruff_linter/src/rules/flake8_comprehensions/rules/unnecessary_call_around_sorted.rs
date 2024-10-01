use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary `list` or `reversed` calls around `sorted`
/// calls.
///
/// ## Why is this bad?
/// It is unnecessary to use `list` around `sorted`, as the latter already
/// returns a list.
///
/// It is also unnecessary to use `reversed` around `sorted`, as the latter
/// has a `reverse` argument that can be used in lieu of an additional
/// `reversed` call.
///
/// In both cases, it's clearer to avoid the redundant call.
///
/// ## Examples
/// ```python
/// reversed(sorted(iterable))
/// ```
///
/// Use instead:
/// ```python
/// sorted(iterable, reverse=True)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as `reversed` and `reverse=True` will
/// yield different results in the event of custom sort keys or equality
/// functions. Specifically, `reversed` will reverse the order of the
/// collection, while `sorted` with `reverse=True` will perform a stable
/// reverse sort, which will preserve the order of elements that compare as
/// equal.
#[violation]
pub struct UnnecessaryCallAroundSorted {
    func: String,
}

impl AlwaysFixableViolation for UnnecessaryCallAroundSorted {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCallAroundSorted { func } = self;
        format!("Unnecessary `{func}` call around `sorted()`")
    }

    fn fix_title(&self) -> String {
        let UnnecessaryCallAroundSorted { func } = self;
        format!("Remove unnecessary `{func}` call")
    }
}

/// C413
pub(crate) fn unnecessary_call_around_sorted(
    checker: &mut Checker,
    expr: &Expr,
    outer_func: &Expr,
    args: &[Expr],
) {
    let Some(Expr::Call(ast::ExprCall {
        func: inner_func, ..
    })) = args.first()
    else {
        return;
    };
    let semantic = checker.semantic();
    let Some(outer_func_name) = semantic.resolve_builtin_symbol(outer_func) else {
        return;
    };
    if !matches!(outer_func_name, "list" | "reversed") {
        return;
    }
    if !semantic.match_builtin_expr(inner_func, "sorted") {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryCallAroundSorted {
            func: outer_func_name.to_string(),
        },
        expr.range(),
    );
    diagnostic.try_set_fix(|| {
        Ok(Fix::applicable_edit(
            fixes::fix_unnecessary_call_around_sorted(expr, checker.locator(), checker.stylist())?,
            if outer_func_name == "reversed" {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ))
    });
    checker.diagnostics.push(diagnostic);
}
