use rustc_hash::FxHashSet;

use crate::Db;
use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast};
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::{place_table, semantic_index, use_def_map};

/// Return whether `expression` may refer to `definition`, directly or through an alias.
///
/// This follows regular use-def bindings first, then falls back to visible end-of-scope
/// declarations for names that can appear in deferred annotations.
pub(crate) fn expression_uses_definition<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    expression: &ast::Expr,
    definition: Definition<'db>,
) -> bool {
    let mut visitor = DefinitionUseVisitor::new(db, definition);
    visitor.visit_expression(scope, expression);
    visitor.found
}

/// Return whether the value expression for `source` may refer to `definition`.
///
/// This is used to detect dependency cycles through aliases without forcing the type
/// of `source`.
pub(crate) fn definition_value_uses_definition<'db>(
    db: &'db dyn Db,
    source: Definition<'db>,
    definition: Definition<'db>,
) -> bool {
    let mut visitor = DefinitionUseVisitor::new(db, definition);
    visitor.visit_definition_value(source);
    visitor.found
}

struct DefinitionUseVisitor<'db> {
    db: &'db dyn Db,
    definition: Definition<'db>,
    visited_definitions: FxHashSet<Definition<'db>>,
    found: bool,
}

impl<'db> DefinitionUseVisitor<'db> {
    fn new(db: &'db dyn Db, definition: Definition<'db>) -> Self {
        Self {
            db,
            definition,
            visited_definitions: FxHashSet::default(),
            found: false,
        }
    }

    fn visit_expression(&mut self, scope: ScopeId<'db>, expression: &ast::Expr) {
        let file = scope.file(self.db);
        DefinitionUseExpressionVisitor {
            visitor: self,
            file,
        }
        .visit_expr(expression);
    }

    fn visit_definition_value(&mut self, source: Definition<'db>) {
        if self.found || !self.visited_definitions.insert(source) {
            return;
        }

        let file = source.file(self.db);
        let module = parsed_module(self.db, file).load(self.db);

        match source.kind(self.db) {
            DefinitionKind::TypeAlias(type_alias) => {
                let value = type_alias.node(&module).value.as_ref();
                let value_scope = semantic_index(self.db, file)
                    .expression_scope_id(value)
                    .to_scope_id(self.db, file);
                self.visit_expression(value_scope, value);
            }
            DefinitionKind::Assignment(assignment) => {
                self.visit_expression(source.scope(self.db), assignment.value(&module));
            }
            DefinitionKind::AnnotatedAssignment(assignment) => {
                self.visit_expression(source.scope(self.db), assignment.annotation(&module));
                if let Some(value) = assignment.value(&module) {
                    self.visit_expression(source.scope(self.db), value);
                }
            }
            _ => {}
        }
    }

    fn visit_name(&mut self, scope: ScopeId<'db>, name: &ast::ExprName) {
        self.visit_reference(scope, name);
        self.visit_visible_name_definitions(scope, name.id.as_str());
    }

    fn visit_reference(&mut self, scope: ScopeId<'db>, expression: &ast::ExprName) {
        if self.found {
            return;
        }

        let use_def = use_def_map(self.db, scope);
        let use_id = ast::ExprRef::Name(expression).scoped_use_id(self.db, scope);

        for binding in use_def.bindings_at_use(use_id) {
            if binding
                .binding
                .is_defined_and(|definition| definition == self.definition)
            {
                self.found = true;
                return;
            }
            if let Some(definition) = binding.binding.definition() {
                self.visit_definition_value(definition);
                if self.found {
                    return;
                }
            }
        }
    }

    fn visit_visible_name_definitions(&mut self, scope: ScopeId<'db>, name: &str) {
        if self.found {
            return;
        }

        let file = scope.file(self.db);
        let index = semantic_index(self.db, file);

        for (file_scope_id, _) in index.visible_ancestor_scopes(scope.file_scope_id(self.db)) {
            let visible_scope = file_scope_id.to_scope_id(self.db, file);
            let place_table = place_table(self.db, visible_scope);
            let Some(symbol_id) = place_table.symbol_id(name) else {
                continue;
            };

            let symbol = place_table.symbol(symbol_id);
            if symbol.is_global() && !file_scope_id.is_global() {
                self.visit_name_declarations(
                    FileScopeId::global().to_scope_id(self.db, file),
                    name,
                );
                return;
            }

            if symbol.is_nonlocal() {
                continue;
            }

            if self.visit_name_declarations(visible_scope, name) {
                return;
            }
        }
    }

    fn visit_name_declarations(&mut self, scope: ScopeId<'db>, name: &str) -> bool {
        let place_table = place_table(self.db, scope);
        let Some(symbol_id) = place_table.symbol_id(name) else {
            return false;
        };

        let use_def = use_def_map(self.db, scope);
        let mut found_any_declaration = false;
        for declaration in use_def.end_of_scope_symbol_declarations(symbol_id) {
            let Some(definition) = declaration.declaration.definition() else {
                continue;
            };

            found_any_declaration = true;
            if definition == self.definition {
                self.found = true;
                return true;
            }

            self.visit_definition_value(definition);
            if self.found {
                return true;
            }
        }

        found_any_declaration
    }
}

struct DefinitionUseExpressionVisitor<'a, 'db> {
    visitor: &'a mut DefinitionUseVisitor<'db>,
    file: File,
}

impl<'db> DefinitionUseExpressionVisitor<'_, 'db> {
    fn expression_scope(&self, expression: &ast::Expr) -> ScopeId<'db> {
        semantic_index(self.visitor.db, self.file)
            .expression_scope_id(expression)
            .to_scope_id(self.visitor.db, self.file)
    }
}

impl<'ast> Visitor<'ast> for DefinitionUseExpressionVisitor<'_, '_> {
    fn visit_expr(&mut self, expression: &'ast ast::Expr) {
        if self.visitor.found {
            return;
        }

        let scope = self.expression_scope(expression);
        if let ast::Expr::Name(name) = expression {
            self.visitor.visit_name(scope, name);
        }

        visitor::walk_expr(self, expression);
    }
}
