use rustpython_ast::{Expr, Stmt, StmtKind};

pub fn decorator_list(stmt: &Stmt) -> &Vec<Expr> {
    match &stmt.node {
        StmtKind::FunctionDef { decorator_list, .. }
        | StmtKind::AsyncFunctionDef { decorator_list, .. } => decorator_list,
        _ => panic!("Expected StmtKind::FunctionDef | StmtKind::AsyncFunctionDef"),
    }
}
