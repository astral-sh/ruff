use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::helpers::{create_expr, create_stmt, unparse_expr, unparse_stmt};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticCode};
use crate::violations;

fn is_main_check(expr: &Expr) -> bool {
    if let ExprKind::Compare {
        left, comparators, ..
    } = &expr.node
    {
        if let ExprKind::Name { id, .. } = &left.node {
            if id == "__name__" {
                if comparators.len() == 1 {
                    if let ExprKind::Constant {
                        value: Constant::Str(value),
                        ..
                    } = &comparators[0].node
                    {
                        if value == "__main__" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// SIM102
pub fn nested_if_statements(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };

    // if a: <---
    //     if b: <---
    //         c
    let is_nested_if = {
        if orelse.is_empty() && body.len() == 1 {
            if let StmtKind::If { orelse, .. } = &body[0].node {
                orelse.is_empty()
            } else {
                false
            }
        } else {
            false
        }
    };

    if !is_nested_if {
        return;
    };

    if is_main_check(test) {
        return;
    }

    checker.checks.push(Diagnostic::new(
        violations::NestedIfStatements,
        Range::from_located(stmt),
    ));
}

fn is_one_line_return_bool(stmts: &[Stmt]) -> bool {
    if stmts.len() != 1 {
        return false;
    }
    let StmtKind::Return { value } = &stmts[0].node else {
        return false;
    };
    let Some(ExprKind::Constant { value, .. }) = value.as_ref().map(|value| &value.node) else {
        return false;
    };
    matches!(value, Constant::Bool(_))
}

/// SIM103
pub fn return_bool_condition_directly(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };
    if !(is_one_line_return_bool(body) && is_one_line_return_bool(orelse)) {
        return;
    }
    let condition = unparse_expr(test, checker.style);
    let mut check = Diagnostic::new(
        violations::ReturnBoolConditionDirectly(condition),
        Range::from_located(stmt),
    );
    if checker.patch(&DiagnosticCode::SIM103) {
        let return_stmt = create_stmt(StmtKind::Return {
            value: Some(test.clone()),
        });
        check.amend(Fix::replacement(
            unparse_stmt(&return_stmt, checker.style),
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.checks.push(check);
}

fn ternary(target_var: &Expr, body_value: &Expr, test: &Expr, orelse_value: &Expr) -> Stmt {
    create_stmt(StmtKind::Assign {
        targets: vec![target_var.clone()],
        value: Box::new(create_expr(ExprKind::IfExp {
            test: Box::new(test.clone()),
            body: Box::new(body_value.clone()),
            orelse: Box::new(orelse_value.clone()),
        })),
        type_comment: None,
    })
}

/// SIM108
pub fn use_ternary_operator(checker: &mut Checker, stmt: &Stmt, parent: Option<&Stmt>) {
    let StmtKind::If { test, body, orelse } = &stmt.node else {
        return;
    };
    if body.len() != 1 || orelse.len() != 1 {
        return;
    }
    let StmtKind::Assign { targets: body_targets, value: body_value, .. } = &body[0].node else {
        return;
    };
    let StmtKind::Assign { targets: orelse_targets, value: orelse_value, .. } = &orelse[0].node else {
        return;
    };
    if body_targets.len() != 1 || orelse_targets.len() != 1 {
        return;
    }
    let ExprKind::Name { id: body_id, .. } = &body_targets[0].node else {
        return;
    };
    let ExprKind::Name { id: orelse_id, .. } = &orelse_targets[0].node else {
        return;
    };
    if body_id != orelse_id {
        return;
    }

    let target_var = &body_targets[0];

    // It's part of a bigger if-elif block:
    // https://github.com/MartinThoma/flake8-simplify/issues/115
    if let Some(StmtKind::If {
        orelse: parent_orelse,
        ..
    }) = parent.map(|parent| &parent.node)
    {
        if parent_orelse.len() == 1 && stmt == &parent_orelse[0] {
            // TODO(charlie): These two cases have the same AST:
            //
            // if True:
            //     pass
            // elif a:
            //     b = 1
            // else:
            //     b = 2
            //
            // if True:
            //     pass
            // else:
            //     if a:
            //         b = 1
            //     else:
            //         b = 2
            //
            // We want to flag the latter, but not the former. Right now, we flag neither.
            return;
        }
    }

    let ternary = ternary(target_var, body_value, test, orelse_value);
    let content = unparse_stmt(&ternary, checker.style);
    let mut check = Diagnostic::new(
        violations::UseTernaryOperator(content.clone()),
        Range::from_located(stmt),
    );
    if checker.patch(&DiagnosticCode::SIM108) {
        check.amend(Fix::replacement(
            content,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.checks.push(check);
}
