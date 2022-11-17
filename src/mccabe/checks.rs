use rustpython_ast::{ExcepthandlerKind, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

fn get_complexity_number(stmts: &[Stmt]) -> usize {
    let mut complexity = 0;
    for stmt in stmts {
        match &stmt.node {
            StmtKind::If { body, orelse, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            StmtKind::For { body, orelse, .. } | StmtKind::AsyncFor { body, orelse, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            StmtKind::While { test, body, orelse } => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
                if let ExprKind::BoolOp { .. } = &test.node {
                    complexity += 1;
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
                complexity += get_complexity_number(finalbody);
                for handler in handlers {
                    complexity += 1;
                    let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
                    complexity += get_complexity_number(body);
                }
            }
            StmtKind::FunctionDef { body, .. } | StmtKind::AsyncFunctionDef { body, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
            }
            StmtKind::ClassDef { body, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
            }
            _ => {}
        }
    }
    complexity
}

pub fn function_is_too_complex(
    stmt: &Stmt,
    name: &str,
    body: &[Stmt],
    max_complexity: usize,
) -> Option<Check> {
    let complexity = get_complexity_number(body) + 1;
    if complexity > max_complexity {
        Some(Check::new(
            CheckKind::FunctionIsTooComplex(name.to_string(), complexity),
            Range::from_located(stmt),
        ))
    } else {
        None
    }
}
