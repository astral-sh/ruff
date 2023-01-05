use rustpython_ast::{Cmpop, Expr, ExprKind, Location, StmtKind, Unaryop};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::pycodestyle::plugins::compare;
use crate::registry::{Check, CheckKind};
use crate::source_code_generator::SourceCodeGenerator;

fn is_exception_check(stmt: &StmtKind) -> bool {
    let StmtKind::If {test: _, body, orelse: _} = stmt else {
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
    let parent = &checker.current_stmt().0.node;
    if is_exception_check(parent) {
        return;
    }
    let mut check = Check::new(
        CheckKind::NegateEqualOp(left.to_string(), comparators[0].to_string()),
        Range::from_located(operand),
    );
    if checker.patch(check.kind.code()) {
        if let Some(content) = compare(left, &[Cmpop::NotEq], comparators, checker.style) {
            check.amend(Fix::replacement(
                content,
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    checker.add_check(check)
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
    let parent = &checker.current_stmt().0.node;
    if is_exception_check(parent) {
        return;
    }
    let mut check = Check::new(
        CheckKind::NegateNotEqualOp(left.to_string(), comparators[0].to_string()),
        Range::from_located(operand),
    );
    if checker.patch(check.kind.code()) {
        if let Some(content) = compare(left, &[Cmpop::Eq], comparators, checker.style) {
            check.amend(Fix::replacement(
                content,
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    checker.add_check(check)
}

/// SIM 208
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

    let mut check = Check::new(
        CheckKind::DoubleNegation(operand.to_string()),
        Range::from_located(operand),
    );
    if checker.patch(check.kind.code()) {
        let inner_expr = Expr::new(
            Location::default(),
            Location::default(),
            operand.node.clone(),
        );
        let mut generator = SourceCodeGenerator::new(
            checker.style.indentation(),
            checker.style.quote(),
            checker.style.line_ending(),
        );
        generator.unparse_expr(&inner_expr, 0);
        if let Ok(content) = generator.generate() {
            check.amend(Fix::replacement(
                content,
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    checker.add_check(check)
}
