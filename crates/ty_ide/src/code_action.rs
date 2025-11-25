use crate::{completion, find_node::covering_node};
use ruff_db::{files::File, parsed::parsed_module};
use ruff_diagnostics::Edit;
use ruff_text_size::TextRange;
use ty_project::Db;

#[derive(Debug, Clone)]
pub struct QuickFix {
    pub title: String,
    pub edit: Edit,
}

pub fn code_actions(
    db: &dyn Db,
    file: File,
    range: TextRange,
    diagnostic: &str,
) -> Option<Vec<QuickFix>> {
    // FIXME: look up the diagnostic properly / have the Diagnostic have a payload for us
    if diagnostic == "unresolved-reference" {
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
