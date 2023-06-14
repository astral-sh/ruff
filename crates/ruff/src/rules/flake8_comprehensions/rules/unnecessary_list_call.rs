use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary `list` calls around list comprehensions.
///
/// ## Why is this bad?
/// It is redundant to use a `list` call around a list comprehension.
///
/// ## Examples
/// ```python
/// list([f(x) for x in foo])
/// ```
///
/// Use instead
/// ```python
/// [f(x) for x in foo]
/// ```
#[violation]
pub struct UnnecessaryListCall;

impl AlwaysAutofixableViolation for UnnecessaryListCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` call (remove the outer call to `list()`)")
    }

    fn autofix_title(&self) -> String {
        "Remove outer `list` call".to_string()
    }
}

/// C411
pub(crate) fn unnecessary_list_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(argument) = helpers::first_argument_with_matching_function("list", func, args) else {
        return;
    };
    if !checker.semantic().is_builtin("list") {
        return;
    }
    if !argument.is_list_comp_expr() {
        return;
    }
    let mut diagnostic = Diagnostic::new(UnnecessaryListCall, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.try_set_fix_from_edit(|| {
            fixes::fix_unnecessary_list_call(checker.locator, checker.stylist, expr)
        });
    }
    checker.diagnostics.push(diagnostic);
}
