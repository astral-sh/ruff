use std::num::NonZeroU32;
use std::sync::Arc;

use countme::Count;
use tracing::{debug, warn};

use ruff_python_ast as ast;
use ruff_python_ast::visitor::preorder;
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Stmt};

use crate::db::Upcast;
use crate::module::ModuleName;
use crate::salsa_db::semantic::ast_ids::AstIdNode;
use crate::salsa_db::semantic::definition::{Definition, ImportDefinition, ImportFromDefinition};
use crate::salsa_db::semantic::flow_graph::{FlowGraph, FlowGraphBuilder, FlowNodeId};
use crate::salsa_db::semantic::module::{
    file_to_module, Module, ModuleSearchPaths, ResolvedModule,
};
use crate::salsa_db::semantic::symbol_table::{
    symbol_table as symbol_table_query, Dependency, NodeWithScopeId, ScopeId, ScopeKind,
    SymbolFlags, SymbolId, SymbolTable, SymbolTableBuilder,
};
use crate::salsa_db::semantic::types::infer::{
    infer_class_body, infer_function_body, infer_module_body, infer_types,
};
use crate::salsa_db::semantic::types::{
    typing_scopes, ClassTypingScope, FunctionTypingScope, Type, TypingScope,
};
use crate::salsa_db::source;
use crate::salsa_db::source::{parse, File};

pub use self::module::resolve_module;

pub mod ast_ids;
mod definition;
pub mod flow_graph;
pub mod module;
pub mod symbol_table;
pub mod types;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct GlobalId<I>
where
    I: Copy,
{
    file: File,
    local: I,
}

impl<I> GlobalId<I>
where
    I: Copy,
{
    pub fn new(file: File, local: I) -> Self {
        GlobalId { file, local }
    }

    pub fn file(&self) -> File {
        self.file
    }

    pub fn local(&self) -> I {
        self.local
    }
}

pub type GlobalSymbolId = GlobalId<SymbolId>;

#[derive(Debug, Eq, PartialEq)]
pub struct SemanticIndex {
    pub symbol_table: Arc<SymbolTable>,

    pub flow_graph: Arc<FlowGraph>,

    count: Count<SemanticIndex>,
}

#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_global_symbol(db: &dyn Db, file: File, name: &str) -> Option<GlobalSymbolId> {
    let symbol_table = symbol_table_query(db, file);
    let symbol_id = symbol_table.root_symbol_id_by_name(name)?;

    Some(GlobalSymbolId::new(file, symbol_id))
}

#[tracing::instrument(level = "debug", skip(db))]
pub fn global_symbol_type(db: &dyn Db, symbol: GlobalSymbolId) -> Type {
    let typing_scope = TypingScope::for_symbol(db, symbol);
    let types = infer_types(db, typing_scope);

    types.symbol_ty(symbol.local())
}

#[tracing::instrument(level = "debug", skip(db))]
pub fn global_symbol_type_by_name(db: &dyn Db, module: File, name: &str) -> Option<Type> {
    let symbols = symbol_table_query(db, module);
    let symbol = symbols.root_symbol_id_by_name(name)?;

    Some(global_symbol_type(db, GlobalSymbolId::new(module, symbol)))
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar, return_ref, no_eq)]
pub fn semantic_index(db: &dyn Db, file: File) -> SemanticIndex {
    let root_scope_id = SymbolTable::root_scope_id();
    let mut indexer = SemanticIndexer {
        db,
        file,
        symbol_table_builder: SymbolTableBuilder::new(),
        flow_graph_builder: FlowGraphBuilder::new(),
        scopes: vec![ScopeState {
            scope_id: root_scope_id,
            current_flow_node_id: FlowGraph::start(),
        }],
        current_definition: None,
    };

    let parsed = parse(db.upcast(), file);

    indexer.visit_body(&parsed.syntax().body);
    indexer.finish()
}

#[derive(Debug)]
struct ScopeState {
    scope_id: ScopeId,
    current_flow_node_id: FlowNodeId,
}

struct SemanticIndexer<'a> {
    db: &'a dyn Db,
    file: File,
    symbol_table_builder: SymbolTableBuilder,
    flow_graph_builder: FlowGraphBuilder,
    scopes: Vec<ScopeState>,
    /// the definition whose target(s) we are currently walking
    current_definition: Option<Definition>,
}

impl SemanticIndexer<'_> {
    pub(crate) fn finish(self) -> SemanticIndex {
        let SemanticIndexer {
            flow_graph_builder,
            symbol_table_builder,
            ..
        } = self;
        SemanticIndex {
            flow_graph: Arc::new(flow_graph_builder.finish()),
            symbol_table: Arc::new(symbol_table_builder.finish()),
            count: Count::default(),
        }
    }

    fn set_current_flow_node(&mut self, new_flow_node_id: FlowNodeId) {
        let scope_state = self.scopes.last_mut().expect("scope stack is never empty");
        scope_state.current_flow_node_id = new_flow_node_id;
    }

    fn current_flow_node(&self) -> FlowNodeId {
        self.scopes
            .last()
            .expect("scope stack is never empty")
            .current_flow_node_id
    }

    fn add_or_update_symbol(&mut self, identifier: &str, flags: SymbolFlags) -> SymbolId {
        self.symbol_table_builder
            .add_or_update_symbol(self.cur_scope(), identifier, flags)
    }

    fn add_or_update_symbol_with_def(
        &mut self,
        identifier: &str,
        definition: Definition,
    ) -> SymbolId {
        let symbol_id = self.add_or_update_symbol(identifier, SymbolFlags::IS_DEFINED);
        self.symbol_table_builder
            .add_definition(symbol_id, definition);
        let new_flow_node_id =
            self.flow_graph_builder
                .add_definition(symbol_id, definition, self.current_flow_node());
        self.set_current_flow_node(new_flow_node_id);
        symbol_id
    }

    fn push_scope(
        &mut self,
        name: &str,
        kind: ScopeKind,
        definition: Option<Definition>,
        defining_symbol: Option<SymbolId>,
    ) -> ScopeId {
        let scope_id = self.symbol_table_builder.add_child_scope(
            self.cur_scope(),
            name,
            kind,
            definition,
            defining_symbol,
        );
        self.scopes.push(ScopeState {
            scope_id,
            current_flow_node_id: FlowGraph::start(),
        });
        scope_id
    }

    fn pop_scope(&mut self) -> ScopeId {
        self.scopes
            .pop()
            .expect("Scope stack should never be empty")
            .scope_id
    }

    fn cur_scope(&self) -> ScopeId {
        self.scopes
            .last()
            .expect("Scope stack should never be empty")
            .scope_id
    }

    fn record_scope_for_node(&mut self, node: NodeWithScopeId, scope_id: ScopeId) {
        self.symbol_table_builder
            .record_scope_for_node(node, scope_id);
    }

    fn with_type_params(
        &mut self,
        name: &str,
        params: &Option<Box<ast::TypeParams>>,
        definition: Option<Definition>,
        defining_symbol: Option<SymbolId>,
        nested: impl FnOnce(&mut Self) -> ScopeId,
    ) -> ScopeId {
        if let Some(type_params) = params {
            self.push_scope(name, ScopeKind::Annotation, definition, defining_symbol);
            for type_param in &type_params.type_params {
                let name = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) => name,
                    ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { name, .. }) => name,
                    ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { name, .. }) => name,
                };
                self.add_or_update_symbol(name, SymbolFlags::IS_DEFINED);
            }
        }
        let scope_id = nested(self);
        if params.is_some() {
            self.pop_scope();
        }
        scope_id
    }
}

impl PreorderVisitor<'_> for SemanticIndexer<'_> {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        let expression_id = expr.ast_id(self.db, self.file);

        if let ast::Expr::Name(ast::ExprName { id, ctx, .. }) = expr {
            let flags = match ctx {
                ast::ExprContext::Load => SymbolFlags::IS_USED,
                ast::ExprContext::Store => SymbolFlags::IS_DEFINED,
                ast::ExprContext::Del => SymbolFlags::IS_DEFINED,
                ast::ExprContext::Invalid => SymbolFlags::empty(),
            };
            self.add_or_update_symbol(id, flags);
            if flags.contains(SymbolFlags::IS_DEFINED) {
                if let Some(curdef) = self.current_definition {
                    self.add_or_update_symbol_with_def(id, curdef);
                }
            }
        }
        let flow_expression_id = self
            .flow_graph_builder
            .record_expr(self.current_flow_node());
        debug_assert_eq!(flow_expression_id, expression_id);
        let scope_expression_id = self
            .symbol_table_builder
            .record_expression(self.cur_scope());
        debug_assert_eq!(scope_expression_id, expression_id);

        preorder::walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        // TODO need to capture more definition statements here
        match stmt {
            ast::Stmt::ClassDef(node) => {
                let class_id = node.ast_id(self.db, self.file);
                let class_definition = Definition::Class(class_id);

                let symbol_id = self.add_or_update_symbol_with_def(&node.name, class_definition);
                for decorator in &node.decorator_list {
                    self.visit_decorator(decorator);
                }
                let scope_id = self.with_type_params(
                    &node.name,
                    &node.type_params,
                    Some(class_definition),
                    Some(symbol_id),
                    |indexer| {
                        if let Some(arguments) = &node.arguments {
                            indexer.visit_arguments(arguments);
                        }
                        let scope_id = indexer.push_scope(
                            &node.name,
                            ScopeKind::Class,
                            Some(class_definition),
                            Some(symbol_id),
                        );
                        indexer.visit_body(&node.body);
                        indexer.pop_scope();
                        scope_id
                    },
                );
                self.record_scope_for_node(NodeWithScopeId::Class(class_id), scope_id);
            }
            ast::Stmt::FunctionDef(node) => {
                let function_id = node.ast_id(self.db, self.file);

                let def = Definition::Function(function_id);
                let symbol_id = self.add_or_update_symbol_with_def(&node.name, def);
                for decorator in &node.decorator_list {
                    self.visit_decorator(decorator);
                }
                let scope_id = self.with_type_params(
                    &node.name,
                    &node.type_params,
                    Some(def),
                    Some(symbol_id),
                    |indexer| {
                        indexer.visit_parameters(&node.parameters);
                        for expr in &node.returns {
                            indexer.visit_annotation(expr);
                        }
                        let scope_id = indexer.push_scope(
                            &node.name,
                            ScopeKind::Function,
                            Some(def),
                            Some(symbol_id),
                        );
                        indexer.visit_body(&node.body);
                        indexer.pop_scope();
                        scope_id
                    },
                );
                self.record_scope_for_node(NodeWithScopeId::Function(function_id), scope_id);
            }
            Stmt::Import(import @ ast::StmtImport { names, .. }) => {
                for (i, alias) in names.iter().enumerate() {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.split('.').next().unwrap()
                    };

                    let module = ModuleName::new(&alias.name.id);

                    let def = Definition::Import(ImportDefinition {
                        import: import.ast_id(self.db, self.file),
                        name: u32::try_from(i).unwrap(),
                    });
                    self.add_or_update_symbol_with_def(symbol_name, def);
                    self.symbol_table_builder
                        .add_dependency(Dependency::Module(module));
                }
            }
            Stmt::ImportFrom(
                import_from @ ast::StmtImportFrom {
                    module,
                    names,
                    level,
                    ..
                },
            ) => {
                let module = module.as_ref().map(|m| ModuleName::new(&m.id));

                for (i, alias) in names.iter().enumerate() {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.as_str()
                    };
                    let def = Definition::ImportFrom(ImportFromDefinition {
                        import: import_from.ast_id(self.db, self.file),
                        name: u32::try_from(i).unwrap(),
                    });
                    self.add_or_update_symbol_with_def(symbol_name, def);
                }

                let dependency = if let Some(module) = module {
                    match NonZeroU32::new(*level) {
                        Some(level) => Dependency::Relative {
                            level,
                            module: Some(module),
                        },
                        None => Dependency::Module(module),
                    }
                } else {
                    Dependency::Relative {
                        level: NonZeroU32::new(*level)
                            .expect("Import without a module to have a level > 0"),
                        module,
                    }
                };

                self.symbol_table_builder.add_dependency(dependency);
            }
            ast::Stmt::Assign(node) => {
                debug_assert!(self.current_definition.is_none());
                let assignment_id = node.ast_id(self.db, self.file);

                self.current_definition = Some(Definition::Assignment(assignment_id));
                ast::visitor::preorder::walk_stmt(self, stmt);
                self.current_definition = None;
            }
            ast::Stmt::If(node) => {
                // we visit the if "test" condition first regardless
                self.visit_expr(&node.test);

                // create branch node: does the if test pass or not?
                let if_branch = self.flow_graph_builder.add_branch(self.current_flow_node());

                // visit the body of the `if` clause
                self.set_current_flow_node(if_branch);
                self.visit_body(&node.body);

                // Flow node for the last if/elif condition branch; represents the "no branch
                // taken yet" possibility (where "taking a branch" means that the condition in an
                // if or elif evaluated to true and control flow went into that clause).
                let mut prior_branch = if_branch;

                // Flow node for the state after the prior if/elif/else clause; represents "we have
                // taken one of the branches up to this point." Initially set to the post-if-clause
                // state, later will be set to the phi node joining that possible path with the
                // possibility that we took a later if/elif/else clause instead.
                let mut post_prior_clause = self.current_flow_node();

                // Flag to mark if the final clause is an "else" -- if so, that means the "match no
                // clauses" path is not possible, we have to go through one of the clauses.
                let mut last_branch_is_else = false;

                for clause in &node.elif_else_clauses {
                    if clause.test.is_some() {
                        // This is an elif clause. Create a new branch node. Its predecessor is the
                        // previous branch node, because we can only take one branch in an entire
                        // if/elif/else chain, so if we take this branch, it can only be because we
                        // didn't take the previous one.
                        prior_branch = self.flow_graph_builder.add_branch(prior_branch);
                        self.set_current_flow_node(prior_branch);
                    } else {
                        // This is an else clause. No need to create a branch node; there's no
                        // branch here, if we haven't taken any previous branch, we definitely go
                        // into the "else" clause.
                        self.set_current_flow_node(prior_branch);
                        last_branch_is_else = true;
                    }
                    self.visit_elif_else_clause(clause);
                    // Update `post_prior_clause` to a new phi node joining the possibility that we
                    // took any of the previous branches with the possibility that we took the one
                    // just visited.
                    post_prior_clause = self
                        .flow_graph_builder
                        .add_phi(self.current_flow_node(), post_prior_clause);
                }

                if !last_branch_is_else {
                    // Final branch was not an "else", which means it's possible we took zero
                    // branches in the entire if/elif chain, so we need one more phi node to join
                    // the "no branches taken" possibility.
                    post_prior_clause = self
                        .flow_graph_builder
                        .add_phi(post_prior_clause, prior_branch);
                }

                // Onward, with current flow node set to our final Phi node.
                self.set_current_flow_node(post_prior_clause);
            }
            _ => {
                ast::visitor::preorder::walk_stmt(self, stmt);
            }
        }
    }
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn dependencies(db: &dyn Db, file: File) -> Arc<[ModuleName]> {
    struct DependenciesVisitor<'a> {
        db: &'a dyn Db,
        resolved_module: Option<ResolvedModule>,
        dependencies: Vec<ModuleName>,
    }

    // TODO support package imports
    impl PreorderVisitor<'_> for DependenciesVisitor<'_> {
        fn enter_node(&mut self, node: AnyNodeRef) -> TraversalSignal {
            // Don't traverse into expressions
            if node.is_expression() {
                return TraversalSignal::Skip;
            }

            TraversalSignal::Traverse
        }

        fn visit_stmt(&mut self, stmt: &Stmt) {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        self.dependencies.push(ModuleName::new(&alias.name));
                    }
                }

                Stmt::ImportFrom(from) => {
                    if let Some(level) = NonZeroU32::new(from.level) {
                        // FIXME how to handle dependencies if the current file isn't a module?
                        //  e.g. what if the current file is a jupyter notebook. We should still be able to resolve files somehow.
                        if let Some(resolved_module) = &self.resolved_module {
                            if let Some(dependency) = resolved_module.resolve_dependency(
                                self.db,
                                &Dependency::Relative {
                                    module: from
                                        .module
                                        .as_ref()
                                        .map(|module| ModuleName::new(module)),
                                    level,
                                },
                            ) {
                                self.dependencies.push(dependency);
                            };
                        }
                    } else {
                        let module = from.module.as_ref().unwrap();
                        self.dependencies.push(ModuleName::new(module));
                    }
                }
                _ => {}
            }
            preorder::walk_stmt(self, stmt);
        }
    }

    let parsed = source::parse(db.upcast(), file);

    let mut visitor = DependenciesVisitor {
        db,
        resolved_module: file_to_module(db, file),
        dependencies: Vec::new(),
    };

    // TODO change the visitor so that `visit_mod` accepts a `ModRef` node that we can construct from module.
    visitor.visit_body(&parsed.syntax().body);

    Arc::from(visitor.dependencies)

    // TODO we should extract the names of dependencies during parsing to avoid an extra traversal here.
}

#[salsa::jar(db=Db)]
pub struct Jar(
    ModuleSearchPaths,
    FunctionTypingScope,
    ClassTypingScope,
    Module,
    ast_ids::ast_ids,
    semantic_index,
    symbol_table_query,
    flow_graph::flow_graph,
    dependencies,
    resolve_module,
    file_to_module,
    typing_scopes,
    infer_function_body,
    infer_class_body,
    infer_module_body,
);

pub trait Db: source::Db + salsa::DbWithJar<Jar> + Upcast<dyn source::Db> {}
