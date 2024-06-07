use std::num::NonZeroU32;

use ruff_python_ast as ast;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_ast::AstNode;

use crate::ast_ids::{NodeKey, TypedNodeKey};
use crate::cache::KeyValueCache;
use crate::db::{QueryResult, SemanticDb, SemanticJar};
use crate::files::FileId;
use crate::module::Module;
use crate::module::ModuleName;
use crate::parse::parse;
use crate::Name;
pub(crate) use definitions::Definition;
use definitions::{ImportDefinition, ImportFromDefinition};
pub(crate) use flow_graph::ConstrainedDefinition;
use flow_graph::{FlowGraph, FlowGraphBuilder, FlowNodeId, ReachableDefinitionsIterator};
use ruff_index::{newtype_index, IndexVec};
use rustc_hash::FxHashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
pub(crate) use symbol_table::{Dependency, SymbolId};
use symbol_table::{ScopeId, ScopeKind, SymbolFlags, SymbolTable, SymbolTableBuilder};
pub(crate) use types::{infer_definition_type, infer_symbol_public_type, Type, TypeStore};

mod definitions;
mod flow_graph;
mod symbol_table;
mod types;

#[tracing::instrument(level = "debug", skip(db))]
pub fn semantic_index(db: &dyn SemanticDb, file_id: FileId) -> QueryResult<Arc<SemanticIndex>> {
    let jar: &SemanticJar = db.jar()?;

    jar.semantic_indices.get(&file_id, |_| {
        let parsed = parse(db.upcast(), file_id)?;
        Ok(Arc::from(SemanticIndex::from_ast(parsed.syntax())))
    })
}

#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_global_symbol(
    db: &dyn SemanticDb,
    module: Module,
    name: &str,
) -> QueryResult<Option<GlobalSymbolId>> {
    let file_id = module.path(db)?.file();
    let symbol_table = &semantic_index(db, file_id)?.symbol_table;
    let Some(symbol_id) = symbol_table.root_symbol_id_by_name(name) else {
        return Ok(None);
    };
    Ok(Some(GlobalSymbolId { file_id, symbol_id }))
}

#[newtype_index]
pub struct ExpressionId;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GlobalSymbolId {
    pub(crate) file_id: FileId,
    pub(crate) symbol_id: SymbolId,
}

#[derive(Debug)]
pub struct SemanticIndex {
    symbol_table: SymbolTable,
    flow_graph: FlowGraph,
    expressions: FxHashMap<NodeKey, ExpressionId>,
    expressions_by_id: IndexVec<ExpressionId, NodeKey>,
}

impl SemanticIndex {
    pub fn from_ast(module: &ast::ModModule) -> Self {
        let root_scope_id = SymbolTable::root_scope_id();
        let mut indexer = SemanticIndexer {
            symbol_table_builder: SymbolTableBuilder::new(),
            flow_graph_builder: FlowGraphBuilder::new(),
            scopes: vec![ScopeState {
                scope_id: root_scope_id,
                current_flow_node_id: FlowGraph::start(),
            }],
            expressions: FxHashMap::default(),
            expressions_by_id: IndexVec::default(),
            current_definition: None,
        };
        indexer.visit_body(&module.body);
        indexer.finish()
    }

    fn resolve_expression_id<'a>(
        &self,
        ast: &'a ast::ModModule,
        expression_id: ExpressionId,
    ) -> ast::AnyNodeRef<'a> {
        let node_key = self.expressions_by_id[expression_id];
        node_key
            .resolve(ast.as_any_node_ref())
            .expect("node to resolve")
    }

    /// Return an iterator over all definitions of `symbol_id` reachable from `use_expr`. The value
    /// of `symbol_id` in `use_expr` must originate from one of the iterated definitions (or from
    /// an external reassignment of the name outside of this scope).
    pub fn reachable_definitions(
        &self,
        symbol_id: SymbolId,
        use_expr: &ast::Expr,
    ) -> ReachableDefinitionsIterator {
        let expression_id = self.expression_id(use_expr);
        ReachableDefinitionsIterator::new(
            &self.flow_graph,
            symbol_id,
            self.flow_graph.for_expr(expression_id),
        )
    }

    pub fn expression_id(&self, expression: &ast::Expr) -> ExpressionId {
        self.expressions[&NodeKey::from_node(expression.into())]
    }

    pub fn symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }
}

#[derive(Debug)]
struct ScopeState {
    scope_id: ScopeId,
    current_flow_node_id: FlowNodeId,
}

#[derive(Debug)]
struct SemanticIndexer {
    symbol_table_builder: SymbolTableBuilder,
    flow_graph_builder: FlowGraphBuilder,
    scopes: Vec<ScopeState>,
    /// the definition whose target(s) we are currently walking
    current_definition: Option<Definition>,
    expressions: FxHashMap<NodeKey, ExpressionId>,
    expressions_by_id: IndexVec<ExpressionId, NodeKey>,
}

impl SemanticIndexer {
    pub(crate) fn finish(mut self) -> SemanticIndex {
        let SemanticIndexer {
            flow_graph_builder,
            symbol_table_builder,
            ..
        } = self;
        self.expressions.shrink_to_fit();
        self.expressions_by_id.shrink_to_fit();
        SemanticIndex {
            flow_graph: flow_graph_builder.finish(),
            symbol_table: symbol_table_builder.finish(),
            expressions: self.expressions,
            expressions_by_id: self.expressions_by_id,
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
            .add_definition(symbol_id, definition.clone());
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

    fn record_scope_for_node(&mut self, node_key: NodeKey, scope_id: ScopeId) {
        self.symbol_table_builder
            .record_scope_for_node(node_key, scope_id);
    }

    fn insert_constraint(&mut self, expr: &ast::Expr) {
        let node_key = NodeKey::from_node(expr.into());
        let expression_id = self.expressions[&node_key];
        let constraint = self
            .flow_graph_builder
            .add_constraint(self.current_flow_node(), expression_id);
        self.set_current_flow_node(constraint);
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

impl SourceOrderVisitor<'_> for SemanticIndexer {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        let node_key = NodeKey::from_node(expr.into());
        let expression_id = self.expressions_by_id.push(node_key);

        debug_assert_eq!(
            expression_id,
            self.flow_graph_builder
                .record_expr(self.current_flow_node())
        );

        debug_assert_eq!(
            expression_id,
            self.symbol_table_builder
                .record_expression(self.cur_scope())
        );

        self.expressions.insert(node_key, expression_id);

        match expr {
            ast::Expr::Name(ast::ExprName { id, ctx, .. }) => {
                let flags = match ctx {
                    ast::ExprContext::Load => SymbolFlags::IS_USED,
                    ast::ExprContext::Store => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Del => SymbolFlags::IS_DEFINED,
                    ast::ExprContext::Invalid => SymbolFlags::empty(),
                };
                self.add_or_update_symbol(id, flags);
                if flags.contains(SymbolFlags::IS_DEFINED) {
                    if let Some(curdef) = self.current_definition.clone() {
                        self.add_or_update_symbol_with_def(id, curdef);
                    }
                }
                ast::visitor::source_order::walk_expr(self, expr);
            }
            ast::Expr::Named(node) => {
                debug_assert!(self.current_definition.is_none());
                self.current_definition =
                    Some(Definition::NamedExpr(TypedNodeKey::from_node(node)));
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

                let if_branch = self.flow_graph_builder.add_branch(self.current_flow_node());

                self.set_current_flow_node(if_branch);
                self.insert_constraint(test);
                self.visit_expr(body);

                let post_body = self.current_flow_node();

                self.set_current_flow_node(if_branch);
                self.visit_expr(orelse);

                let post_else = self
                    .flow_graph_builder
                    .add_phi(self.current_flow_node(), post_body);

                self.set_current_flow_node(post_else);
            }
            _ => {
                ast::visitor::source_order::walk_expr(self, expr);
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        // TODO need to capture more definition statements here
        match stmt {
            ast::Stmt::ClassDef(node) => {
                let node_key = TypedNodeKey::from_node(node);
                let def = Definition::ClassDef(node_key.clone());
                let symbol_id = self.add_or_update_symbol_with_def(&node.name, def.clone());
                for decorator in &node.decorator_list {
                    self.visit_decorator(decorator);
                }
                let scope_id = self.with_type_params(
                    &node.name,
                    &node.type_params,
                    Some(def.clone()),
                    Some(symbol_id),
                    |indexer| {
                        if let Some(arguments) = &node.arguments {
                            indexer.visit_arguments(arguments);
                        }
                        let scope_id = indexer.push_scope(
                            &node.name,
                            ScopeKind::Class,
                            Some(def.clone()),
                            Some(symbol_id),
                        );
                        indexer.visit_body(&node.body);
                        indexer.pop_scope();
                        scope_id
                    },
                );
                self.record_scope_for_node(*node_key.erased(), scope_id);
            }
            ast::Stmt::FunctionDef(node) => {
                let node_key = TypedNodeKey::from_node(node);
                let def = Definition::FunctionDef(node_key.clone());
                let symbol_id = self.add_or_update_symbol_with_def(&node.name, def.clone());
                for decorator in &node.decorator_list {
                    self.visit_decorator(decorator);
                }
                let scope_id = self.with_type_params(
                    &node.name,
                    &node.type_params,
                    Some(def.clone()),
                    Some(symbol_id),
                    |indexer| {
                        indexer.visit_parameters(&node.parameters);
                        for expr in &node.returns {
                            indexer.visit_annotation(expr);
                        }
                        let scope_id = indexer.push_scope(
                            &node.name,
                            ScopeKind::Function,
                            Some(def.clone()),
                            Some(symbol_id),
                        );
                        indexer.visit_body(&node.body);
                        indexer.pop_scope();
                        scope_id
                    },
                );
                self.record_scope_for_node(*node_key.erased(), scope_id);
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.split('.').next().unwrap()
                    };

                    let module = ModuleName::new(&alias.name.id);

                    let def = Definition::Import(ImportDefinition {
                        module: module.clone(),
                    });
                    self.add_or_update_symbol_with_def(symbol_name, def);
                    self.symbol_table_builder
                        .add_dependency(Dependency::Module(module));
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                ..
            }) => {
                let module = module.as_ref().map(|m| ModuleName::new(&m.id));

                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.as_str()
                    };
                    let def = Definition::ImportFrom(ImportFromDefinition {
                        module: module.clone(),
                        name: Name::new(&alias.name.id),
                        level: *level,
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
                self.current_definition =
                    Some(Definition::Assignment(TypedNodeKey::from_node(node)));
                for expr in &node.targets {
                    self.visit_expr(expr);
                }

                self.current_definition = None;
                self.visit_expr(&node.value);
            }
            ast::Stmt::If(node) => {
                // TODO detect statically known truthy or falsy test (via type inference, not naive
                // AST inspection, so we can't simplify here, need to record test expression in CFG
                // for later checking)

                // we visit the if "test" condition first regardless
                self.visit_expr(&node.test);

                // create branch node: does the if test pass or not?
                let if_branch = self.flow_graph_builder.add_branch(self.current_flow_node());

                // visit the body of the `if` clause
                self.set_current_flow_node(if_branch);
                self.insert_constraint(&node.test);
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
                    if let Some(test) = &clause.test {
                        self.visit_expr(test);
                        // This is an elif clause. Create a new branch node. Its predecessor is the
                        // previous branch node, because we can only take one branch in an entire
                        // if/elif/else chain, so if we take this branch, it can only be because we
                        // didn't take the previous one.
                        prior_branch = self.flow_graph_builder.add_branch(prior_branch);
                        self.set_current_flow_node(prior_branch);
                        self.insert_constraint(test);
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
                ast::visitor::source_order::walk_stmt(self, stmt);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct SemanticIndexStorage(KeyValueCache<FileId, Arc<SemanticIndex>>);

impl Deref for SemanticIndexStorage {
    type Target = KeyValueCache<FileId, Arc<SemanticIndex>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SemanticIndexStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::semantic::symbol_table::{Symbol, SymbolIterator};
    use ruff_python_ast as ast;
    use ruff_python_ast::ModModule;
    use ruff_python_parser::{Mode, Parsed};

    use super::{Definition, ScopeKind, SemanticIndex, SymbolId};

    fn parse(code: &str) -> Parsed<ModModule> {
        ruff_python_parser::parse_unchecked(code, Mode::Module)
            .try_into_module()
            .unwrap()
    }

    fn names<I>(it: SymbolIterator<I>) -> Vec<&str>
    where
        I: Iterator<Item = SymbolId>,
    {
        let mut symbols: Vec<_> = it.map(Symbol::name).collect();
        symbols.sort_unstable();
        symbols
    }

    #[test]
    fn empty() {
        let parsed = parse("");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()).len(), 0);
    }

    #[test]
    fn simple() {
        let parsed = parse("x");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["x"]);
        assert_eq!(
            table
                .definitions(table.root_symbol_id_by_name("x").unwrap())
                .len(),
            0
        );
    }

    #[test]
    fn annotation_only() {
        let parsed = parse("x: int");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["int", "x"]);
        // TODO record definition
    }

    #[test]
    fn import() {
        let parsed = parse("import foo");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["foo"]);
        assert_eq!(
            table
                .definitions(table.root_symbol_id_by_name("foo").unwrap())
                .len(),
            1
        );
    }

    #[test]
    fn import_sub() {
        let parsed = parse("import foo.bar");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["foo"]);
    }

    #[test]
    fn import_as() {
        let parsed = parse("import foo.bar as baz");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["baz"]);
    }

    #[test]
    fn import_from() {
        let parsed = parse("from bar import foo");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["foo"]);
        assert_eq!(
            table
                .definitions(table.root_symbol_id_by_name("foo").unwrap())
                .len(),
            1
        );
        assert!(
            table.root_symbol_id_by_name("foo").is_some_and(|sid| {
                let s = sid.symbol(&table);
                s.is_defined() || !s.is_used()
            }),
            "symbols that are defined get the defined flag"
        );
    }

    #[test]
    fn assign() {
        let parsed = parse("x = foo");
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["foo", "x"]);
        assert_eq!(
            table
                .definitions(table.root_symbol_id_by_name("x").unwrap())
                .len(),
            1
        );
        assert!(
            table.root_symbol_id_by_name("foo").is_some_and(|sid| {
                let s = sid.symbol(&table);
                !s.is_defined() && s.is_used()
            }),
            "a symbol used but not defined in a scope should have only the used flag"
        );
    }

    #[test]
    fn class_scope() {
        let parsed = parse(
            "
                class C:
                    x = 1
                y = 2
                ",
        );
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["C", "y"]);
        let scopes = table.root_child_scope_ids();
        assert_eq!(scopes.len(), 1);
        let c_scope = scopes[0].scope(&table);
        assert_eq!(c_scope.kind(), ScopeKind::Class);
        assert_eq!(c_scope.name(), "C");
        assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
        assert_eq!(
            table
                .definitions(table.root_symbol_id_by_name("C").unwrap())
                .len(),
            1
        );
    }

    #[test]
    fn func_scope() {
        let parsed = parse(
            "
                def func():
                    x = 1
                y = 2
                ",
        );
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["func", "y"]);
        let scopes = table.root_child_scope_ids();
        assert_eq!(scopes.len(), 1);
        let func_scope = scopes[0].scope(&table);
        assert_eq!(func_scope.kind(), ScopeKind::Function);
        assert_eq!(func_scope.name(), "func");
        assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
        assert_eq!(
            table
                .definitions(table.root_symbol_id_by_name("func").unwrap())
                .len(),
            1
        );
    }

    #[test]
    fn dupes() {
        let parsed = parse(
            "
                def func():
                    x = 1
                def func():
                    y = 2
                ",
        );
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["func"]);
        let scopes = table.root_child_scope_ids();
        assert_eq!(scopes.len(), 2);
        let func_scope_1 = scopes[0].scope(&table);
        let func_scope_2 = scopes[1].scope(&table);
        assert_eq!(func_scope_1.kind(), ScopeKind::Function);
        assert_eq!(func_scope_1.name(), "func");
        assert_eq!(func_scope_2.kind(), ScopeKind::Function);
        assert_eq!(func_scope_2.name(), "func");
        assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
        assert_eq!(names(table.symbols_for_scope(scopes[1])), vec!["y"]);
        assert_eq!(
            table
                .definitions(table.root_symbol_id_by_name("func").unwrap())
                .len(),
            2
        );
    }

    #[test]
    fn generic_func() {
        let parsed = parse(
            "
                def func[T]():
                    x = 1
                ",
        );
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["func"]);
        let scopes = table.root_child_scope_ids();
        assert_eq!(scopes.len(), 1);
        let ann_scope_id = scopes[0];
        let ann_scope = ann_scope_id.scope(&table);
        assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
        assert_eq!(ann_scope.name(), "func");
        assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
        let scopes = table.child_scope_ids_of(ann_scope_id);
        assert_eq!(scopes.len(), 1);
        let func_scope_id = scopes[0];
        let func_scope = func_scope_id.scope(&table);
        assert_eq!(func_scope.kind(), ScopeKind::Function);
        assert_eq!(func_scope.name(), "func");
        assert_eq!(names(table.symbols_for_scope(func_scope_id)), vec!["x"]);
    }

    #[test]
    fn generic_class() {
        let parsed = parse(
            "
                class C[T]:
                    x = 1
                ",
        );
        let table = SemanticIndex::from_ast(parsed.syntax()).symbol_table;
        assert_eq!(names(table.root_symbols()), vec!["C"]);
        let scopes = table.root_child_scope_ids();
        assert_eq!(scopes.len(), 1);
        let ann_scope_id = scopes[0];
        let ann_scope = ann_scope_id.scope(&table);
        assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
        assert_eq!(ann_scope.name(), "C");
        assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
        assert!(
            table
                .symbol_by_name(ann_scope_id, "T")
                .is_some_and(|s| s.is_defined() && !s.is_used()),
            "type parameters are defined by the scope that introduces them"
        );
        let scopes = table.child_scope_ids_of(ann_scope_id);
        assert_eq!(scopes.len(), 1);
        let func_scope_id = scopes[0];
        let func_scope = func_scope_id.scope(&table);
        assert_eq!(func_scope.kind(), ScopeKind::Class);
        assert_eq!(func_scope.name(), "C");
        assert_eq!(names(table.symbols_for_scope(func_scope_id)), vec!["x"]);
    }

    #[test]
    fn reachability_trivial() {
        let parsed = parse("x = 1; x");
        let ast = parsed.syntax();
        let index = SemanticIndex::from_ast(ast);
        let table = &index.symbol_table;
        let x_sym = table
            .root_symbol_id_by_name("x")
            .expect("x symbol should exist");
        let ast::Stmt::Expr(ast::StmtExpr { value: x_use, .. }) = &ast.body[1] else {
            panic!("should be an expr")
        };
        let x_defs: Vec<_> = index
            .reachable_definitions(x_sym, x_use)
            .map(|constrained_definition| constrained_definition.definition)
            .collect();
        assert_eq!(x_defs.len(), 1);
        let Definition::Assignment(node_key) = &x_defs[0] else {
            panic!("def should be an assignment")
        };
        let Some(def_node) = node_key.resolve(ast.into()) else {
            panic!("node key should resolve")
        };
        let ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(num),
            ..
        }) = &*def_node.value
        else {
            panic!("should be a number literal")
        };
        assert_eq!(*num, 1);
    }

    #[test]
    fn expression_scope() {
        let parsed = parse("x = 1;\ndef test():\n  y = 4");
        let ast = parsed.syntax();
        let index = SemanticIndex::from_ast(ast);
        let table = &index.symbol_table;

        let x_sym = table
            .root_symbol_by_name("x")
            .expect("x symbol should exist");

        let x_stmt = ast.body[0].as_assign_stmt().unwrap();

        let x_id = index.expression_id(&x_stmt.targets[0]);

        assert_eq!(table.scope_of_expression(x_id).kind(), ScopeKind::Module);
        assert_eq!(table.scope_id_of_expression(x_id), x_sym.scope_id());

        let def = ast.body[1].as_function_def_stmt().unwrap();
        let y_stmt = def.body[0].as_assign_stmt().unwrap();
        let y_id = index.expression_id(&y_stmt.targets[0]);

        assert_eq!(table.scope_of_expression(y_id).kind(), ScopeKind::Function);
    }
}
