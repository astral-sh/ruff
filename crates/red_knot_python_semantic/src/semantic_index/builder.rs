use std::sync::Arc;

use rustc_hash::FxHashMap;

use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};

use crate::ast_node_ref::AstNodeRef;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::ast_ids::AstIdsBuilder;
use crate::semantic_index::definition::{
    AssignmentDefinitionNodeRef, Definition, DefinitionNodeKey, DefinitionNodeRef,
    ImportFromDefinitionNodeRef,
};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeRef, Scope, ScopeId, ScopedSymbolId, SymbolFlags,
    SymbolTableBuilder,
};
use crate::semantic_index::use_def::{FlowSnapshot, UseDefMapBuilder};
use crate::semantic_index::SemanticIndex;
use crate::Db;

pub(super) struct SemanticIndexBuilder<'db> {
    // Builder state
    db: &'db dyn Db,
    file: File,
    module: &'db ParsedModule,
    scope_stack: Vec<FileScopeId>,
    /// the assignment we're currently visiting
    current_assignment: Option<CurrentAssignment<'db>>,

    // Semantic Index fields
    scopes: IndexVec<FileScopeId, Scope>,
    scope_ids_by_scope: IndexVec<FileScopeId, ScopeId<'db>>,
    symbol_tables: IndexVec<FileScopeId, SymbolTableBuilder>,
    ast_ids: IndexVec<FileScopeId, AstIdsBuilder>,
    use_def_maps: IndexVec<FileScopeId, UseDefMapBuilder<'db>>,
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,
    scopes_by_expression: FxHashMap<ExpressionNodeKey, FileScopeId>,
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definition<'db>>,
    expressions_by_node: FxHashMap<ExpressionNodeKey, Expression<'db>>,
}

impl<'db> SemanticIndexBuilder<'db> {
    pub(super) fn new(db: &'db dyn Db, file: File, parsed: &'db ParsedModule) -> Self {
        let mut builder = Self {
            db,
            file,
            module: parsed,
            scope_stack: Vec::new(),
            current_assignment: None,

            scopes: IndexVec::new(),
            symbol_tables: IndexVec::new(),
            ast_ids: IndexVec::new(),
            scope_ids_by_scope: IndexVec::new(),
            use_def_maps: IndexVec::new(),

            scopes_by_expression: FxHashMap::default(),
            scopes_by_node: FxHashMap::default(),
            definitions_by_node: FxHashMap::default(),
            expressions_by_node: FxHashMap::default(),
        };

        builder.push_scope_with_parent(NodeWithScopeRef::Module, None);

        builder
    }

    fn current_scope(&self) -> FileScopeId {
        *self
            .scope_stack
            .last()
            .expect("Always to have a root scope")
    }

    fn push_scope(&mut self, node: NodeWithScopeRef) {
        let parent = self.current_scope();
        self.push_scope_with_parent(node, Some(parent));
    }

    fn push_scope_with_parent(&mut self, node: NodeWithScopeRef, parent: Option<FileScopeId>) {
        let children_start = self.scopes.next_index() + 1;

        let scope = Scope {
            parent,
            kind: node.scope_kind(),
            descendents: children_start..children_start,
        };

        let file_scope_id = self.scopes.push(scope);
        self.symbol_tables.push(SymbolTableBuilder::new());
        self.use_def_maps.push(UseDefMapBuilder::new());
        let ast_id_scope = self.ast_ids.push(AstIdsBuilder::new());

        #[allow(unsafe_code)]
        // SAFETY: `node` is guaranteed to be a child of `self.module`
        let scope_id = ScopeId::new(self.db, self.file, file_scope_id, unsafe {
            node.to_kind(self.module.clone())
        });

        self.scope_ids_by_scope.push(scope_id);
        self.scopes_by_node.insert(node.node_key(), file_scope_id);

        debug_assert_eq!(ast_id_scope, file_scope_id);

        self.scope_stack.push(file_scope_id);
    }

    fn pop_scope(&mut self) -> FileScopeId {
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

    fn current_use_def_map(&mut self) -> &mut UseDefMapBuilder<'db> {
        let scope_id = self.current_scope();
        &mut self.use_def_maps[scope_id]
    }

    fn current_ast_ids(&mut self) -> &mut AstIdsBuilder {
        let scope_id = self.current_scope();
        &mut self.ast_ids[scope_id]
    }

    fn flow_snapshot(&mut self) -> FlowSnapshot {
        self.current_use_def_map().snapshot()
    }

    fn flow_restore(&mut self, state: FlowSnapshot) {
        self.current_use_def_map().restore(state);
    }

    fn flow_merge(&mut self, state: FlowSnapshot) {
        self.current_use_def_map().merge(state);
    }

    fn add_or_update_symbol(&mut self, name: Name, flags: SymbolFlags) -> ScopedSymbolId {
        let symbol_table = self.current_symbol_table();
        let (symbol_id, added) = symbol_table.add_or_update_symbol(name, flags);
        if added {
            let use_def_map = self.current_use_def_map();
            use_def_map.add_symbol(symbol_id);
        }
        symbol_id
    }

    fn add_definition<'a>(
        &mut self,
        symbol: ScopedSymbolId,
        definition_node: impl Into<DefinitionNodeRef<'a>>,
    ) -> Definition<'db> {
        let definition_node = definition_node.into();
        let definition = Definition::new(
            self.db,
            self.file,
            self.current_scope(),
            symbol,
            #[allow(unsafe_code)]
            unsafe {
                definition_node.into_owned(self.module.clone())
            },
        );

        self.definitions_by_node
            .insert(definition_node.key(), definition);
        self.current_use_def_map()
            .record_definition(symbol, definition);

        definition
    }

    /// Record an expression that needs to be a Salsa ingredient, because we need to infer its type
    /// standalone (type narrowing tests, RHS of an assignment.)
    fn add_standalone_expression(&mut self, expression_node: &ast::Expr) {
        let expression = Expression::new(
            self.db,
            self.file,
            self.current_scope(),
            #[allow(unsafe_code)]
            unsafe {
                AstNodeRef::new(self.module.clone(), expression_node)
            },
        );
        self.expressions_by_node
            .insert(expression_node.into(), expression);
    }

    fn with_type_params(
        &mut self,
        with_params: &WithTypeParams,
        nested: impl FnOnce(&mut Self) -> FileScopeId,
    ) -> FileScopeId {
        let type_params = with_params.type_parameters();

        if let Some(type_params) = type_params {
            let with_scope = match with_params {
                WithTypeParams::ClassDef { node, .. } => {
                    NodeWithScopeRef::ClassTypeParameters(node)
                }
                WithTypeParams::FunctionDef { node, .. } => {
                    NodeWithScopeRef::FunctionTypeParameters(node)
                }
            };

            self.push_scope(with_scope);

            for type_param in &type_params.type_params {
                let name = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) => name,
                    ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { name, .. }) => name,
                    ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { name, .. }) => name,
                };
                self.add_or_update_symbol(name.id.clone(), SymbolFlags::IS_DEFINED);
            }
        }

        let nested_scope = nested(self);

        if type_params.is_some() {
            self.pop_scope();
        }

        nested_scope
    }

    pub(super) fn build(mut self) -> SemanticIndex<'db> {
        let module = self.module;
        self.visit_body(module.suite());

        // Pop the root scope
        self.pop_scope();
        assert!(self.scope_stack.is_empty());

        assert!(self.current_assignment.is_none());

        let mut symbol_tables: IndexVec<_, _> = self
            .symbol_tables
            .into_iter()
            .map(|builder| Arc::new(builder.finish()))
            .collect();

        let mut use_def_maps: IndexVec<_, _> = self
            .use_def_maps
            .into_iter()
            .map(|builder| Arc::new(builder.finish()))
            .collect();

        let mut ast_ids: IndexVec<_, _> = self
            .ast_ids
            .into_iter()
            .map(super::ast_ids::AstIdsBuilder::finish)
            .collect();

        self.scopes.shrink_to_fit();
        symbol_tables.shrink_to_fit();
        use_def_maps.shrink_to_fit();
        ast_ids.shrink_to_fit();
        self.scopes_by_expression.shrink_to_fit();
        self.definitions_by_node.shrink_to_fit();

        self.scope_ids_by_scope.shrink_to_fit();
        self.scopes_by_node.shrink_to_fit();

        SemanticIndex {
            symbol_tables,
            scopes: self.scopes,
            definitions_by_node: self.definitions_by_node,
            expressions_by_node: self.expressions_by_node,
            scope_ids_by_scope: self.scope_ids_by_scope,
            ast_ids,
            scopes_by_expression: self.scopes_by_expression,
            scopes_by_node: self.scopes_by_node,
            use_def_maps,
        }
    }
}

impl<'db, 'ast> Visitor<'ast> for SemanticIndexBuilder<'db>
where
    'ast: 'db,
{
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(function_def) => {
                for decorator in &function_def.decorator_list {
                    self.visit_decorator(decorator);
                }

                let symbol = self
                    .add_or_update_symbol(function_def.name.id.clone(), SymbolFlags::IS_DEFINED);
                self.add_definition(symbol, function_def);

                self.with_type_params(
                    &WithTypeParams::FunctionDef { node: function_def },
                    |builder| {
                        builder.visit_parameters(&function_def.parameters);
                        for expr in &function_def.returns {
                            builder.visit_annotation(expr);
                        }

                        builder.push_scope(NodeWithScopeRef::Function(function_def));
                        builder.visit_body(&function_def.body);
                        builder.pop_scope()
                    },
                );
            }
            ast::Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }

                let symbol =
                    self.add_or_update_symbol(class.name.id.clone(), SymbolFlags::IS_DEFINED);
                self.add_definition(symbol, class);

                self.with_type_params(&WithTypeParams::ClassDef { node: class }, |builder| {
                    if let Some(arguments) = &class.arguments {
                        builder.visit_arguments(arguments);
                    }

                    builder.push_scope(NodeWithScopeRef::Class(class));
                    builder.visit_body(&class.body);

                    builder.pop_scope()
                });
            }
            ast::Stmt::Import(node) => {
                for alias in &node.names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.clone()
                    } else {
                        Name::new(alias.name.id.split('.').next().unwrap())
                    };

                    let symbol = self.add_or_update_symbol(symbol_name, SymbolFlags::IS_DEFINED);
                    self.add_definition(symbol, alias);
                }
            }
            ast::Stmt::ImportFrom(node) => {
                for (alias_index, alias) in node.names.iter().enumerate() {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        &asname.id
                    } else {
                        &alias.name.id
                    };

                    let symbol =
                        self.add_or_update_symbol(symbol_name.clone(), SymbolFlags::IS_DEFINED);
                    self.add_definition(symbol, ImportFromDefinitionNodeRef { node, alias_index });
                }
            }
            ast::Stmt::Assign(node) => {
                debug_assert!(self.current_assignment.is_none());
                self.visit_expr(&node.value);
                self.add_standalone_expression(&node.value);
                self.current_assignment = Some(node.into());
                for target in &node.targets {
                    self.visit_expr(target);
                }
                self.current_assignment = None;
            }
            ast::Stmt::AnnAssign(node) => {
                debug_assert!(self.current_assignment.is_none());
                // TODO deferred annotation visiting
                self.visit_expr(&node.annotation);
                match &node.value {
                    Some(value) => {
                        self.visit_expr(value);
                        self.current_assignment = Some(node.into());
                        self.visit_expr(&node.target);
                        self.current_assignment = None;
                    }
                    None => {
                        // TODO annotation-only assignments
                        self.visit_expr(&node.target);
                    }
                }
            }
            ast::Stmt::If(node) => {
                self.visit_expr(&node.test);
                let pre_if = self.flow_snapshot();
                self.visit_body(&node.body);
                let mut last_clause_is_else = false;
                let mut post_clauses: Vec<FlowSnapshot> = vec![self.flow_snapshot()];
                for clause in &node.elif_else_clauses {
                    // we can only take an elif/else clause if none of the previous ones were taken
                    self.flow_restore(pre_if.clone());
                    self.visit_elif_else_clause(clause);
                    post_clauses.push(self.flow_snapshot());
                    if clause.test.is_none() {
                        last_clause_is_else = true;
                    }
                }
                let mut post_clause_iter = post_clauses.into_iter();
                if last_clause_is_else {
                    // if the last clause was an else, the pre_if state can't directly reach the
                    // post-state; we must enter one of the clauses.
                    self.flow_restore(post_clause_iter.next().unwrap());
                } else {
                    self.flow_restore(pre_if);
                }
                for post_clause_state in post_clause_iter {
                    self.flow_merge(post_clause_state);
                }
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        self.scopes_by_expression
            .insert(expr.into(), self.current_scope());
        self.current_ast_ids().record_expression(expr);

        match expr {
            ast::Expr::Name(name_node) => {
                let ast::ExprName { id, ctx, .. } = name_node;
                let flags = match ctx {
                    ast::ExprContext::Load => SymbolFlags::IS_USED,
                    ast::ExprContext::Store => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Del => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Invalid => SymbolFlags::empty(),
                };
                let symbol = self.add_or_update_symbol(id.clone(), flags);
                if flags.contains(SymbolFlags::IS_DEFINED) {
                    match self.current_assignment {
                        Some(CurrentAssignment::Assign(assignment)) => {
                            self.add_definition(
                                symbol,
                                AssignmentDefinitionNodeRef {
                                    assignment,
                                    target: name_node,
                                },
                            );
                        }
                        Some(CurrentAssignment::AnnAssign(ann_assign)) => {
                            self.add_definition(symbol, ann_assign);
                        }
                        Some(CurrentAssignment::Named(named)) => {
                            self.add_definition(symbol, named);
                        }
                        None => {}
                    }
                }

                if flags.contains(SymbolFlags::IS_USED) {
                    let use_id = self.current_ast_ids().record_use(expr);
                    self.current_use_def_map().record_use(symbol, use_id);
                }

                walk_expr(self, expr);
            }
            ast::Expr::Named(node) => {
                debug_assert!(self.current_assignment.is_none());
                self.current_assignment = Some(node.into());
                // TODO walrus in comprehensions is implicitly nonlocal
                self.visit_expr(&node.target);
                self.current_assignment = None;
                self.visit_expr(&node.value);
            }
            ast::Expr::If(ast::ExprIf {
                body, test, orelse, ..
            }) => {
                // TODO detect statically known truthy or falsy test (via type inference, not naive
                // AST inspection, so we can't simplify here, need to record test expression for
                // later checking)
                self.visit_expr(test);
                let pre_if = self.flow_snapshot();
                self.visit_expr(body);
                let post_body = self.flow_snapshot();
                self.flow_restore(pre_if);
                self.visit_expr(orelse);
                self.flow_merge(post_body);
            }
            _ => {
                walk_expr(self, expr);
            }
        }
    }
}

enum WithTypeParams<'node> {
    ClassDef { node: &'node ast::StmtClassDef },
    FunctionDef { node: &'node ast::StmtFunctionDef },
}

impl<'node> WithTypeParams<'node> {
    fn type_parameters(&self) -> Option<&'node ast::TypeParams> {
        match self {
            WithTypeParams::ClassDef { node, .. } => node.type_params.as_deref(),
            WithTypeParams::FunctionDef { node, .. } => node.type_params.as_deref(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum CurrentAssignment<'a> {
    Assign(&'a ast::StmtAssign),
    AnnAssign(&'a ast::StmtAnnAssign),
    Named(&'a ast::ExprNamed),
}

impl<'a> From<&'a ast::StmtAssign> for CurrentAssignment<'a> {
    fn from(value: &'a ast::StmtAssign) -> Self {
        Self::Assign(value)
    }
}

impl<'a> From<&'a ast::StmtAnnAssign> for CurrentAssignment<'a> {
    fn from(value: &'a ast::StmtAnnAssign) -> Self {
        Self::AnnAssign(value)
    }
}

impl<'a> From<&'a ast::ExprNamed> for CurrentAssignment<'a> {
    fn from(value: &'a ast::ExprNamed) -> Self {
        Self::Named(value)
    }
}
