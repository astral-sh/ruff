use std::sync::Arc;

use rustc_hash::FxHashMap;

use ruff_db::parsed::ParsedModule;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};

use crate::name::Name;
use crate::red_knot::node_key::NodeKey;
use crate::red_knot::semantic_index::ast_ids::{
    AstIdsBuilder, LocalAssignmentId, LocalClassId, LocalFunctionId, LocalImportFromId,
    LocalImportId, LocalNamedExprId,
};
use crate::red_knot::semantic_index::definition::{
    Definition, ImportDefinition, ImportFromDefinition,
};
use crate::red_knot::semantic_index::symbol::{
    LocalSymbolId, Scope, ScopeId, ScopeKind, SymbolFlags, SymbolId, SymbolTableBuilder,
};
use crate::red_knot::semantic_index::SemanticIndex;

pub(super) struct SemanticIndexBuilder<'a> {
    // Builder state
    module: &'a ParsedModule,
    scope_stack: Vec<ScopeId>,
    /// the definition whose target(s) we are currently walking
    current_definition: Option<Definition>,

    // Semantic Index fields
    scopes: IndexVec<ScopeId, Scope>,
    symbol_tables: IndexVec<ScopeId, SymbolTableBuilder>,
    ast_ids: IndexVec<ScopeId, AstIdsBuilder>,
    expression_scopes: FxHashMap<NodeKey, ScopeId>,
}

impl<'a> SemanticIndexBuilder<'a> {
    pub(super) fn new(parsed: &'a ParsedModule) -> Self {
        let mut builder = Self {
            module: parsed,
            scope_stack: Vec::new(),
            current_definition: None,

            scopes: IndexVec::new(),
            symbol_tables: IndexVec::new(),
            ast_ids: IndexVec::new(),
            expression_scopes: FxHashMap::default(),
        };

        builder.push_scope_with_parent(
            ScopeKind::Module,
            &Name::new_static("<module>"),
            None,
            None,
            None,
        );

        builder
    }

    fn current_scope(&self) -> ScopeId {
        *self
            .scope_stack
            .last()
            .expect("Always to have a root scope")
    }

    fn push_scope(
        &mut self,
        scope_kind: ScopeKind,
        name: &Name,
        defining_symbol: Option<SymbolId>,
        definition: Option<Definition>,
    ) {
        let parent = self.current_scope();
        self.push_scope_with_parent(scope_kind, name, defining_symbol, definition, Some(parent));
    }

    fn push_scope_with_parent(
        &mut self,
        scope_kind: ScopeKind,
        name: &Name,
        defining_symbol: Option<SymbolId>,
        definition: Option<Definition>,
        parent: Option<ScopeId>,
    ) {
        let children_start = self.scopes.next_index() + 1;

        let scope = Scope {
            name: name.clone(),
            parent,
            defining_symbol,
            definition,
            kind: scope_kind,
            descendents: children_start..children_start,
        };

        let scope_id = self.scopes.push(scope);
        self.symbol_tables.push(SymbolTableBuilder::new());
        self.ast_ids.push(AstIdsBuilder::new());
        self.scope_stack.push(scope_id);
    }

    fn pop_scope(&mut self) -> ScopeId {
        let id = self.scope_stack.pop().expect("Root scope to be present");
        let children_end = self.scopes.next_index();
        let scope = &mut self.scopes[id];
        scope.descendents = scope.descendents.start..children_end;
        id
    }

    fn current_symbol_table(&mut self) -> &mut SymbolTableBuilder {
        let scope_id = self.current_scope();
        &mut self.symbol_tables[scope_id]
    }

    fn current_ast_ids(&mut self) -> &mut AstIdsBuilder {
        let scope_id = self.current_scope();
        &mut self.ast_ids[scope_id]
    }

    fn add_or_update_symbol(&mut self, name: Name, flags: SymbolFlags) -> LocalSymbolId {
        let scope = self.current_scope();
        let symbol_table = self.current_symbol_table();

        symbol_table.add_or_update_symbol(name, scope, flags, None)
    }

    fn add_or_update_symbol_with_definition(
        &mut self,
        name: Name,

        definition: Definition,
    ) -> LocalSymbolId {
        let scope = self.current_scope();
        let symbol_table = self.current_symbol_table();

        symbol_table.add_or_update_symbol(name, scope, SymbolFlags::IS_DEFINED, Some(definition))
    }

    fn with_type_params(
        &mut self,
        name: &Name,
        params: &Option<Box<ast::TypeParams>>,
        definition: Option<Definition>,
        defining_symbol: SymbolId,
        nested: impl FnOnce(&mut Self) -> ScopeId,
    ) -> ScopeId {
        if let Some(type_params) = params {
            self.push_scope(
                ScopeKind::Annotation,
                name,
                Some(defining_symbol),
                definition,
            );
            for type_param in &type_params.type_params {
                let name = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) => name,
                    ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { name, .. }) => name,
                    ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { name, .. }) => name,
                };
                self.add_or_update_symbol(Name::new(name), SymbolFlags::IS_DEFINED);
            }
        }
        let nested_scope = nested(self);

        if params.is_some() {
            self.pop_scope();
        }

        nested_scope
    }

    pub(super) fn build(mut self) -> SemanticIndex {
        let module = self.module;
        self.visit_body(module.suite());

        // Pop the root scope
        self.pop_scope();
        assert!(self.scope_stack.is_empty());

        assert!(self.current_definition.is_none());

        let mut symbol_tables: IndexVec<_, _> = self
            .symbol_tables
            .into_iter()
            .map(|builder| Arc::new(builder.finish()))
            .collect();

        let mut ast_ids: IndexVec<_, _> = self
            .ast_ids
            .into_iter()
            .map(super::ast_ids::AstIdsBuilder::finish)
            .collect();

        self.scopes.shrink_to_fit();
        ast_ids.shrink_to_fit();
        symbol_tables.shrink_to_fit();
        self.expression_scopes.shrink_to_fit();

        SemanticIndex {
            symbol_tables,
            scopes: self.scopes,
            ast_ids,
            expression_scopes: self.expression_scopes,
        }
    }
}

impl Visitor<'_> for SemanticIndexBuilder<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        let module = self.module;
        #[allow(unsafe_code)]
        let statement_id = unsafe {
            // SAFETY: The builder only visits nodes that are part of `module`. This guarantees that
            // the current statement must be a child of `module`.
            self.current_ast_ids().record_statement(stmt, module)
        };
        match stmt {
            ast::Stmt::FunctionDef(function_def) => {
                for decorator in &function_def.decorator_list {
                    self.visit_decorator(decorator);
                }
                let name = Name::new(&function_def.name.id);
                let definition = Definition::FunctionDef(LocalFunctionId(statement_id));
                let scope = self.current_scope();
                let symbol = SymbolId::new(
                    scope,
                    self.add_or_update_symbol_with_definition(name.clone(), definition),
                );

                self.with_type_params(
                    &name,
                    &function_def.type_params,
                    Some(definition),
                    symbol,
                    |builder| {
                        builder.visit_parameters(&function_def.parameters);
                        for expr in &function_def.returns {
                            builder.visit_annotation(expr);
                        }

                        builder.push_scope(
                            ScopeKind::Function,
                            &name,
                            Some(symbol),
                            Some(definition),
                        );
                        builder.visit_body(&function_def.body);
                        builder.pop_scope()
                    },
                );
            }
            ast::Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }

                let name = Name::new(&class.name.id);
                let definition = Definition::from(LocalClassId(statement_id));
                let id = SymbolId::new(
                    self.current_scope(),
                    self.add_or_update_symbol_with_definition(name.clone(), definition),
                );
                self.with_type_params(&name, &class.type_params, Some(definition), id, |builder| {
                    if let Some(arguments) = &class.arguments {
                        builder.visit_arguments(arguments);
                    }

                    builder.push_scope(ScopeKind::Class, &name, Some(id), Some(definition));
                    builder.visit_body(&class.body);

                    builder.pop_scope()
                });
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for (i, alias) in names.iter().enumerate() {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.split('.').next().unwrap()
                    };

                    let def = Definition::Import(ImportDefinition {
                        import_id: LocalImportId(statement_id),
                        alias: u32::try_from(i).unwrap(),
                    });
                    self.add_or_update_symbol_with_definition(Name::new(symbol_name), def);
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module: _,
                names,
                level: _,
                ..
            }) => {
                for (i, alias) in names.iter().enumerate() {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.as_str()
                    };
                    let def = Definition::ImportFrom(ImportFromDefinition {
                        import_id: LocalImportFromId(statement_id),
                        name: u32::try_from(i).unwrap(),
                    });
                    self.add_or_update_symbol_with_definition(Name::new(symbol_name), def);
                }
            }
            ast::Stmt::Assign(node) => {
                debug_assert!(self.current_definition.is_none());
                self.visit_expr(&node.value);
                self.current_definition =
                    Some(Definition::Assignment(LocalAssignmentId(statement_id)));
                for target in &node.targets {
                    self.visit_expr(target);
                }
                self.current_definition = None;
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'_ ast::Expr) {
        let module = self.module;
        #[allow(unsafe_code)]
        let expression_id = unsafe {
            // SAFETY: The builder only visits nodes that are part of `module`. This guarantees that
            // the current expression must be a child of `module`.
            self.current_ast_ids().record_expression(expr, module)
        };

        self.expression_scopes
            .insert(NodeKey::from_node(expr), self.current_scope());

        match expr {
            ast::Expr::Name(ast::ExprName { id, ctx, .. }) => {
                let flags = match ctx {
                    ast::ExprContext::Load => SymbolFlags::IS_USED,
                    ast::ExprContext::Store => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Del => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Invalid => SymbolFlags::empty(),
                };
                match self.current_definition {
                    Some(definition) if flags.contains(SymbolFlags::IS_DEFINED) => {
                        self.add_or_update_symbol_with_definition(Name::new(id), definition);
                    }
                    _ => {
                        self.add_or_update_symbol(Name::new(id), flags);
                    }
                }

                walk_expr(self, expr);
            }
            ast::Expr::Named(node) => {
                debug_assert!(self.current_definition.is_none());
                self.current_definition =
                    Some(Definition::NamedExpr(LocalNamedExprId(expression_id)));
                // TODO walrus in comprehensions is implicitly nonlocal
                self.visit_expr(&node.target);
                self.current_definition = None;
                self.visit_expr(&node.value);
            }
            ast::Expr::If(ast::ExprIf {
                body, test, orelse, ..
            }) => {
                // TODO detect statically known truthy or falsy test (via type inference, not naive
                // AST inspection, so we can't simplify here, need to record test expression in CFG
                // for later checking)

                self.visit_expr(test);

                // let if_branch = self.flow_graph_builder.add_branch(self.current_flow_node());

                // self.set_current_flow_node(if_branch);
                // self.insert_constraint(test);
                self.visit_expr(body);

                // let post_body = self.current_flow_node();

                // self.set_current_flow_node(if_branch);
                self.visit_expr(orelse);

                // let post_else = self
                //     .flow_graph_builder
                //     .add_phi(self.current_flow_node(), post_body);

                // self.set_current_flow_node(post_else);
            }
            _ => {
                walk_expr(self, expr);
            }
        }
    }
}
