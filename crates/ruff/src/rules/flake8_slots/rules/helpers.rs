use ruff_python_ast::prelude::{Expr, Stmt};
use rustpython_parser::ast;

pub(crate) fn has_slots(body: &[Stmt]) -> bool {
    for stmt in body {
        if let Stmt::Assign(ast::StmtAssign { targets, .. }) = stmt {
            // x, __slots__ = "bla", ["foo"] is weird but acceptable
            for target in targets {
                if let Expr::Name(ast::ExprName { id, .. }) = target {
                    if id.as_str() == "__slots__" {
                        return true;
                    }
                }
            }
        }
    }
    false
}
