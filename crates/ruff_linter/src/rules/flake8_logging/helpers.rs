use ruff_python_ast::Stmt;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextSize};

pub(super) fn outside_handlers(offset: TextSize, semantic: &SemanticModel) -> bool {
    for stmt in semantic.current_statements() {
        let Stmt::Try(try_stmt) = stmt else {
            continue;
        };

        if try_stmt
            .handlers
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
