use rustpython_parser::ast::{self, Expr, Stmt, StmtKind};

pub fn name(stmt: &Stmt) -> &str {
    match &stmt.node {
        StmtKind::FunctionDef(ast::StmtFunctionDef { name, .. })
        | StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef { name, .. }) => name.as_str(),
        _ => panic!("Expected StmtKind::FunctionDef | StmtKind::AsyncFunctionDef"),
    }
}

pub fn decorator_list(stmt: &Stmt) -> &Vec<Expr> {
    match &stmt.node {
        StmtKind::FunctionDef(ast::StmtFunctionDef { decorator_list, .. })
        | StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef { decorator_list, .. }) => {
            decorator_list
        }
        _ => panic!("Expected StmtKind::FunctionDef | StmtKind::AsyncFunctionDef"),
    }
}
