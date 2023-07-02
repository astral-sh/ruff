use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprContext, Ranged, UnaryOp};

use ruff_diagnostics::{AlwaysAutofixableViolation, AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

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
    expr: String,
}

impl Violation for IfExprWithTrueFalse {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTrueFalse { expr } = self;
        format!("Use `bool({expr})` instead of `True if {expr} else False`")
    }

    fn autofix_title(&self) -> Option<String> {
        let IfExprWithTrueFalse { expr } = self;
        Some(format!("Replace with `not {expr}"))
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
pub struct IfExprWithFalseTrue {
    expr: String,
}

impl AlwaysAutofixableViolation for IfExprWithFalseTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithFalseTrue { expr } = self;
        format!("Use `not {expr}` instead of `False if {expr} else True`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithFalseTrue { expr } = self;
        format!("Replace with `bool({expr})")
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
pub(crate) fn explicit_true_false_in_ifexpr(
    checker: &mut Checker,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let Expr::Constant(ast::ExprConstant { value, .. }) = &body else {
        return;
    };
    if !matches!(value, Constant::Bool(true)) {
        return;
    }
    let Expr::Constant(ast::ExprConstant { value, .. }) = &orelse else {
        return;
    };
    if !matches!(value, Constant::Bool(false)) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfExprWithTrueFalse {
            expr: checker.generator().expr(test),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if matches!(test, Expr::Compare(_)) {
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                checker.generator().expr(&test.clone()),
                expr.range(),
            )));
        } else if checker.semantic().is_builtin("bool") {
            let node = ast::ExprName {
                id: "bool".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            let node1 = ast::ExprCall {
                func: Box::new(node.into()),
                args: vec![test.clone()],
                keywords: vec![],
                range: TextRange::default(),
            };
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                checker.generator().expr(&node1.into()),
                expr.range(),
            )));
        };
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM211
pub(crate) fn explicit_false_true_in_ifexpr(
    checker: &mut Checker,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let Expr::Constant(ast::ExprConstant { value, .. }) = &body else {
        return;
    };
    if !matches!(value, Constant::Bool(false)) {
        return;
    }
    let Expr::Constant(ast::ExprConstant { value, .. }) = &orelse else {
        return;
    };
    if !matches!(value, Constant::Bool(true)) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        IfExprWithFalseTrue {
            expr: checker.generator().expr(test),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let node = test.clone();
        let node1 = ast::ExprUnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(node),
            range: TextRange::default(),
        };
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            checker.generator().expr(&node1.into()),
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
        operand: test_operand,
        range: _,
    }) = &test
    else {
        return;
    };
    if !op.is_not() {
        return;
    }

    // Check if the test operand and else branch use the same variable.
    let Expr::Name(ast::ExprName { id: test_id, .. }) = test_operand.as_ref() else {
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
