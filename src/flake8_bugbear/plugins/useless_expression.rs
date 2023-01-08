use rustpython_ast::{Constant, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

// B018
pub fn useless_expression(xxxxxxxx: &mut xxxxxxxx, body: &[Stmt]) {
    for stmt in body {
        if let StmtKind::Expr { value } = &stmt.node {
            match &value.node {
                ExprKind::List { .. } | ExprKind::Dict { .. } | ExprKind::Set { .. } => {
                    xxxxxxxx.diagnostics.push(Diagnostic::new(
                        violations::UselessExpression,
                        Range::from_located(value),
                    ));
                }
                ExprKind::Constant { value: val, .. } => match &val {
                    Constant::Str { .. } | Constant::Ellipsis => {}
                    _ => {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::UselessExpression,
                            Range::from_located(value),
                        ));
                    }
                },
                _ => {}
            }
        }
    }
}
