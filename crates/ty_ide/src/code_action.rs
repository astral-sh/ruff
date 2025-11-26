use crate::{completion, find_node::covering_node};
use ruff_db::{files::File, parsed::parsed_module};
use ruff_diagnostics::Edit;
use ruff_text_size::TextRange;
use ty_project::Db;
use ty_python_semantic::types::UNRESOLVED_REFERENCE;

/// A `QuickFix` Code Action
#[derive(Debug, Clone)]
pub struct QuickFix {
    pub title: String,
    pub edit: Edit,
}

pub fn code_actions(
    db: &dyn Db,
    file: File,
    range: TextRange,
    diagnostic_id: &str,
) -> Option<Vec<QuickFix>> {
    let registry = db.lint_registry();
    let Ok(lint_id) = registry.get(diagnostic_id) else {
        return None;
    };
    if lint_id.name() == UNRESOLVED_REFERENCE.name() {
        let parsed = parsed_module(db, file).load(db);
        let node = covering_node(parsed.syntax().into(), range).node();
        let symbol = &node.expr_name()?.id;

        let fixes = completion::missing_imports(db, file, &parsed, symbol, node)
            .into_iter()
            .map(|import| QuickFix {
                title: import.label,
                edit: import.edit,
            })
            .collect();
        Some(fixes)
    } else {
        None
    }
}
