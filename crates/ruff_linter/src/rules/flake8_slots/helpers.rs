use ruff_python_ast::flat;

/// Return `true` if the given body contains a `__slots__` assignment.
pub(super) fn has_slots(body: flat::Suite<'_>) -> bool {
    for stmt in body {
        match stmt.classify() {
            flat::StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();

                for target in targets {
                    if let flat::ExprKind::Name(name) = target.classify() {
                        let id = name.id();
                        if id.as_str() == "__slots__" {
                            return true;
                        }
                    }
                }
            }
            flat::StmtKind::AnnAssign(stmt_ann_assign) => {
                let target = stmt_ann_assign.target();

                if let flat::ExprKind::Name(name) = target.classify() {
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
