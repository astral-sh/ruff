use rustpython_ast::{Constant, ExprKind, Stmt, StmtKind};

pub fn is_empty(body: &[Stmt]) -> bool {
    match &body {
        [] => true,
        // Also allow: raise NotImplementedError, raise NotImplemented
        [stmt] => match &stmt.node {
            StmtKind::Pass => true,
            StmtKind::Expr { value } => match &value.node {
                ExprKind::Constant { value, .. } => {
                    matches!(value, Constant::Str(_) | Constant::Ellipsis)
                }
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}
