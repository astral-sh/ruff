use ruff_python_ast::StmtKind;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextSize};

pub(super) fn outside_handlers(offset: TextSize, semantic: &SemanticModel) -> bool {
    for stmt in semantic.current_statements() {
        let StmtKind::Try(try_stmt) = stmt.classify() else {
            continue;
        };
        let handlers = try_stmt.handlers();

        if handlers
            .iter()
            .any(|handler| handler.range().contains(offset))
        {
            return false;
        }
    }

    true
}

#[inline]
pub(crate) fn is_logger_method_name(attr: &str) -> bool {
    matches!(
        attr,
        "debug" | "info" | "warn" | "warning" | "error" | "critical" | "log" | "exception"
    )
}
