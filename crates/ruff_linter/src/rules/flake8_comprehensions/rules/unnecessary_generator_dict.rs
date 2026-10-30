use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Expr;
use ruff_python_ast::flat;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Fix};

use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as dict
/// comprehensions.
///
/// ## Why is this bad?
/// It is unnecessary to use `dict()` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// ## Example
/// ```python
/// dict((x, f(x)) for x in foo)
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
#[violation_metadata(stable_since = "v0.0.61", category = Category::Complexity)]
pub(crate) struct UnnecessaryGeneratorDict;

impl AlwaysFixableViolation for UnnecessaryGeneratorDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary generator (rewrite as a dict comprehension)".to_string()
    }

    fn fix_title(&self) -> String {
        "Rewrite as a dict comprehension".to_string()
    }
}

/// C402 (`dict((x, y) for x, y in iterable)`)
pub(crate) fn unnecessary_generator_dict(
    checker: &Checker,
    expr: flat::View<'_, Expr>,
    argument: flat::View<'_, Expr>,
) {
    let flat::ExprKind::Generator(generator_node) = argument.classify() else {
        return;
    };
    let elt = generator_node.elt();
    let flat::ExprKind::Tuple(tuple) = elt.classify() else {
        return;
    };
    if tuple.elts().len() != 2 {
        return;
    }
    if tuple.iter().any(flat::View::is_starred_expr) {
        return;
    }
    let mut diagnostic = checker.report_diagnostic(UnnecessaryGeneratorDict, expr.range());
    diagnostic
        .try_set_fix(|| fixes::fix_unnecessary_generator_dict(expr, checker).map(Fix::unsafe_edit));
}
