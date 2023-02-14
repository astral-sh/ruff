use rustpython_parser::ast::{Expr, Stmt, StmtKind};

pub fn name(stmt: &Stmt) -> &str {
    match &stmt.node {
        StmtKind::FunctionDef { name, .. } | StmtKind::AsyncFunctionDef { name, .. } => name,
        _ => panic!("Expected StmtKind::FunctionDef | StmtKind::AsyncFunctionDef"),
    }
}

pub fn decorator_list(stmt: &Stmt) -> &Vec<Expr> {
    match &stmt.node {
        StmtKind::FunctionDef { decorator_list, .. }
        | StmtKind::AsyncFunctionDef { decorator_list, .. } => decorator_list,
        _ => panic!("Expected StmtKind::FunctionDef | StmtKind::AsyncFunctionDef"),
    }
}
