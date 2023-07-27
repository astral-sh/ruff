use crate::{nodes, Decorator, Stmt};

pub fn name(stmt: &Stmt) -> &str {
    match stmt {
        Stmt::FunctionDef(nodes::StmtFunctionDef { name, .. })
        | Stmt::AsyncFunctionDef(nodes::StmtAsyncFunctionDef { name, .. }) => name.as_str(),
        _ => panic!("Expected Stmt::FunctionDef | Stmt::AsyncFunctionDef"),
    }
}

pub fn decorator_list(stmt: &Stmt) -> &[Decorator] {
    match stmt {
        Stmt::FunctionDef(nodes::StmtFunctionDef { decorator_list, .. })
        | Stmt::AsyncFunctionDef(nodes::StmtAsyncFunctionDef { decorator_list, .. }) => {
            decorator_list
        }
        _ => panic!("Expected Stmt::FunctionDef | Stmt::AsyncFunctionDef"),
    }
}
