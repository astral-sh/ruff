use crate::{nodes, Decorator, Stmt};

pub fn name(stmt: &Stmt) -> &str {
    let Stmt::FunctionDef(nodes::StmtFunctionDef { name, .. }) = stmt else {
        panic!("Expected Stmt::FunctionDef")
    };
    name.as_str()
}

pub fn decorator_list(stmt: &Stmt) -> &[Decorator] {
    let Stmt::FunctionDef(nodes::StmtFunctionDef { decorator_list, .. }) = stmt else {
        panic!("Expected Stmt::FunctionDef")
    };
    decorator_list
}
