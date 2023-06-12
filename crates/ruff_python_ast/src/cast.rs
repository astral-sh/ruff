use rustpython_parser::ast::{self, Decorator, Stmt};

pub fn name(stmt: &Stmt) -> &str {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef { name, .. })
        | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { name, .. }) => name.as_str(),
        _ => panic!("Expected Stmt::FunctionDef | Stmt::AsyncFunctionDef"),
    }
}

pub fn decorator_list(stmt: &Stmt) -> &[Decorator] {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef { decorator_list, .. })
        | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { decorator_list, .. }) => {
            decorator_list
        }
        _ => panic!("Expected Stmt::FunctionDef | Stmt::AsyncFunctionDef"),
    }
}
