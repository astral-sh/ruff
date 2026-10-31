use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Expr;
use ruff_python_ast::flat;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_comprehensions::fixes;
use crate::{AlwaysFixableViolation, Fix};

/// ## What it does
/// Checks for unnecessary list comprehensions.
///
/// ## Why is this bad?
/// It's unnecessary to use a list comprehension inside a call to `dict()`,
/// since there is an equivalent comprehension for this type.
///
/// ## Example
/// ```python
/// dict([(x, f(x)) for x in foo])
/// ```
///
/// Use instead:
/// ```python
/// {x: f(x) for x in foo}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.58", category = Category::Complexity)]
pub(crate) struct UnnecessaryListComprehensionDict;

impl AlwaysFixableViolation for UnnecessaryListComprehensionDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary list comprehension (rewrite as a dict comprehension)".to_string()
    }

    fn fix_title(&self) -> String {
        "Rewrite as a dict comprehension".to_string()
    }
}

/// C404 (`dict([...])`)
pub(crate) fn unnecessary_list_comprehension_dict(
    checker: &Checker,
    expr: flat::View<'_, Expr>,
    argument: flat::View<'_, Expr>,
) {
    let flat::ExprKind::ListComp(list_comp) = argument.classify() else {
        return;
    };
    let elt = list_comp.elt();
    let flat::ExprKind::Tuple(tuple) = elt.classify() else {
        return;
    };
    if tuple.elts().len() != 2 {
        return;
    }
    if !checker.semantic().has_builtin_binding("dict") {
        return;
    }
    let mut diagnostic = checker.report_diagnostic(UnnecessaryListComprehensionDict, expr.range());
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_list_comprehension_dict(expr, checker).map(Fix::unsafe_edit)
    });
}
