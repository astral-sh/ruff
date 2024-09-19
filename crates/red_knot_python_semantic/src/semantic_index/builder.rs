use std::sync::Arc;

use rustc_hash::FxHashMap;

use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{walk_expr, walk_pattern, walk_stmt, Visitor};
use ruff_python_ast::AnyParameterRef;

use crate::ast_node_ref::AstNodeRef;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::ast_ids::AstIdsBuilder;
use crate::semantic_index::definition::{
    AssignmentDefinitionNodeRef, ComprehensionDefinitionNodeRef, Definition, DefinitionNodeKey,
    DefinitionNodeRef, ForStmtDefinitionNodeRef, ImportFromDefinitionNodeRef,
};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeRef, Scope, ScopeId, ScopedSymbolId,
    SymbolTableBuilder,
};
use crate::semantic_index::use_def::{FlowSnapshot, UseDefMapBuilder};
use crate::semantic_index::SemanticIndex;
use crate::Db;

use super::constraint::{Constraint, PatternConstraint};
use super::definition::{
    DefinitionCategory, ExceptHandlerDefinitionNodeRef, MatchPatternDefinitionNodeRef,
    WithItemDefinitionNodeRef,
};

pub(super) struct SemanticIndexBuilder<'db> {
    // Builder state
    db: &'db dyn Db,
    file: File,
    module: &'db ParsedModule,
    scope_stack: Vec<FileScopeId>,
    /// The assignment we're currently visiting.
    current_assignment: Option<CurrentAssignment<'db>>,
    /// The match case we're currently visiting.
    current_match_case: Option<CurrentMatchCase<'db>>,
    /// Flow states at each `break` in the current loop.
    loop_break_states: Vec<FlowSnapshot>,

    /// Flags about the file's global scope
    has_future_annotations: bool,

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
            current_match_case: None,
            loop_break_states: vec![],

            has_future_annotations: false,

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

    fn current_use_def_map_mut(&mut self) -> &mut UseDefMapBuilder<'db> {
        let scope_id = self.current_scope();
        &mut self.use_def_maps[scope_id]
    }

    fn current_use_def_map(&self) -> &UseDefMapBuilder<'db> {
        let scope_id = self.current_scope();
        &self.use_def_maps[scope_id]
    }

    fn current_ast_ids(&mut self) -> &mut AstIdsBuilder {
        let scope_id = self.current_scope();
        &mut self.ast_ids[scope_id]
    }

    fn flow_snapshot(&self) -> FlowSnapshot {
        self.current_use_def_map().snapshot()
    }

    fn flow_restore(&mut self, state: FlowSnapshot) {
        self.current_use_def_map_mut().restore(state);
    }

    fn flow_merge(&mut self, state: FlowSnapshot) {
        self.current_use_def_map_mut().merge(state);
    }

    fn add_symbol(&mut self, name: Name) -> ScopedSymbolId {
        let (symbol_id, added) = self.current_symbol_table().add_symbol(name);
        if added {
            self.current_use_def_map_mut().add_symbol(symbol_id);
        }
        symbol_id
    }

    fn mark_symbol_bound(&mut self, id: ScopedSymbolId) {
        self.current_symbol_table().mark_symbol_bound(id);
    }

    fn mark_symbol_used(&mut self, id: ScopedSymbolId) {
        self.current_symbol_table().mark_symbol_used(id);
    }

    fn add_definition<'a>(
        &mut self,
        symbol: ScopedSymbolId,
        definition_node: impl Into<DefinitionNodeRef<'a>>,
    ) -> Definition<'db> {
        let definition_node: DefinitionNodeRef<'_> = definition_node.into();
        #[allow(unsafe_code)]
        // SAFETY: `definition_node` is guaranteed to be a child of `self.module`
        let kind = unsafe { definition_node.into_owned(self.module.clone()) };
        let category = kind.category();
        let definition = Definition::new(
            self.db,
            self.file,
            self.current_scope(),
            symbol,
            kind,
            countme::Count::default(),
        );

        let existing_definition = self
            .definitions_by_node
            .insert(definition_node.key(), definition);
        debug_assert_eq!(existing_definition, None);

        if category.is_binding() {
            self.mark_symbol_bound(symbol);
        }

        let use_def = self.current_use_def_map_mut();
        match category {
            DefinitionCategory::DeclarationAndBinding => {
                use_def.record_declaration_and_binding(symbol, definition);
            }
            DefinitionCategory::Declaration => use_def.record_declaration(symbol, definition),
            DefinitionCategory::Binding => use_def.record_binding(symbol, definition),
        }

        definition
    }

    fn add_expression_constraint(&mut self, constraint_node: &ast::Expr) -> Expression<'db> {
        let expression = self.add_standalone_expression(constraint_node);
        self.current_use_def_map_mut()
            .record_constraint(Constraint::Expression(expression));

        expression
    }

    fn add_pattern_constraint(
        &mut self,
        subject: &ast::Expr,
        pattern: &ast::Pattern,
    ) -> PatternConstraint<'db> {
        #[allow(unsafe_code)]
        let (subject, pattern) = unsafe {
            (
                AstNodeRef::new(self.module.clone(), subject),
                AstNodeRef::new(self.module.clone(), pattern),
            )
        };
        let pattern_constraint = PatternConstraint::new(
            self.db,
            self.file,
            self.current_scope(),
            subject,
            pattern,
            countme::Count::default(),
        );
        self.current_use_def_map_mut()
            .record_constraint(Constraint::Pattern(pattern_constraint));
        pattern_constraint
    }

    /// Record an expression that needs to be a Salsa ingredient, because we need to infer its type
    /// standalone (type narrowing tests, RHS of an assignment.)
    fn add_standalone_expression(&mut self, expression_node: &ast::Expr) -> Expression<'db> {
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
        expression
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
                let symbol = self.add_symbol(name.id.clone());
                // TODO create Definition for PEP 695 typevars
                // note that the "bound" on the typevar is a totally different thing than whether
                // or not a name is "bound" by a typevar declaration; the latter is always true.
                self.mark_symbol_bound(symbol);
                if let Some(bounds) = bound {
                    self.visit_expr(bounds);
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

    /// This method does several things:
    /// - It pushes a new scope onto the stack for visiting
    ///   a list/dict/set comprehension or generator expression
    /// - Inside that scope, it visits a list of [`Comprehension`] nodes,
    ///   assumed to be the "generators" that compose a comprehension
    ///   (that is, the `for x in y` and `for y in z` parts of `x for x in y for y in z`).
    /// - Inside that scope, it also calls a closure for visiting the outer `elt`
    ///   of a list/dict/set comprehension or generator expression
    /// - It then pops the new scope off the stack
    ///
    /// [`Comprehension`]: ast::Comprehension
    fn with_generators_scope(
        &mut self,
        scope: NodeWithScopeRef,
        generators: &'db [ast::Comprehension],
        visit_outer_elt: impl FnOnce(&mut Self),
    ) {
        let mut generators_iter = generators.iter();

        let Some(generator) = generators_iter.next() else {
            unreachable!("Expression must contain at least one generator");
        };

        // The `iter` of the first generator is evaluated in the outer scope, while all subsequent
        // nodes are evaluated in the inner scope.
        self.add_standalone_expression(&generator.iter);
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
            self.add_standalone_expression(&generator.iter);
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

        visit_outer_elt(self);
        self.pop_scope();
    }

    fn declare_parameter(&mut self, parameter: AnyParameterRef) {
        let symbol = self.add_symbol(parameter.name().id().clone());

        let definition = self.add_definition(symbol, parameter);

        if let AnyParameterRef::NonVariadic(with_default) = parameter {
            // Insert a mapping from the parameter to the same definition.
            // This ensures that calling `HasTy::ty` on the inner parameter returns
            // a valid type (and doesn't panic)
            let existing_definition = self.definitions_by_node.insert(
                DefinitionNodeRef::from(AnyParameterRef::Variadic(&with_default.parameter)).key(),
                definition,
            );
            debug_assert_eq!(existing_definition, None);
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
            has_future_annotations: self.has_future_annotations,
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

                self.with_type_params(
                    NodeWithScopeRef::FunctionTypeParameters(function_def),
                    function_def.type_params.as_deref(),
                    |builder| {
                        builder.visit_parameters(&function_def.parameters);
                        if let Some(expr) = &function_def.returns {
                            builder.visit_annotation(expr);
                        }

                        builder.push_scope(NodeWithScopeRef::Function(function_def));

                        // Add symbols and definitions for the parameters to the function scope.
                        for parameter in &*function_def.parameters {
                            builder.declare_parameter(parameter);
                        }

                        builder.visit_body(&function_def.body);
                        builder.pop_scope()
                    },
                );
                // The default value of the parameters needs to be evaluated in the
                // enclosing scope.
                for default in function_def
                    .parameters
                    .iter_non_variadic_params()
                    .filter_map(|param| param.default.as_deref())
                {
                    self.visit_expr(default);
                }
                // The symbol for the function name itself has to be evaluated
                // at the end to match the runtime evaluation of parameter defaults
                // and return-type annotations.
                let symbol = self.add_symbol(function_def.name.id.clone());
                self.add_definition(symbol, function_def);
            }
            ast::Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }

                let symbol = self.add_symbol(class.name.id.clone());
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

                    let symbol = self.add_symbol(symbol_name);
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

                    // Look for imports `from __future__ import annotations`, ignore `as ...`
                    // We intentionally don't enforce the rules about location of `__future__`
                    // imports here, we assume the user's intent was to apply the `__future__`
                    // import, so we still check using it (and will also emit a diagnostic about a
                    // miss-placed `__future__` import.)
                    self.has_future_annotations |= alias.name.id == "annotations"
                        && node.module.as_deref() == Some("__future__");

                    let symbol = self.add_symbol(symbol_name.clone());

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
                self.visit_expr(&node.annotation);
                if let Some(value) = &node.value {
                    self.visit_expr(value);
                }
                self.current_assignment = Some(node.into());
                self.visit_expr(&node.target);
                self.current_assignment = None;
            }
            ast::Stmt::AugAssign(
                aug_assign @ ast::StmtAugAssign {
                    range: _,
                    target,
                    op: _,
                    value,
                },
            ) => {
                debug_assert!(self.current_assignment.is_none());
                self.visit_expr(value);
                self.current_assignment = Some(aug_assign.into());
                self.visit_expr(target);
                self.current_assignment = None;
            }
            ast::Stmt::If(node) => {
                self.visit_expr(&node.test);
                let pre_if = self.flow_snapshot();
                self.add_expression_constraint(&node.test);
                self.visit_body(&node.body);
                let mut post_clauses: Vec<FlowSnapshot> = vec![];
                for clause in &node.elif_else_clauses {
                    // snapshot after every block except the last; the last one will just become
                    // the state that we merge the other snapshots into
                    post_clauses.push(self.flow_snapshot());
                    // we can only take an elif/else branch if none of the previous ones were
                    // taken, so the block entry state is always `pre_if`
                    self.flow_restore(pre_if.clone());
                    self.visit_elif_else_clause(clause);
                }
                for post_clause_state in post_clauses {
                    self.flow_merge(post_clause_state);
                }
                let has_else = node
                    .elif_else_clauses
                    .last()
                    .is_some_and(|clause| clause.test.is_none());
                if !has_else {
                    // if there's no else clause, then it's possible we took none of the branches,
                    // and the pre_if state can reach here
                    self.flow_merge(pre_if);
                }
            }
            ast::Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _,
            }) => {
                self.visit_expr(test);

                let pre_loop = self.flow_snapshot();

                // Save aside any break states from an outer loop
                let saved_break_states = std::mem::take(&mut self.loop_break_states);

                // TODO: definitions created inside the body should be fully visible
                // to other statements/expressions inside the body --Alex/Carl
                self.visit_body(body);

                // Get the break states from the body of this loop, and restore the saved outer
                // ones.
                let break_states =
                    std::mem::replace(&mut self.loop_break_states, saved_break_states);

                // We may execute the `else` clause without ever executing the body, so merge in
                // the pre-loop state before visiting `else`.
                self.flow_merge(pre_loop);
                self.visit_body(orelse);

                // Breaking out of a while loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                for break_state in break_states {
                    self.flow_merge(break_state);
                }
            }
            ast::Stmt::With(ast::StmtWith { items, body, .. }) => {
                for item in items {
                    self.visit_expr(&item.context_expr);
                    if let Some(optional_vars) = item.optional_vars.as_deref() {
                        self.add_standalone_expression(&item.context_expr);
                        self.current_assignment = Some(item.into());
                        self.visit_expr(optional_vars);
                        self.current_assignment = None;
                    }
                }
                self.visit_body(body);
            }
            ast::Stmt::Break(_) => {
                self.loop_break_states.push(self.flow_snapshot());
            }

            ast::Stmt::For(
                for_stmt @ ast::StmtFor {
                    range: _,
                    is_async: _,
                    target,
                    iter,
                    body,
                    orelse,
                },
            ) => {
                self.add_standalone_expression(iter);
                self.visit_expr(iter);

                let pre_loop = self.flow_snapshot();
                let saved_break_states = std::mem::take(&mut self.loop_break_states);

                debug_assert!(self.current_assignment.is_none());
                self.current_assignment = Some(for_stmt.into());
                self.visit_expr(target);
                self.current_assignment = None;

                // TODO: Definitions created by loop variables
                // (and definitions created inside the body)
                // are fully visible to other statements/expressions inside the body --Alex/Carl
                self.visit_body(body);

                let break_states =
                    std::mem::replace(&mut self.loop_break_states, saved_break_states);

                // We may execute the `else` clause without ever executing the body, so merge in
                // the pre-loop state before visiting `else`.
                self.flow_merge(pre_loop);
                self.visit_body(orelse);

                // Breaking out of a `for` loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                for break_state in break_states {
                    self.flow_merge(break_state);
                }
            }
            ast::Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range: _,
            }) => {
                self.add_standalone_expression(subject);
                self.visit_expr(subject);

                let after_subject = self.flow_snapshot();
                let Some((first, remaining)) = cases.split_first() else {
                    return;
                };
                self.add_pattern_constraint(subject, &first.pattern);
                self.visit_match_case(first);

                let mut post_case_snapshots = vec![];
                for case in remaining {
                    post_case_snapshots.push(self.flow_snapshot());
                    self.flow_restore(after_subject.clone());
                    self.add_pattern_constraint(subject, &case.pattern);
                    self.visit_match_case(case);
                }
                for post_clause_state in post_case_snapshots {
                    self.flow_merge(post_clause_state);
                }
                if !cases
                    .last()
                    .is_some_and(|case| case.guard.is_none() && case.pattern.is_wildcard())
                {
                    self.flow_merge(after_subject);
                }
            }
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range: _,
            }) => {
                self.visit_body(body);

                for except_handler in handlers {
                    let ast::ExceptHandler::ExceptHandler(except_handler) = except_handler;
                    let ast::ExceptHandlerExceptHandler {
                        name: symbol_name,
                        type_: handled_exceptions,
                        body: handler_body,
                        range: _,
                    } = except_handler;

                    if let Some(handled_exceptions) = handled_exceptions {
                        self.visit_expr(handled_exceptions);
                    }

                    // If `handled_exceptions` above was `None`, it's something like `except as e:`,
                    // which is invalid syntax. However, it's still pretty obvious here that the user
                    // *wanted* `e` to be bound, so we should still create a definition here nonetheless.
                    if let Some(symbol_name) = symbol_name {
                        let symbol = self.add_symbol(symbol_name.id.clone());

                        self.add_definition(
                            symbol,
                            DefinitionNodeRef::ExceptHandler(ExceptHandlerDefinitionNodeRef {
                                handler: except_handler,
                                is_star: *is_star,
                            }),
                        );
                    }

                    self.visit_body(handler_body);
                }

                self.visit_body(orelse);
                self.visit_body(finalbody);
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
                let (is_use, is_definition) = match (ctx, self.current_assignment) {
                    (ast::ExprContext::Store, Some(CurrentAssignment::AugAssign(_))) => {
                        // For augmented assignment, the target expression is also used.
                        (true, true)
                    }
                    (ast::ExprContext::Load, _) => (true, false),
                    (ast::ExprContext::Store, _) => (false, true),
                    (ast::ExprContext::Del, _) => (false, true),
                    (ast::ExprContext::Invalid, _) => (false, false),
                };
                let symbol = self.add_symbol(id.clone());
                if is_definition {
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
                        Some(CurrentAssignment::AugAssign(aug_assign)) => {
                            self.add_definition(symbol, aug_assign);
                        }
                        Some(CurrentAssignment::For(node)) => {
                            self.add_definition(
                                symbol,
                                ForStmtDefinitionNodeRef {
                                    iterable: &node.iter,
                                    target: name_node,
                                    is_async: node.is_async,
                                },
                            );
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
                                ComprehensionDefinitionNodeRef {
                                    iterable: &node.iter,
                                    target: name_node,
                                    first,
                                    is_async: node.is_async,
                                },
                            );
                        }
                        Some(CurrentAssignment::WithItem(with_item)) => {
                            self.add_definition(
                                symbol,
                                WithItemDefinitionNodeRef {
                                    node: with_item,
                                    target: name_node,
                                },
                            );
                        }
                        None => {}
                    }
                }

                if is_use {
                    self.mark_symbol_used(symbol);
                    let use_id = self.current_ast_ids().record_use(expr);
                    self.current_use_def_map_mut().record_use(symbol, use_id);
                }

                walk_expr(self, expr);
            }
            ast::Expr::Named(node) => {
                debug_assert!(self.current_assignment.is_none());
                // TODO walrus in comprehensions is implicitly nonlocal
                self.visit_expr(&node.value);
                self.current_assignment = Some(node.into());
                self.visit_expr(&node.target);
                self.current_assignment = None;
            }
            ast::Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    // The default value of the parameters needs to be evaluated in the
                    // enclosing scope.
                    for default in parameters
                        .iter_non_variadic_params()
                        .filter_map(|param| param.default.as_deref())
                    {
                        self.visit_expr(default);
                    }
                    self.visit_parameters(parameters);
                }
                self.push_scope(NodeWithScopeRef::Lambda(lambda));

                // Add symbols and definitions for the parameters to the lambda scope.
                if let Some(parameters) = &lambda.parameters {
                    for parameter in &**parameters {
                        self.declare_parameter(parameter);
                    }
                }

                self.visit_expr(lambda.body.as_ref());
                self.pop_scope();
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
            ast::Expr::ListComp(
                list_comprehension @ ast::ExprListComp {
                    elt, generators, ..
                },
            ) => {
                self.with_generators_scope(
                    NodeWithScopeRef::ListComprehension(list_comprehension),
                    generators,
                    |builder| builder.visit_expr(elt),
                );
            }
            ast::Expr::SetComp(
                set_comprehension @ ast::ExprSetComp {
                    elt, generators, ..
                },
            ) => {
                self.with_generators_scope(
                    NodeWithScopeRef::SetComprehension(set_comprehension),
                    generators,
                    |builder| builder.visit_expr(elt),
                );
            }
            ast::Expr::Generator(
                generator @ ast::ExprGenerator {
                    elt, generators, ..
                },
            ) => {
                self.with_generators_scope(
                    NodeWithScopeRef::GeneratorExpression(generator),
                    generators,
                    |builder| builder.visit_expr(elt),
                );
            }
            ast::Expr::DictComp(
                dict_comprehension @ ast::ExprDictComp {
                    key,
                    value,
                    generators,
                    ..
                },
            ) => {
                self.with_generators_scope(
                    NodeWithScopeRef::DictComprehension(dict_comprehension),
                    generators,
                    |builder| {
                        builder.visit_expr(key);
                        builder.visit_expr(value);
                    },
                );
            }
            _ => {
                walk_expr(self, expr);
            }
        }
    }

    fn visit_parameters(&mut self, parameters: &'ast ast::Parameters) {
        // Intentionally avoid walking default expressions, as we handle them in the enclosing
        // scope.
        for parameter in parameters.iter().map(ast::AnyParameterRef::as_parameter) {
            self.visit_parameter(parameter);
        }
    }

    fn visit_match_case(&mut self, match_case: &'ast ast::MatchCase) {
        debug_assert!(self.current_match_case.is_none());
        self.current_match_case = Some(CurrentMatchCase::new(&match_case.pattern));
        self.visit_pattern(&match_case.pattern);
        self.current_match_case = None;

        if let Some(expr) = &match_case.guard {
            self.visit_expr(expr);
        }
        self.visit_body(&match_case.body);
    }

    fn visit_pattern(&mut self, pattern: &'ast ast::Pattern) {
        if let ast::Pattern::MatchStar(ast::PatternMatchStar {
            name: Some(name),
            range: _,
        }) = pattern
        {
            let symbol = self.add_symbol(name.id().clone());
            let state = self.current_match_case.as_ref().unwrap();
            self.add_definition(
                symbol,
                MatchPatternDefinitionNodeRef {
                    pattern: state.pattern,
                    identifier: name,
                    index: state.index,
                },
            );
        }

        walk_pattern(self, pattern);

        if let ast::Pattern::MatchAs(ast::PatternMatchAs {
            name: Some(name), ..
        })
        | ast::Pattern::MatchMapping(ast::PatternMatchMapping {
            rest: Some(name), ..
        }) = pattern
        {
            let symbol = self.add_symbol(name.id().clone());
            let state = self.current_match_case.as_ref().unwrap();
            self.add_definition(
                symbol,
                MatchPatternDefinitionNodeRef {
                    pattern: state.pattern,
                    identifier: name,
                    index: state.index,
                },
            );
        }

        self.current_match_case.as_mut().unwrap().index += 1;
    }
}

#[derive(Copy, Clone, Debug)]
enum CurrentAssignment<'a> {
    Assign(&'a ast::StmtAssign),
    AnnAssign(&'a ast::StmtAnnAssign),
    AugAssign(&'a ast::StmtAugAssign),
    For(&'a ast::StmtFor),
    Named(&'a ast::ExprNamed),
    Comprehension {
        node: &'a ast::Comprehension,
        first: bool,
    },
    WithItem(&'a ast::WithItem),
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

impl<'a> From<&'a ast::StmtAugAssign> for CurrentAssignment<'a> {
    fn from(value: &'a ast::StmtAugAssign) -> Self {
        Self::AugAssign(value)
    }
}

impl<'a> From<&'a ast::StmtFor> for CurrentAssignment<'a> {
    fn from(value: &'a ast::StmtFor) -> Self {
        Self::For(value)
    }
}

impl<'a> From<&'a ast::ExprNamed> for CurrentAssignment<'a> {
    fn from(value: &'a ast::ExprNamed) -> Self {
        Self::Named(value)
    }
}

impl<'a> From<&'a ast::WithItem> for CurrentAssignment<'a> {
    fn from(value: &'a ast::WithItem) -> Self {
        Self::WithItem(value)
    }
}

struct CurrentMatchCase<'a> {
    /// The pattern that's part of the current match case.
    pattern: &'a ast::Pattern,

    /// The index of the sub-pattern that's being currently visited within the pattern.
    ///
    /// For example:
    /// ```py
    /// match subject:
    ///     case a as b: ...
    ///     case [a, b]: ...
    ///     case a | b: ...
    /// ```
    ///
    /// In all of the above cases, the index would be 0 for `a` and 1 for `b`.
    index: u32,
}

impl<'a> CurrentMatchCase<'a> {
    fn new(pattern: &'a ast::Pattern) -> Self {
        Self { pattern, index: 0 }
    }
}
