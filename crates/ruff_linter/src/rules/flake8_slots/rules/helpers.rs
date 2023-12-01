use ruff_python_ast::{self as ast, Expr, Stmt};

/// Return `true` if the given body contains a `__slots__` assignment.
pub(super) fn has_slots(body: &[Stmt]) -> bool {
    for stmt in body {
        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        if id.as_str() == "__slots__" {
                            return true;
                        }
                    }
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                    if id.as_str() == "__slots__" {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}
