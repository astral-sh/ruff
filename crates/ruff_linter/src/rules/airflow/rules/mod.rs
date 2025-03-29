use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{statement_visitor, Alias, Expr, ExprName, Stmt, StmtImportFrom, StmtTry};
use ruff_python_semantic::Exceptions;
use ruff_python_semantic::SemanticModel;

pub(crate) use dag_schedule_argument::*;
pub(crate) use moved_to_provider_in_3::*;
pub(crate) use removal_in_3::*;
pub(crate) use task_variable_name::*;

mod dag_schedule_argument;
mod moved_to_provider_in_3;
mod removal_in_3;
mod task_variable_name;

#[derive(Debug, Clone, Eq, PartialEq)]
enum Replacement {
    None,
    Name(&'static str),
    Message(&'static str),
    AutoImport {
        path: &'static str,
        name: &'static str,
    },
}

fn is_guarded_by_try_except(
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

/// AST visitor that searches an AST tree for [`ast::StmtImportFrom`] nodes
/// that match a certain [`QualifiedName`].
struct ImportSearcher<'a> {
    module: &'a str,
    name: &'a str,
    found_import: bool,
}

impl<'a> ImportSearcher<'a> {
    fn new(module: &'a str, name: &'a str) -> Self {
        Self {
            module,
            name,
            found_import: false,
        }
    }
}
impl StatementVisitor<'_> for ImportSearcher<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if self.found_import {
            return;
        }
        if let Stmt::ImportFrom(StmtImportFrom { module, names, .. }) = stmt {
            if module.as_ref().is_some_and(|module| module == self.module)
                && names.iter().any(|Alias { name, .. }| name == self.name)
            {
                self.found_import = true;
                return;
            }
        }
        statement_visitor::walk_stmt(self, stmt);
    }

    fn visit_body(&mut self, body: &[ruff_python_ast::Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
            if self.found_import {
                return;
            }
        }
    }
}
