use ruff_python_ast::Stmt;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextSize};

pub(super) fn outside_handlers(offset: TextSize, semantic: &SemanticModel) -> bool {
    for stmt in semantic.current_statements() {
        if matches!(stmt, Stmt::FunctionDef(_)) {
            break;
        }

        let Stmt::Try(try_stmt) = stmt else {
            continue;
        };
        let handlers = &try_stmt.handlers;

        if handlers
            .iter()
            .any(|handler| handler.range().contains(offset))
        {
            return false;
        }
    }

    true
}
