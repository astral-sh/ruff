use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Cmpop, Expr, ExprKind, Stmt, StmtKind, Unaryop};

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::{Range, ScopeKind};
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct NegateEqualOp {
        pub left: String,
        pub right: String,
    }
);
impl AlwaysAutofixableViolation for NegateEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateEqualOp { left, right } = self;
        format!("Use `{left} != {right}` instead of `not {left} == {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `!=` operator".to_string()
    }
}

define_violation!(
    pub struct NegateNotEqualOp {
        pub left: String,
        pub right: String,
    }
);
impl AlwaysAutofixableViolation for NegateNotEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateNotEqualOp { left, right } = self;
        format!("Use `{left} == {right}` instead of `not {left} != {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `==` operator".to_string()
    }
}

define_violation!(
    pub struct DoubleNegation {
        pub expr: String,
    }
);
impl AlwaysAutofixableViolation for DoubleNegation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Use `{expr}` instead of `not (not {expr})`")
    }

    fn autofix_title(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Replace with `{expr}`")
    }
}

const DUNDER_METHODS: &[&str] = &["__eq__", "__ne__", "__lt__", "__le__", "__gt__", "__ge__"];

fn is_exception_check(stmt: &Stmt) -> bool {
    let StmtKind::If {test: _, body, orelse: _} = &stmt.node else {
        return false;
    };
    if body.len() != 1 {
        return false;
    }
    if matches!(body[0].node, StmtKind::Raise { .. }) {
        return true;
    }
    false
}

/// SIM201
pub fn negation_with_equal_op(checker: &mut Checker, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let ExprKind::Compare{ left, ops, comparators} = &operand.node else {
        return;
    };
    if !matches!(&ops[..], [Cmpop::Eq]) {
        return;
    }
    if is_exception_check(checker.current_stmt()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(def) = &checker.current_scope().kind {
        if DUNDER_METHODS.contains(&def.name) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(
        NegateEqualOp {
            left: unparse_expr(left, checker.stylist),
            right: unparse_expr(&comparators[0], checker.stylist),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            unparse_expr(
                &create_expr(ExprKind::Compare {
                    left: left.clone(),
                    ops: vec![Cmpop::NotEq],
                    comparators: comparators.clone(),
                }),
                checker.stylist,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM202
pub fn negation_with_not_equal_op(
    checker: &mut Checker,
    expr: &Expr,
    op: &Unaryop,
    operand: &Expr,
) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let ExprKind::Compare{ left, ops, comparators} = &operand.node else {
        return;
    };
    if !matches!(&ops[..], [Cmpop::NotEq]) {
        return;
    }
    if is_exception_check(checker.current_stmt()) {
        return;
    }

    // Avoid flagging issues in dunder implementations.
    if let ScopeKind::Function(def) = &checker.current_scope().kind {
        if DUNDER_METHODS.contains(&def.name) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(
        NegateNotEqualOp {
            left: unparse_expr(left, checker.stylist),
            right: unparse_expr(&comparators[0], checker.stylist),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            unparse_expr(
                &create_expr(ExprKind::Compare {
                    left: left.clone(),
                    ops: vec![Cmpop::Eq],
                    comparators: comparators.clone(),
                }),
                checker.stylist,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM208
pub fn double_negation(checker: &mut Checker, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let ExprKind::UnaryOp { op: operand_op, operand } = &operand.node else {
        return;
    };
    if !matches!(operand_op, Unaryop::Not) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        DoubleNegation {
            expr: unparse_expr(operand, checker.stylist),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            unparse_expr(operand, checker.stylist),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
