use rustpython_ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};

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

    checker.add_check(Check::new(
        CheckKind::NestedIfStatements,
        Range::from_located(stmt),
    ));
}
