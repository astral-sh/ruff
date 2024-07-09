use std::sync::Arc;

use rustc_hash::FxHashMap;

use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};

use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::ast_ids::AstIdsBuilder;
use crate::semantic_index::definition::{Definition, DefinitionNodeKey, DefinitionNodeRef};
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeRef, Scope, ScopeId, ScopedSymbolId, SymbolFlags,
    SymbolTableBuilder,
};
use crate::semantic_index::SemanticIndex;
use crate::Db;

pub(super) struct SemanticIndexBuilder<'db, 'ast> {
    // Builder state
    db: &'db dyn Db,
    file: File,
    module: &'db ParsedModule,
    scope_stack: Vec<FileScopeId>,
    /// the target we're currently inferring
    current_target: Option<CurrentTarget<'ast>>,

    // Semantic Index fields
    scopes: IndexVec<FileScopeId, Scope>,
    scope_ids_by_scope: IndexVec<FileScopeId, ScopeId<'db>>,
    symbol_tables: IndexVec<FileScopeId, SymbolTableBuilder<'db>>,
    ast_ids: IndexVec<FileScopeId, AstIdsBuilder>,
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,
    scopes_by_expression: FxHashMap<ExpressionNodeKey, FileScopeId>,
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definition<'db>>,
}

impl<'db, 'ast> SemanticIndexBuilder<'db, 'ast>
where
    'db: 'ast,
{
    pub(super) fn new(db: &'db dyn Db, file: File, parsed: &'db ParsedModule) -> Self {
        let mut builder = Self {
            db,
            file,
            module: parsed,
            scope_stack: Vec::new(),
            current_target: None,

            scopes: IndexVec::new(),
            symbol_tables: IndexVec::new(),
            ast_ids: IndexVec::new(),
            scope_ids_by_scope: IndexVec::new(),

            scopes_by_expression: FxHashMap::default(),
            scopes_by_node: FxHashMap::default(),
            definitions_by_node: FxHashMap::default(),
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

    fn push_scope(&mut self, node: NodeWithScopeRef<'ast>) {
        let parent = self.current_scope();
        self.push_scope_with_parent(node, Some(parent));
    }

    fn push_scope_with_parent(
        &mut self,
        node: NodeWithScopeRef<'ast>,
        parent: Option<FileScopeId>,
    ) {
        let children_start = self.scopes.next_index() + 1;

        let scope = Scope {
            parent,
            kind: node.scope_kind(),
            descendents: children_start..children_start,
        };

        let file_scope_id = self.scopes.push(scope);
        self.symbol_tables.push(SymbolTableBuilder::new());
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

    fn current_symbol_table(&mut self) -> &mut SymbolTableBuilder<'db> {
        let scope_id = self.current_scope();
        &mut self.symbol_tables[scope_id]
    }

    fn current_ast_ids(&mut self) -> &mut AstIdsBuilder {
        let scope_id = self.current_scope();
        &mut self.ast_ids[scope_id]
    }

    fn add_or_update_symbol(&mut self, name: Name, flags: SymbolFlags) -> ScopedSymbolId {
        let symbol_table = self.current_symbol_table();
        symbol_table.add_or_update_symbol(name, flags)
    }

    fn add_definition(
        &mut self,
        definition_node: impl Into<DefinitionNodeRef<'ast>>,
        symbol_id: ScopedSymbolId,
    ) -> Definition<'db> {
        let definition_node = definition_node.into();
        let definition = Definition::new(
            self.db,
            self.file,
            self.current_scope(),
            symbol_id,
            #[allow(unsafe_code)]
            unsafe {
                definition_node.into_owned(self.module.clone())
            },
        );

        self.definitions_by_node
            .insert(definition_node.key(), definition);

        definition
    }

    fn add_or_update_symbol_with_definition(
        &mut self,
        name: Name,
        definition: impl Into<DefinitionNodeRef<'ast>>,
    ) -> (ScopedSymbolId, Definition<'db>) {
        let symbol_table = self.current_symbol_table();

        let id = symbol_table.add_or_update_symbol(name, SymbolFlags::IS_DEFINED);
        let definition = self.add_definition(definition, id);
        self.current_symbol_table().add_definition(id, definition);
        (id, definition)
    }

    fn with_type_params(
        &mut self,
        with_params: &WithTypeParams<'ast>,
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

        assert!(self.current_target.is_none());

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
        self.scopes_by_expression.shrink_to_fit();
        self.definitions_by_node.shrink_to_fit();

        self.scope_ids_by_scope.shrink_to_fit();
        self.scopes_by_node.shrink_to_fit();

        SemanticIndex {
            symbol_tables,
            scopes: self.scopes,
            definitions_by_node: self.definitions_by_node,
            scope_ids_by_scope: self.scope_ids_by_scope,
            ast_ids,
            scopes_by_expression: self.scopes_by_expression,
            scopes_by_node: self.scopes_by_node,
        }
    }
}

impl<'db, 'ast> Visitor<'ast> for SemanticIndexBuilder<'db, 'ast>
where
    'db: 'ast,
{
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(function_def) => {
                for decorator in &function_def.decorator_list {
                    self.visit_decorator(decorator);
                }

                self.add_or_update_symbol_with_definition(
                    function_def.name.id.clone(),
                    function_def,
                );

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

                self.add_or_update_symbol_with_definition(class.name.id.clone(), class);

                self.with_type_params(&WithTypeParams::ClassDef { node: class }, |builder| {
                    if let Some(arguments) = &class.arguments {
                        builder.visit_arguments(arguments);
                    }

                    builder.push_scope(NodeWithScopeRef::Class(class));
                    builder.visit_body(&class.body);

                    builder.pop_scope()
                });
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.clone()
                    } else {
                        Name::new(alias.name.id.split('.').next().unwrap())
                    };

                    self.add_or_update_symbol_with_definition(symbol_name, alias);
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module: _,
                names,
                level: _,
                ..
            }) => {
                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        &asname.id
                    } else {
                        &alias.name.id
                    };

                    self.add_or_update_symbol_with_definition(symbol_name.clone(), alias);
                }
            }
            ast::Stmt::Assign(node) => {
                debug_assert!(self.current_target.is_none());
                self.visit_expr(&node.value);
                for target in &node.targets {
                    self.current_target = Some(CurrentTarget::Expr(target));
                    self.visit_expr(target);
                }
                self.current_target = None;
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
            ast::Expr::Name(ast::ExprName { id, ctx, .. }) => {
                let flags = match ctx {
                    ast::ExprContext::Load => SymbolFlags::IS_USED,
                    ast::ExprContext::Store => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Del => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Invalid => SymbolFlags::empty(),
                };
                match self.current_target {
                    Some(target) if flags.contains(SymbolFlags::IS_DEFINED) => {
                        self.add_or_update_symbol_with_definition(id.clone(), target);
                    }
                    _ => {
                        self.add_or_update_symbol(id.clone(), flags);
                    }
                }

                walk_expr(self, expr);
            }
            ast::Expr::Named(node) => {
                debug_assert!(self.current_target.is_none());
                self.current_target = Some(CurrentTarget::ExprNamed(node));
                // TODO walrus in comprehensions is implicitly nonlocal
                self.visit_expr(&node.target);
                self.current_target = None;
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
enum CurrentTarget<'a> {
    Expr(&'a ast::Expr),
    ExprNamed(&'a ast::ExprNamed),
}

impl<'a> From<CurrentTarget<'a>> for DefinitionNodeRef<'a> {
    fn from(val: CurrentTarget<'a>) -> Self {
        match val {
            CurrentTarget::Expr(expression) => DefinitionNodeRef::Target(expression),
            CurrentTarget::ExprNamed(named) => DefinitionNodeRef::NamedExpression(named),
        }
    }
}
