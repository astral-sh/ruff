use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysAutofixableViolation, AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_const_false, is_const_true};
use ruff_python_ast::parenthesize::parenthesized_range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `if` expressions that can be replaced with `bool()` calls.
///
/// ## Why is this bad?
/// `if` expressions that evaluate to `True` for a truthy condition an `False`
/// for a falsey condition can be replaced with `bool()` calls, which are more
/// concise and readable.
///
/// ## Example
/// ```python
/// True if a else False
/// ```
///
/// Use instead:
/// ```python
/// bool(a)
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[violation]
pub struct IfExprWithTrueFalse {
    is_compare: bool,
}

impl Violation for IfExprWithTrueFalse {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTrueFalse { is_compare } = self;
        if *is_compare {
            format!("Remove unnecessary `True if ... else False`")
        } else {
            format!("Use `bool(...)` instead of `True if ... else False`")
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let IfExprWithTrueFalse { is_compare } = self;
        if *is_compare {
            Some(format!("Remove unnecessary `True if ... else False`"))
        } else {
            Some(format!("Replace with `bool(...)"))
        }
    }
}

/// ## What it does
/// Checks for `if` expressions that can be replaced by negating a given
/// condition.
///
/// ## Why is this bad?
/// `if` expressions that evaluate to `False` for a truthy condition an `True`
/// for a falsey condition can be replaced with `not` operators, which are more
/// concise and readable.
///
/// ## Example
/// ```python
/// False if a else True
/// ```
///
/// Use instead:
/// ```python
/// not a
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[violation]
pub struct IfExprWithFalseTrue;

impl AlwaysAutofixableViolation for IfExprWithFalseTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `not ...` instead of `False if ... else True`")
    }

    fn autofix_title(&self) -> String {
        format!("Replace with `not ...`")
    }
}

/// ## What it does
/// Checks for `if` expressions that check against a negated condition.
///
/// ## Why is this bad?
/// `if` expressions that check against a negated condition are more difficult
/// to read than `if` expressions that check against the condition directly.
///
/// ## Example
/// ```python
/// b if not a else a
/// ```
///
/// Use instead:
/// ```python
/// a if a else b
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[violation]
pub struct IfExprWithTwistedArms {
    expr_body: String,
    expr_else: String,
}

impl AlwaysAutofixableViolation for IfExprWithTwistedArms {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!(
            "Use `{expr_else} if {expr_else} else {expr_body}` instead of `{expr_body} if not \
             {expr_else} else {expr_else}`"
        )
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!("Replace with `{expr_else} if {expr_else} else {expr_body}`")
    }
}

/// SIM210
pub(crate) fn if_expr_with_true_false(
    checker: &mut Checker,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    if !is_const_true(body) || !is_const_false(orelse) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfExprWithTrueFalse {
            is_compare: test.is_compare_expr(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if test.is_compare_expr() {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                checker
                    .locator()
                    .slice(
                        parenthesized_range(test.into(), expr.into(), checker.locator().contents())
                            .unwrap_or(test.range()),
                    )
                    .to_string(),
                expr.range(),
            )));
        } else if checker.semantic().is_builtin("bool") {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                checker.generator().expr(
                    &ast::ExprCall {
                        func: Box::new(
                            ast::ExprName {
                                id: "bool".into(),
                                ctx: ExprContext::Load,
                                range: TextRange::default(),
                            }
                            .into(),
                        ),
                        arguments: Arguments {
                            args: vec![test.clone()],
                            keywords: vec![],
                            range: TextRange::default(),
                        },
                        range: TextRange::default(),
                    }
                    .into(),
                ),
                expr.range(),
            )));
        };
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM211
pub(crate) fn if_expr_with_false_true(
    checker: &mut Checker,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    if !is_const_false(body) || !is_const_true(orelse) {
        return;
    }

    let mut diagnostic = Diagnostic::new(IfExprWithFalseTrue, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            checker.generator().expr(
                &ast::ExprUnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(test.clone()),
                    range: TextRange::default(),
                }
                .into(),
            ),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM212
pub(crate) fn twisted_arms_in_ifexpr(
    checker: &mut Checker,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let Expr::UnaryOp(ast::ExprUnaryOp {
        op,
        operand,
        range: _,
    }) = &test
    else {
        return;
    };
    if !op.is_not() {
        return;
    }

    // Check if the test operand and else branch use the same variable.
    let Expr::Name(ast::ExprName { id: test_id, .. }) = operand.as_ref() else {
        return;
    };
    let Expr::Name(ast::ExprName { id: orelse_id, .. }) = orelse else {
        return;
    };
    if !test_id.eq(orelse_id) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfExprWithTwistedArms {
            expr_body: checker.generator().expr(body),
            expr_else: checker.generator().expr(orelse),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let node = body.clone();
        let node1 = orelse.clone();
        let node2 = orelse.clone();
        let node3 = ast::ExprIfExp {
            test: Box::new(node2),
            body: Box::new(node1),
            orelse: Box::new(node),
            range: TextRange::default(),
        };
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            checker.generator().expr(&node3.into()),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
