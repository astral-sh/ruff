use crate::rules::numpy::helpers::ImportSearcher;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{Expr, ExprName, StmtTry};
use ruff_python_semantic::Exceptions;
use ruff_python_semantic::SemanticModel;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum Replacement {
    None,
    Name(&'static str),
    Message(&'static str),
    AutoImport {
        path: &'static str,
        name: &'static str,
    },
}

pub(crate) fn is_guarded_by_try_except(
    expr: &Expr,
    replacement: &Replacement,
    semantic: &SemanticModel,
) -> bool {
    match expr {
        Expr::Name(ExprName { id, .. }) => {
            let Some(binding_id) = semantic.lookup_symbol(id.as_str()) else {
                return false;
            };
            let binding = semantic.binding(binding_id);
            if !binding.is_external() {
                return false;
            }
            if !binding.in_exception_handler() {
                return false;
            }
            let Some(try_node) = binding.source.and_then(|import_id| {
                semantic
                    .statements(import_id)
                    .find_map(|stmt| stmt.as_try_stmt())
            }) else {
                return false;
            };
            let suspended_exceptions = Exceptions::from_try_stmt(try_node, semantic);
            if !suspended_exceptions
                .intersects(Exceptions::IMPORT_ERROR | Exceptions::MODULE_NOT_FOUND_ERROR)
            {
                return false;
            }
            try_block_contains_undeprecated_import(try_node, replacement)
        }
        _ => false,
    }
}

/// Given an [`ast::StmtTry`] node, does the `try` branch of that node
/// contain any [`ast::StmtImportFrom`] nodes that indicate the numpy
/// member is being imported from the non-deprecated location?
fn try_block_contains_undeprecated_import(try_node: &StmtTry, replacement: &Replacement) -> bool {
    let Replacement::AutoImport { path, name } = replacement else {
        return false;
    };
    let mut import_searcher = ImportSearcher::new(path, name);
    import_searcher.visit_body(&try_node.body);
    import_searcher.found_import
}
