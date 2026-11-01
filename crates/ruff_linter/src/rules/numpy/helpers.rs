use ruff_python_ast::helpers::any_over_body;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{Stmt, StmtKind, Suite, statement_visitor};
use ruff_python_semantic::SemanticModel;

/// Searches for a particular name imported from a module.
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
    fn visit_stmt(&mut self, stmt: Stmt<'_>) {
        if self.found_import {
            return;
        }
        if let StmtKind::ImportFrom(import_from) = stmt.classify() {
            let module = import_from.module();
            let names = import_from.names();
            if module.is_some_and(|module| module == self.module)
                && names.iter().any(|alias| {
                    let name = alias.name();
                    *name == *self.name
                })
            {
                self.found_import = true;
                return;
            }
        }
        statement_visitor::walk_stmt(self, stmt);
    }

    fn visit_body(&mut self, body: Suite<'_>) {
        for stmt in body {
            self.visit_stmt(stmt);
            if self.found_import {
                return;
            }
        }
    }
}

pub(crate) fn body_contains_attribute(
    body: Suite<'_>,
    module: &str,
    member: &str,
    semantic: &SemanticModel,
) -> bool {
    let target = QualifiedName::user_defined(module).append_member(member);
    any_over_body(body, |expr| {
        expr.is_attribute_expr()
            && semantic
                .resolve_qualified_name(expr)
                .is_some_and(|name| name == target)
    })
}
