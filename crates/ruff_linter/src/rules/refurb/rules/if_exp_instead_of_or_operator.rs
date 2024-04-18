use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for ternary `if` expressions that can be replaced with the `or`
/// operator.
///
/// ## Why is this bad?
/// Ternary `if` expressions are more verbose than `or` expressions while
/// providing the same functionality.
///
/// ## Example
/// ```python
/// z = x if x else y
/// ```
///
/// Use instead:
/// ```python
/// z = x or y
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe in the event that the body of the
/// `if` expression contains side effects.
///
/// For example, `foo` will be called twice in `foo() if foo() else bar()`
/// (assuming `foo()` returns a truthy value), but only once in
/// `foo() or bar()`.
#[violation]
pub struct IfExpInsteadOfOrOperator;

impl Violation for IfExpInsteadOfOrOperator {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace ternary `if` expression with `or` operator")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `or` operator"))
    }
}

/// FURB110
pub(crate) fn if_exp_instead_of_or_operator(checker: &mut Checker, if_expr: &ast::ExprIf) {
    let ast::ExprIf {
        test,
        body,
        orelse,
        range,
    } = if_expr;

    if ComparableExpr::from(test) != ComparableExpr::from(body) {
        return;
    }

    let mut diagnostic = Diagnostic::new(IfExpInsteadOfOrOperator, *range);

    // Grab the range of the `test` and `orelse` expressions.
    let left = parenthesized_range(
        test.into(),
        if_expr.into(),
        checker.indexer().comment_ranges(),
        checker.locator().contents(),
    )
    .unwrap_or(test.range());
    let right = parenthesized_range(
        orelse.into(),
        if_expr.into(),
        checker.indexer().comment_ranges(),
        checker.locator().contents(),
    )
    .unwrap_or(orelse.range());

    // Replace with `{test} or {orelse}`.
    diagnostic.set_fix(Fix::applicable_edit(
        Edit::range_replacement(
            format!(
                "{} or {}",
                checker.locator().slice(left),
                checker.locator().slice(right),
            ),
            if_expr.range(),
        ),
        if contains_effect(body, |id| checker.semantic().has_builtin_binding(id)) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        },
    ));

    checker.diagnostics.push(diagnostic);
}
