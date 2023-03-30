use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary list comprehensions.
///
/// ## Why is it bad?
/// It's unnecessary to use a list comprehension inside a call to `dict`,
/// since there is an equivalent comprehension for this type.
///
/// ## Examples
/// ```python
/// dict([(x, f(x)) for x in foo])
/// ```
///
/// Use instead:
/// ```python
/// {x: f(x) for x in foo}
/// ```
#[violation]
pub struct UnnecessaryListComprehensionDict;

impl AlwaysAutofixableViolation for UnnecessaryListComprehensionDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` comprehension (rewrite as a `dict` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` comprehension".to_string()
    }
}

/// C404 (`dict([...])`)
pub fn unnecessary_list_comprehension_dict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function("dict", func, args, keywords) else {
        return;
    };
    if !checker.ctx.is_builtin("dict") {
        return;
    }
    let ExprKind::ListComp { elt, .. } = &argument else {
        return;
    };
    let ExprKind::Tuple { elts, .. } = &elt.node else {
        return;
    };
    if elts.len() != 2 {
        return;
    }
    let mut diagnostic = Diagnostic::new(UnnecessaryListComprehensionDict, Range::from(expr));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_list_comprehension_dict(checker.locator, checker.stylist, expr)
        });
    }
    checker.diagnostics.push(diagnostic);
}
