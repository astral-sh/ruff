use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::visitor::{walk_expr, walk_stmt};
use ruff_python_ast::Expr;
use ruff_python_ast::{statement_visitor, Alias, Stmt, StmtImportFrom};
use ruff_python_semantic::SemanticModel;

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

/// AST visitor that searches an AST tree for [`ast::ExprAttribute`] nodes
/// that match a certain [`QualifiedName`].
pub(crate) struct AttributeSearcher<'a> {
    attribute_to_find: QualifiedName<'a>,
    semantic: &'a SemanticModel<'a>,
    pub found_attribute: bool,
}

impl<'a> AttributeSearcher<'a> {
    pub(crate) fn new(
        attribute_to_find: QualifiedName<'a>,
        semantic: &'a SemanticModel<'a>,
    ) -> Self {
        Self {
            attribute_to_find,
            semantic,
            found_attribute: false,
        }
    }
}

impl Visitor<'_> for AttributeSearcher<'_> {
    fn visit_expr(&mut self, expr: &'_ Expr) {
        if self.found_attribute {
            return;
        }
        if expr.is_attribute_expr()
            && self
                .semantic
                .resolve_qualified_name(expr)
                .is_some_and(|qualified_name| qualified_name == self.attribute_to_find)
        {
            self.found_attribute = true;
            return;
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &ruff_python_ast::Stmt) {
        if !self.found_attribute {
            walk_stmt(self, stmt);
        }
    }

    fn visit_body(&mut self, body: &[ruff_python_ast::Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
            if self.found_attribute {
                return;
            }
        }
    }
}
