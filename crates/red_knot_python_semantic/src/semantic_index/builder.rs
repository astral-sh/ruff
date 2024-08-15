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
    AssignmentDefinitionNodeRef, ComprehensionDefinitionNodeRef, Definition, DefinitionKind,
    DefinitionNodeKey, DefinitionNodeRef, ImportFromDefinitionNodeRef,
};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeRef, Scope, ScopeId, ScopedSymbolId, SymbolFlags,
    SymbolTableBuilder,
};
use crate::semantic_index::use_def::{BasicBlockId, UseDefMapBuilder};
use crate::semantic_index::SemanticIndex;
use crate::Db;

pub(super) struct SemanticIndexBuilder<'db> {
    // Builder state
    db: &'db dyn Db,
    file: File,
    module: &'db ParsedModule,
    scope_stack: Vec<FileScopeId>,
    /// The assignment we're currently visiting.
    current_assignment: Option<CurrentAssignment<'db>>,
    /// Basic block ending at each `break` in the current loop.
    loop_breaks: Vec<BasicBlockId>,

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
            loop_breaks: vec![],

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
        self.use_def_maps
            .push(UseDefMapBuilder::new(self.db, self.file, file_scope_id));
        let ast_id_scope = self.ast_ids.push(AstIdsBuilder::new());

        #[allow(unsafe_code)]
        // SAFETY: `node` is guaranteed to be a child of `self.module`
        let scope_id = ScopeId::new(
            self.db,
            self.file,
            file_scope_id,
            unsafe { node.to_kind(self.module.clone()) },
            countme::Count::default(),
        );

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

    /// Start a new basic block and return the previous block's ID.
    fn next_block(&mut self) -> BasicBlockId {
        self.current_use_def_map().next_block(/* sealed */ true)
    }

    /// Start a new unsealed basic block and return the previous block's ID.
    fn next_block_unsealed(&mut self) -> BasicBlockId {
        self.current_use_def_map().next_block(/* sealed */ false)
    }

    /// Seal an unsealed basic block.
    fn seal_block(&mut self) {
        self.current_use_def_map().seal_current_block();
    }

    /// Start a new basic block with the given block as predecessor.
    fn new_block_from(&mut self, predecessor: BasicBlockId) {
        self.current_use_def_map()
            .new_block_from(predecessor, /* sealed */ true);
    }

    /// Add a predecessor to the current block.
    fn merge_block(&mut self, predecessor: BasicBlockId) {
        self.current_use_def_map().merge_block(predecessor);
    }

    /// Add predecessors to the current block.
    fn merge_blocks(&mut self, predecessors: Vec<BasicBlockId>) {
        self.current_use_def_map().merge_blocks(predecessors);
    }

    fn add_or_update_symbol(&mut self, name: Name, flags: SymbolFlags) -> ScopedSymbolId {
        let symbol_table = self.current_symbol_table();
        symbol_table.add_or_update_symbol(name, flags)
    }

    fn add_definition<'a>(
        &mut self,
        symbol: ScopedSymbolId,
        definition_node: impl Into<DefinitionNodeRef<'a>>,
    ) -> Definition<'db> {
        let definition_node: DefinitionNodeRef<'_> = definition_node.into();
        let definition = Definition::new(
            self.db,
            self.file,
            self.current_scope(),
            symbol,
            #[allow(unsafe_code)]
            DefinitionKind::Node(unsafe { definition_node.into_owned(self.module.clone()) }),
            countme::Count::default(),
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
            countme::Count::default(),
        );
        self.expressions_by_node
            .insert(expression_node.into(), expression);
    }

    fn with_type_params(
        &mut self,
        with_scope: NodeWithScopeRef,
        type_params: Option<&'db ast::TypeParams>,
        nested: impl FnOnce(&mut Self) -> FileScopeId,
    ) -> FileScopeId {
        if let Some(type_params) = type_params {
            self.push_scope(with_scope);

            for type_param in &type_params.type_params {
                let (name, bound, default) = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar {
                        range: _,
                        name,
                        bound,
                        default,
                    }) => (name, bound, default),
                    ast::TypeParam::ParamSpec(ast::TypeParamParamSpec {
                        name, default, ..
                    }) => (name, &None, default),
                    ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple {
                        name,
                        default,
                        ..
                    }) => (name, &None, default),
                };
                // TODO create Definition for typevars
                self.add_or_update_symbol(name.id.clone(), SymbolFlags::IS_DEFINED);
                if let Some(bound) = bound {
                    self.visit_expr(bound);
                }
                if let Some(default) = default {
                    self.visit_expr(default);
                }
            }
        }

        let nested_scope = nested(self);

        if type_params.is_some() {
            self.pop_scope();
        }

        nested_scope
    }

    /// Visit a list of [`Comprehension`] nodes, assumed to be the "generators" that compose a
    /// comprehension (that is, the `for x in y` and `for y in z` parts of `x for x in y for y in z`.)
    ///
    /// [`Comprehension`]: ast::Comprehension
    fn visit_generators(&mut self, scope: NodeWithScopeRef, generators: &'db [ast::Comprehension]) {
        let mut generators_iter = generators.iter();

        let Some(generator) = generators_iter.next() else {
            unreachable!("Expression must contain at least one generator");
        };

        // The `iter` of the first generator is evaluated in the outer scope, while all subsequent
        // nodes are evaluated in the inner scope.
        self.visit_expr(&generator.iter);
        self.push_scope(scope);

        self.current_assignment = Some(CurrentAssignment::Comprehension {
            node: generator,
            first: true,
        });
        self.visit_expr(&generator.target);
        self.current_assignment = None;

        for expr in &generator.ifs {
            self.visit_expr(expr);
        }

        for generator in generators_iter {
            self.visit_expr(&generator.iter);

            self.current_assignment = Some(CurrentAssignment::Comprehension {
                node: generator,
                first: false,
            });
            self.visit_expr(&generator.target);
            self.current_assignment = None;

            for expr in &generator.ifs {
                self.visit_expr(expr);
            }
        }
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
                    NodeWithScopeRef::FunctionTypeParameters(function_def),
                    function_def.type_params.as_deref(),
                    |builder| {
                        builder.visit_parameters(&function_def.parameters);
                        if let Some(expr) = &function_def.returns {
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

                self.with_type_params(
                    NodeWithScopeRef::ClassTypeParameters(class),
                    class.type_params.as_deref(),
                    |builder| {
                        if let Some(arguments) = &class.arguments {
                            builder.visit_arguments(arguments);
                        }

                        builder.push_scope(NodeWithScopeRef::Class(class));
                        builder.visit_body(&class.body);

                        builder.pop_scope()
                    },
                );
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
                if let Some(value) = &node.value {
                    self.visit_expr(value);
                }
                self.current_assignment = Some(node.into());
                self.visit_expr(&node.target);
                self.current_assignment = None;
            }
            ast::Stmt::If(node) => {
                self.visit_expr(&node.test);
                let pre_if = self.next_block();
                self.visit_body(&node.body);
                let mut post_clauses: Vec<BasicBlockId> = vec![];
                for clause in &node.elif_else_clauses {
                    // snapshot after every block except the last; the last one will just become
                    // the state that we merge the other snapshots into
                    post_clauses.push(self.next_block());
                    // we can only take an elif/else branch if none of the previous ones were
                    // taken, so the block entry state is always `pre_if`
                    self.new_block_from(pre_if);
                    self.visit_elif_else_clause(clause);
                }
                self.next_block_unsealed();
                let has_else = node
                    .elif_else_clauses
                    .last()
                    .is_some_and(|clause| clause.test.is_none());
                if !has_else {
                    // if there's no else clause, then it's possible we took none of the branches,
                    // and the pre_if state can reach here
                    self.merge_block(pre_if);
                }
                self.merge_blocks(post_clauses);
                self.seal_block();
            }
            ast::Stmt::While(node) => {
                self.visit_expr(&node.test);

                let pre_loop = self.next_block();

                // Save aside any break states from an outer loop
                let saved_break_states = std::mem::take(&mut self.loop_breaks);
                self.visit_body(&node.body);
                // Get the break states from the body of this loop, and restore the saved outer
                // ones.
                let break_states = std::mem::replace(&mut self.loop_breaks, saved_break_states);

                // We may execute the `else` clause without ever executing the body, so merge in
                // the pre-loop state before visiting `else`.
                self.next_block_unsealed();
                self.merge_block(pre_loop);
                self.seal_block();
                self.visit_body(&node.orelse);

                // Breaking out of a while loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                self.next_block_unsealed();
                self.merge_blocks(break_states);
                self.seal_block();
            }
            ast::Stmt::Break(_) => {
                let block_id = self.next_block();
                self.loop_breaks.push(block_id);
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
            ast::Expr::Name(name_node @ ast::ExprName { id, ctx, .. }) => {
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
                            // TODO(dhruvmanila): If the current scope is a comprehension, then the
                            // named expression is implicitly nonlocal. This is yet to be
                            // implemented.
                            self.add_definition(symbol, named);
                        }
                        Some(CurrentAssignment::Comprehension { node, first }) => {
                            self.add_definition(
                                symbol,
                                ComprehensionDefinitionNodeRef { node, first },
                            );
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
            ast::Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
                self.push_scope(NodeWithScopeRef::Lambda(lambda));
                self.visit_expr(lambda.body.as_ref());
            }
            ast::Expr::If(ast::ExprIf {
                body, test, orelse, ..
            }) => {
                // TODO detect statically known truthy or falsy test (via type inference, not naive
                // AST inspection, so we can't simplify here, need to record test expression for
                // later checking)
                self.visit_expr(test);
                let pre_if = self.next_block();
                self.visit_expr(body);
                let post_body = self.next_block();
                self.new_block_from(pre_if);
                self.visit_expr(orelse);
                self.next_block_unsealed();
                self.merge_block(post_body);
                self.seal_block();
            }
            ast::Expr::ListComp(
                list_comprehension @ ast::ExprListComp {
                    elt, generators, ..
                },
            ) => {
                self.visit_generators(
                    NodeWithScopeRef::ListComprehension(list_comprehension),
                    generators,
                );
                self.visit_expr(elt);
            }
            ast::Expr::SetComp(
                set_comprehension @ ast::ExprSetComp {
                    elt, generators, ..
                },
            ) => {
                self.visit_generators(
                    NodeWithScopeRef::SetComprehension(set_comprehension),
                    generators,
                );
                self.visit_expr(elt);
            }
            ast::Expr::Generator(
                generator @ ast::ExprGenerator {
                    elt, generators, ..
                },
            ) => {
                self.visit_generators(NodeWithScopeRef::GeneratorExpression(generator), generators);
                self.visit_expr(elt);
            }
            ast::Expr::DictComp(
                dict_comprehension @ ast::ExprDictComp {
                    key,
                    value,
                    generators,
                    ..
                },
            ) => {
                self.visit_generators(
                    NodeWithScopeRef::DictComprehension(dict_comprehension),
                    generators,
                );
                self.visit_expr(key);
                self.visit_expr(value);
            }
            _ => {
                walk_expr(self, expr);
            }
        }

        if matches!(
            expr,
            ast::Expr::Lambda(_)
                | ast::Expr::ListComp(_)
                | ast::Expr::SetComp(_)
                | ast::Expr::Generator(_)
                | ast::Expr::DictComp(_)
        ) {
            self.pop_scope();
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum CurrentAssignment<'a> {
    Assign(&'a ast::StmtAssign),
    AnnAssign(&'a ast::StmtAnnAssign),
    Named(&'a ast::ExprNamed),
    Comprehension {
        node: &'a ast::Comprehension,
        first: bool,
    },
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
