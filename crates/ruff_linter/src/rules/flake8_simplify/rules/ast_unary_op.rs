use ruff_python_ast::{
    Builder, CmpOp, Expr, ExprKind, Stmt, StmtFunctionDefFields, StmtKind, UnaryOp,
};
use ruff_text_size::Ranged;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::ScopeKind;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[violation_metadata(stable_since = "v0.0.213", category = Category::Complexity)]
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
#[violation_metadata(stable_since = "v0.0.213", category = Category::Complexity)]
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
#[violation_metadata(stable_since = "v0.0.213", category = Category::Complexity)]
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

fn is_exception_check(stmt: Stmt<'_>) -> bool {
    let StmtKind::If(if_stmt) = stmt.classify() else {
        return false;
    };
    let body = if_stmt.body();
    matches!(
        body.as_array().map(|items| items.map(Stmt::classify)),
        Some([StmtKind::Raise(_)])
    )
}

/// SIM201
pub(crate) fn negation_with_equal_op(
    checker: &Checker,
    &expr: &Expr<'_>,
    op: UnaryOp,
    operand: Expr<'_>,
) {
    if !matches!(op, UnaryOp::Not) {
        return;
    }
    let ExprKind::Compare(compare) = operand.classify() else {
        return;
    };
    if !matches!(&compare.ops().as_array(), Some([CmpOp::Eq])) {
        return;
    }
    if is_exception_check(checker.semantic().current_statement()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(stmt_function_def) = &checker.semantic().current_scope().kind
        && let StmtFunctionDefFields { name, .. } = stmt_function_def.fields()
    {
        if is_dunder_method(name.as_str()) {
            return;
        }
    }

    let mut diagnostic = checker.report_diagnostic(
        NegateEqualOp {
            left: checker.generator().expr(compare.first_operand()),
            right: checker.generator().expr(compare.second_operand()),
        },
        expr.range(),
    );
    let mut builder = Builder::default();
    let replacement = syntax::with_comparison_op(&mut builder, compare, CmpOp::NotEq);
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(builder.get(replacement)),
        expr.range(),
    )));
}

/// SIM202
pub(crate) fn negation_with_not_equal_op(
    checker: &Checker,
    &expr: &Expr<'_>,
    op: UnaryOp,
    operand: Expr<'_>,
) {
    if !matches!(op, UnaryOp::Not) {
        return;
    }
    let ExprKind::Compare(compare) = operand.classify() else {
        return;
    };
    if !matches!(&compare.ops().as_array(), Some([CmpOp::NotEq])) {
        return;
    }
    if is_exception_check(checker.semantic().current_statement()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(stmt_function_def) = &checker.semantic().current_scope().kind
        && let StmtFunctionDefFields { name, .. } = stmt_function_def.fields()
    {
        if is_dunder_method(name.as_str()) {
            return;
        }
    }

    let mut diagnostic = checker.report_diagnostic(
        NegateNotEqualOp {
            left: checker.generator().expr(compare.first_operand()),
            right: checker.generator().expr(compare.second_operand()),
        },
        expr.range(),
    );
    let mut builder = Builder::default();
    let replacement = syntax::with_comparison_op(&mut builder, compare, CmpOp::Eq);
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(builder.get(replacement)),
        expr.range(),
    )));
}

/// SIM208
pub(crate) fn double_negation(checker: &Checker, &expr: &Expr<'_>, op: UnaryOp, operand: Expr<'_>) {
    if !matches!(op, UnaryOp::Not) {
        return;
    }
    let ExprKind::UnaryOp(unary_op) = operand.classify() else {
        return;
    };
    let operand_op = unary_op.op();
    let operand = unary_op.operand();
    if !matches!(operand_op, UnaryOp::Not) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        DoubleNegation {
            expr: checker.generator().expr(operand),
        },
        expr.range(),
    );
    if checker.semantic().in_boolean_test() {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            checker.locator().slice(operand).to_string(),
            expr.range(),
        )));
    } else if checker.semantic().has_builtin_binding("bool") {
        let mut builder = Builder::default();
        let args = builder.copy_nodes([operand]);
        let replacement = syntax::call_name(&mut builder, "bool", args);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            checker.generator().expr(builder.get(replacement)),
            expr.range(),
        )));
    }
}
