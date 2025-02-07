use std::borrow::Cow;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::Expr;
use ruff_python_trivia::CommentRanges;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::Locator;

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
#[derive(ViolationMetadata)]
pub(crate) struct IfExpInsteadOfOrOperator;

impl Violation for IfExpInsteadOfOrOperator {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Replace ternary `if` expression with `or` operator".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `or` operator".to_string())
    }
}

/// FURB110
pub(crate) fn if_exp_instead_of_or_operator(checker: &Checker, if_expr: &ast::ExprIf) {
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

    // Replace with `{test} or {orelse}`.
    diagnostic.set_fix(Fix::applicable_edit(
        Edit::range_replacement(
            format!(
                "{} or {}",
                parenthesize_test(test, if_expr, checker.comment_ranges(), checker.locator()),
                parenthesize_test(orelse, if_expr, checker.comment_ranges(), checker.locator()),
            ),
            if_expr.range(),
        ),
        if contains_effect(body, |id| checker.semantic().has_builtin_binding(id)) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        },
    ));

    checker.report_diagnostic(diagnostic);
}

/// Parenthesize an expression for use in an `or` operator (e.g., parenthesize `x` in `x or y`),
/// if it's required to maintain the correct order of operations.
///
/// If the expression is already parenthesized, it will be returned as-is regardless of whether
/// the parentheses are required.
///
/// See: <https://docs.python.org/3/reference/expressions.html#operator-precedence>
fn parenthesize_test<'a>(
    expr: &Expr,
    if_expr: &ast::ExprIf,
    comment_ranges: &CommentRanges,
    locator: &Locator<'a>,
) -> Cow<'a, str> {
    if let Some(range) = parenthesized_range(
        expr.into(),
        if_expr.into(),
        comment_ranges,
        locator.contents(),
    ) {
        Cow::Borrowed(locator.slice(range))
    } else if matches!(expr, Expr::If(_) | Expr::Lambda(_) | Expr::Named(_)) {
        Cow::Owned(format!("({})", locator.slice(expr.range())))
    } else {
        Cow::Borrowed(locator.slice(expr.range()))
    }
}
