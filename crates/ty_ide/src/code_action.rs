use crate::{completion, find_node::covering_node};

use ruff_db::{files::File, parsed::parsed_module};
use ruff_diagnostics::Edit;
use ruff_text_size::TextRange;
use ty_project::Db;
use ty_python_semantic::create_suppression_fix;
use ty_python_semantic::types::UNRESOLVED_REFERENCE;

/// A `QuickFix` Code Action
#[derive(Debug, Clone)]
pub struct QuickFix {
    pub title: String,
    pub edits: Vec<Edit>,
    pub preferred: bool,
}

pub fn code_actions(
    db: &dyn Db,
    file: File,
    diagnostic_range: TextRange,
    diagnostic_id: &str,
) -> Vec<QuickFix> {
    let registry = db.lint_registry();
    let Ok(lint_id) = registry.get(diagnostic_id) else {
        return Vec::new();
    };

    let mut actions = Vec::new();

    if lint_id.name() == UNRESOLVED_REFERENCE.name()
        && let Some(import_quick_fix) = create_import_symbol_quick_fix(db, file, diagnostic_range)
    {
        actions.extend(import_quick_fix)
    }

    actions.push(QuickFix {
        title: format!("Ignore '{}' for this line", lint_id.name()),
        edits: create_suppression_fix(db, file, lint_id, diagnostic_range).into_edits(),
        preferred: false,
    });

    actions
}

fn create_import_symbol_quick_fix(
    db: &dyn Db,
    file: File,
    diagnostic_range: TextRange,
) -> Option<impl Iterator<Item = QuickFix>> {
    let parsed = parsed_module(db, file).load(db);
    let node = covering_node(parsed.syntax().into(), diagnostic_range).node();
    let symbol = &node.expr_name()?.id;

    Some(
        completion::missing_imports(db, file, &parsed, symbol, node)
            .into_iter()
            .map(|import| QuickFix {
                title: import.label,
                edits: vec![import.edit],
                preferred: true,
            }),
    )
}
