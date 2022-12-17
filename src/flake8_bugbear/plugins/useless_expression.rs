use rustpython_ast::{Constant, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

// B018
pub fn useless_expression(checker: &mut Checker, body: &[Stmt]) {
    for stmt in body {
        if let StmtKind::Expr { value } = &stmt.node {
            match &value.node {
                ExprKind::List { .. } | ExprKind::Dict { .. } | ExprKind::Set { .. } => {
                    checker.add_check(Check::new(
                        CheckKind::UselessExpression,
                        Range::from_located(value),
                    ));
                }
                ExprKind::Constant { value: val, .. } => match &val {
                    Constant::Str { .. } | Constant::Ellipsis => {}
                    _ => {
                        checker.add_check(Check::new(
                            CheckKind::UselessExpression,
                            Range::from_located(value),
                        ));
                    }
                },
                _ => {}
            }
        }
    }
}
