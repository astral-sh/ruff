use std::string::FromUtf8Error;

use log::error;
use rustpython_ast::{Cmpop, Expr, ExprKind, StmtKind, Unaryop};

use crate::ast::helpers::create_expr;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::pycodestyle;
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

fn expr_with_style(expr: &Expr, checker: &mut Checker) -> Result<String, FromUtf8Error> {
    let mut generator = SourceCodeGenerator::new(
        checker.style.indentation(),
        checker.style.quote(),
        checker.style.line_ending(),
    );
    generator.unparse_expr(expr, 0);
    generator.generate()
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
        CheckKind::NegateEqualOp(
            expr_with_style(left, checker).unwrap(),
            expr_with_style(&comparators[0], checker).unwrap(),
        ),
        Range::from_located(operand),
    );
    if checker.patch(check.kind.code()) {
        match pycodestyle::plugins::compare(left, &[Cmpop::NotEq], comparators, checker.style) {
            Ok(content) => {
                check.amend(Fix::replacement(
                    content,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        };
    }
    checker.add_check(check);
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
        CheckKind::NegateNotEqualOp(
            expr_with_style(left, checker).unwrap(),
            expr_with_style(&comparators[0], checker).unwrap(),
        ),
        Range::from_located(operand),
    );
    if checker.patch(check.kind.code()) {
        match pycodestyle::plugins::compare(left, &[Cmpop::Eq], comparators, checker.style) {
            Ok(content) => {
                check.amend(Fix::replacement(
                    content,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        };
    }
    checker.add_check(check);
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

    let mut check = Check::new(
        CheckKind::DoubleNegation(operand.to_string()),
        Range::from_located(operand),
    );
    if checker.patch(check.kind.code()) {
        let inner_expr = create_expr(operand.node.clone());
        match expr_with_style(&inner_expr, checker) {
            Ok(content) => {
                check.amend(Fix::replacement(
                    content,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.add_check(check);
}
