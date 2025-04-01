use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{statement_visitor, Alias, Stmt, StmtImportFrom};

/// AST visitor that searches an AST tree for [`ast::StmtImportFrom`] nodes
/// that match a certain [`QualifiedName`].
pub(crate) struct ImportSearcher<'a> {
    module: &'a str,
    name: &'a str,
    pub found_import: bool,
}

impl<'a> ImportSearcher<'a> {
    pub(crate) fn new(module: &'a str, name: &'a str) -> Self {
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
