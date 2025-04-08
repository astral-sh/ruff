use ruff_python_ast::{self as ast, Arguments, CmpOp, Expr, ExprContext, Stmt, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::Name;
use ruff_python_semantic::ScopeKind;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for negated `==` operators.
///
/// ## Why is this bad?
/// Negated `==` operators are less readable than `!=` operators. When testing
/// for non-equality, it is more common to use `!=` than `==`.
///
/// ## Example
/// ```python
/// not a == b
/// ```
///
/// Use instead:
/// ```python
/// a != b
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe, as it might change the behaviour
/// if `a` and/or `b` overrides `__eq__`/`__ne__`
/// in such a manner that they don't return booleans.
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[derive(ViolationMetadata)]
pub(crate) struct NegateEqualOp {
    left: String,
    right: String,
}

impl AlwaysFixableViolation for NegateEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateEqualOp { left, right } = self;
        format!("Use `{left} != {right}` instead of `not {left} == {right}`")
    }

    fn fix_title(&self) -> String {
        "Replace with `!=` operator".to_string()
    }
}

/// ## What it does
/// Checks for negated `!=` operators.
///
/// ## Why is this bad?
/// Negated `!=` operators are less readable than `==` operators, as they avoid a
/// double negation.
///
/// ## Example
/// ```python
/// not a != b
/// ```
///
/// Use instead:
/// ```python
/// a == b
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe, as it might change the behaviour
/// if `a` and/or `b` overrides `__ne__`/`__eq__`
/// in such a manner that they don't return booleans.
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[derive(ViolationMetadata)]
pub(crate) struct NegateNotEqualOp {
    left: String,
    right: String,
}

impl AlwaysFixableViolation for NegateNotEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateNotEqualOp { left, right } = self;
        format!("Use `{left} == {right}` instead of `not {left} != {right}`")
    }

    fn fix_title(&self) -> String {
        "Replace with `==` operator".to_string()
    }
}

/// ## What it does
/// Checks for double negations (i.e., multiple `not` operators).
///
/// ## Why is this bad?
/// A double negation is redundant and less readable than omitting the `not`
/// operators entirely.
///
/// ## Example
/// ```python
/// not (not a)
/// ```
///
/// Use instead:
/// ```python
/// a
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[derive(ViolationMetadata)]
pub(crate) struct DoubleNegation {
    expr: String,
}

impl AlwaysFixableViolation for DoubleNegation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Use `{expr}` instead of `not (not {expr})`")
    }

    fn fix_title(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Replace with `{expr}`")
    }
}

fn is_dunder_method(name: &str) -> bool {
    matches!(
        name,
        "__eq__" | "__ne__" | "__lt__" | "__le__" | "__gt__" | "__ge__"
    )
}

fn is_exception_check(stmt: &Stmt) -> bool {
    let Stmt::If(ast::StmtIf { body, .. }) = stmt else {
        return false;
    };
    matches!(body.as_slice(), [Stmt::Raise(_)])
}

/// SIM201
pub(crate) fn negation_with_equal_op(checker: &Checker, expr: &Expr, op: UnaryOp, operand: &Expr) {
    if !matches!(op, UnaryOp::Not) {
        return;
    }
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
    }) = operand
    else {
        return;
    };
    if !matches!(&ops[..], [CmpOp::Eq]) {
        return;
    }
    if is_exception_check(checker.semantic().current_statement()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(ast::StmtFunctionDef { name, .. }) =
        &checker.semantic().current_scope().kind
    {
        if is_dunder_method(name) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(
        NegateEqualOp {
            left: checker.generator().expr(left),
            right: checker.generator().expr(&comparators[0]),
        },
        expr.range(),
    );
    let node = ast::ExprCompare {
        left: left.clone(),
        ops: Box::from([CmpOp::NotEq]),
        comparators: comparators.clone(),
        range: TextRange::default(),
    };
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(&node.into()),
        expr.range(),
    )));
    checker.report_diagnostic(diagnostic);
}

/// SIM202
pub(crate) fn negation_with_not_equal_op(
    checker: &Checker,
    expr: &Expr,
    op: UnaryOp,
    operand: &Expr,
) {
    if !matches!(op, UnaryOp::Not) {
        return;
    }
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
    }) = operand
    else {
        return;
    };
    if !matches!(&**ops, [CmpOp::NotEq]) {
        return;
    }
    if is_exception_check(checker.semantic().current_statement()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(ast::StmtFunctionDef { name, .. }) =
        &checker.semantic().current_scope().kind
    {
        if is_dunder_method(name) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(
        NegateNotEqualOp {
            left: checker.generator().expr(left),
            right: checker.generator().expr(&comparators[0]),
        },
        expr.range(),
    );
    let node = ast::ExprCompare {
        left: left.clone(),
        ops: Box::from([CmpOp::Eq]),
        comparators: comparators.clone(),
        range: TextRange::default(),
    };
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(&node.into()),
        expr.range(),
    )));
    checker.report_diagnostic(diagnostic);
}

/// SIM208
pub(crate) fn double_negation(checker: &Checker, expr: &Expr, op: UnaryOp, operand: &Expr) {
    if !matches!(op, UnaryOp::Not) {
        return;
    }
    let Expr::UnaryOp(ast::ExprUnaryOp {
        op: operand_op,
        operand,
        range: _,
    }) = operand
    else {
        return;
    };
    if !matches!(operand_op, UnaryOp::Not) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        DoubleNegation {
            expr: checker.generator().expr(operand),
        },
        expr.range(),
    );
    if checker.semantic().in_boolean_test() {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            checker.locator().slice(operand.as_ref()).to_string(),
            expr.range(),
        )));
    } else if checker.semantic().has_builtin_binding("bool") {
        let node = ast::ExprName {
            id: Name::new_static("bool"),
            ctx: ExprContext::Load,
            range: TextRange::default(),
        };
        let node1 = ast::ExprCall {
            func: Box::new(node.into()),
            arguments: Arguments {
                args: Box::from([*operand.clone()]),
                keywords: Box::from([]),
                range: TextRange::default(),
            },
            range: TextRange::default(),
        };
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            checker.generator().expr(&node1.into()),
            expr.range(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}
