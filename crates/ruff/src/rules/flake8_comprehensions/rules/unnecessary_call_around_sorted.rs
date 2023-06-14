use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

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
#[violation]
pub struct UnnecessaryCallAroundSorted {
    func: String,
}

impl AlwaysAutofixableViolation for UnnecessaryCallAroundSorted {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCallAroundSorted { func } = self;
        format!("Unnecessary `{func}` call around `sorted()`")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryCallAroundSorted { func } = self;
        format!("Remove unnecessary `{func}` call")
    }
}

/// C413
pub(crate) fn unnecessary_call_around_sorted(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(outer) = helpers::expr_name(func) else {
        return;
    };
    if !(outer == "list" || outer == "reversed") {
        return;
    }
    let Some(arg) = args.first() else {
        return;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = arg else {
        return;
    };
    let Some(inner) = helpers::expr_name(func) else {
        return;
    };
    if inner != "sorted" {
        return;
    }
    if !checker.semantic().is_builtin(inner) || !checker.semantic().is_builtin(outer) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryCallAroundSorted {
            func: outer.to_string(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            let edit =
                fixes::fix_unnecessary_call_around_sorted(checker.locator, checker.stylist, expr)?;
            if outer == "reversed" {
                Ok(Fix::suggested(edit))
            } else {
                Ok(Fix::automatic(edit))
            }
        });
    }
    checker.diagnostics.push(diagnostic);
}
