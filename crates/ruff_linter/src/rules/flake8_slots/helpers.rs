use ruff_python_ast::{ExprKind, StmtKind, Suite};

/// Return `true` if the given body contains a `__slots__` assignment.
pub(super) fn has_slots(body: Suite<'_>) -> bool {
    for stmt in body {
        match stmt.classify() {
            StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();

                for target in targets {
                    if let ExprKind::Name(name) = target.classify() {
                        let id = name.id();
                        if id.as_str() == "__slots__" {
                            return true;
                        }
                    }
                }
            }
            StmtKind::AnnAssign(stmt_ann_assign) => {
                let target = stmt_ann_assign.target();

                if let ExprKind::Name(name) = target.classify() {
                    let id = name.id();
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
