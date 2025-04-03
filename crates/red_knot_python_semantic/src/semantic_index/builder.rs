use std::sync::Arc;

use except_handlers::TryNodeContextStackManager;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::IndexVec;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{walk_expr, walk_pattern, walk_stmt, Visitor};
use ruff_python_ast::{self as ast, ExprContext};

use crate::ast_node_ref::AstNodeRef;
use crate::module_name::ModuleName;
use crate::module_resolver::resolve_module;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::ast_ids::AstIdsBuilder;
use crate::semantic_index::attribute_assignment::{AttributeAssignment, AttributeAssignments};
use crate::semantic_index::definition::{
    AssignmentDefinitionNodeRef, ComprehensionDefinitionNodeRef, Definition, DefinitionCategory,
    DefinitionNodeKey, DefinitionNodeRef, Definitions, ExceptHandlerDefinitionNodeRef,
    ForStmtDefinitionNodeRef, ImportDefinitionNodeRef, ImportFromDefinitionNodeRef,
    MatchPatternDefinitionNodeRef, StarImportDefinitionNodeRef, WithItemDefinitionNodeRef,
};
use crate::semantic_index::expression::{Expression, ExpressionKind};
use crate::semantic_index::predicate::{
    PatternPredicate, PatternPredicateKind, Predicate, PredicateNode, ScopedPredicateId,
};
use crate::semantic_index::re_exports::exported_names;
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeRef, Scope, ScopeId, ScopeKind, ScopedSymbolId,
    SymbolTableBuilder,
};
use crate::semantic_index::use_def::{
    EagerBindingsKey, FlowSnapshot, ScopedEagerBindingsId, UseDefMapBuilder,
};
use crate::semantic_index::visibility_constraints::{
    ScopedVisibilityConstraintId, VisibilityConstraintsBuilder,
};
use crate::semantic_index::SemanticIndex;
use crate::unpack::{Unpack, UnpackKind, UnpackPosition, UnpackValue};
use crate::Db;

mod except_handlers;

#[derive(Clone, Debug, Default)]
struct Loop {
    /// Flow states at each `break` in the current loop.
    break_states: Vec<FlowSnapshot>,
}

impl Loop {
    fn push_break(&mut self, state: FlowSnapshot) {
        self.break_states.push(state);
    }
}

struct ScopeInfo {
    file_scope_id: FileScopeId,
    /// Current loop state; None if we are not currently visiting a loop
    current_loop: Option<Loop>,
}

pub(super) struct SemanticIndexBuilder<'db> {
    // Builder state
    db: &'db dyn Db,
    file: File,
    module: &'db ParsedModule,
    scope_stack: Vec<ScopeInfo>,
    /// The assignments we're currently visiting, with
    /// the most recent visit at the end of the Vec
    current_assignments: Vec<CurrentAssignment<'db>>,
    /// The match case we're currently visiting.
    current_match_case: Option<CurrentMatchCase<'db>>,
    /// The name of the first function parameter of the innermost function that we're currently visiting.
    current_first_parameter_name: Option<&'db str>,

    /// Per-scope contexts regarding nested `try`/`except` statements
    try_node_context_stack_manager: TryNodeContextStackManager,

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
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definitions<'db>>,
    expressions_by_node: FxHashMap<ExpressionNodeKey, Expression<'db>>,
    imported_modules: FxHashSet<ModuleName>,
    attribute_assignments: FxHashMap<FileScopeId, AttributeAssignments<'db>>,
    eager_bindings: FxHashMap<EagerBindingsKey, ScopedEagerBindingsId>,
}

impl<'db> SemanticIndexBuilder<'db> {
    pub(super) fn new(db: &'db dyn Db, file: File, parsed: &'db ParsedModule) -> Self {
        let mut builder = Self {
            db,
            file,
            module: parsed,
            scope_stack: Vec::new(),
            current_assignments: vec![],
            current_match_case: None,
            current_first_parameter_name: None,
            try_node_context_stack_manager: TryNodeContextStackManager::default(),

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

            imported_modules: FxHashSet::default(),

            attribute_assignments: FxHashMap::default(),

            eager_bindings: FxHashMap::default(),
        };

        builder.push_scope_with_parent(NodeWithScopeRef::Module, None);

        builder
    }

    fn current_scope_info(&self) -> &ScopeInfo {
        self.scope_stack
            .last()
            .expect("SemanticIndexBuilder should have created a root scope")
    }

    fn current_scope_info_mut(&mut self) -> &mut ScopeInfo {
        self.scope_stack
            .last_mut()
            .expect("SemanticIndexBuilder should have created a root scope")
    }

    fn current_scope(&self) -> FileScopeId {
        self.current_scope_info().file_scope_id
    }

    fn current_scope_is_global_scope(&self) -> bool {
        self.scope_stack.len() == 1
    }

    /// Returns the scope ID of the surrounding class body scope if the current scope
    /// is a method inside a class body. Returns `None` otherwise, e.g. if the current
    /// scope is a function body outside of a class, or if the current scope is not a
    /// function body.
    fn is_method_of_class(&self) -> Option<FileScopeId> {
        let mut scopes_rev = self.scope_stack.iter().rev();
        let current = scopes_rev.next()?;
        let parent = scopes_rev.next()?;

        match (
            self.scopes[current.file_scope_id].kind(),
            self.scopes[parent.file_scope_id].kind(),
        ) {
            (ScopeKind::Function, ScopeKind::Class) => Some(parent.file_scope_id),
            _ => None,
        }
    }

    /// Push a new loop, returning the outer loop, if any.
    fn push_loop(&mut self) -> Option<Loop> {
        self.current_scope_info_mut()
            .current_loop
            .replace(Loop::default())
    }

    /// Pop a loop, replacing with the previous saved outer loop, if any.
    fn pop_loop(&mut self, outer_loop: Option<Loop>) -> Loop {
        std::mem::replace(&mut self.current_scope_info_mut().current_loop, outer_loop)
            .expect("pop_loop() should not be called without a prior push_loop()")
    }

    fn current_loop_mut(&mut self) -> Option<&mut Loop> {
        self.current_scope_info_mut().current_loop.as_mut()
    }

    fn push_scope(&mut self, node: NodeWithScopeRef) {
        let parent = self.current_scope();
        self.push_scope_with_parent(node, Some(parent));
    }

    fn push_scope_with_parent(&mut self, node: NodeWithScopeRef, parent: Option<FileScopeId>) {
        let children_start = self.scopes.next_index() + 1;

        // SAFETY: `node` is guaranteed to be a child of `self.module`
        #[allow(unsafe_code)]
        let node_with_kind = unsafe { node.to_kind(self.module.clone()) };

        let scope = Scope::new(parent, node_with_kind, children_start..children_start);
        self.try_node_context_stack_manager.enter_nested_scope();

        let file_scope_id = self.scopes.push(scope);
        self.symbol_tables.push(SymbolTableBuilder::default());
        self.use_def_maps.push(UseDefMapBuilder::default());
        let ast_id_scope = self.ast_ids.push(AstIdsBuilder::default());

        let scope_id = ScopeId::new(self.db, self.file, file_scope_id, countme::Count::default());

        self.scope_ids_by_scope.push(scope_id);
        let previous = self.scopes_by_node.insert(node.node_key(), file_scope_id);
        debug_assert_eq!(previous, None);

        debug_assert_eq!(ast_id_scope, file_scope_id);

        self.scope_stack.push(ScopeInfo {
            file_scope_id,
            current_loop: None,
        });
    }

    fn pop_scope(&mut self) -> FileScopeId {
        self.try_node_context_stack_manager.exit_scope();

        let ScopeInfo {
            file_scope_id: popped_scope_id,
            ..
        } = self
            .scope_stack
            .pop()
            .expect("Root scope should be present");

        let children_end = self.scopes.next_index();
        let popped_scope = &mut self.scopes[popped_scope_id];
        popped_scope.extend_descendants(children_end);

        if !popped_scope.is_eager() {
            return popped_scope_id;
        }

        // If the scope that we just popped off is an eager scope, we need to "lock" our view of
        // which bindings reach each of the uses in the scope. Loop through each enclosing scope,
        // looking for any that bind each symbol.
        for enclosing_scope_info in self.scope_stack.iter().rev() {
            let enclosing_scope_id = enclosing_scope_info.file_scope_id;
            let enclosing_scope_kind = self.scopes[enclosing_scope_id].kind();
            let enclosing_symbol_table = &self.symbol_tables[enclosing_scope_id];

            // Names bound in class scopes are never visible to nested scopes, so we never need to
            // save eager scope bindings in a class scope.
            if enclosing_scope_kind.is_class() {
                continue;
            }

            for nested_symbol in self.symbol_tables[popped_scope_id].symbols() {
                // Skip this symbol if this enclosing scope doesn't contain any bindings for it.
                // Note that even if this symbol is bound in the popped scope,
                // it may refer to the enclosing scope bindings
                // so we also need to snapshot the bindings of the enclosing scope.

                let Some(enclosing_symbol_id) =
                    enclosing_symbol_table.symbol_id_by_name(nested_symbol.name())
                else {
                    continue;
                };
                let enclosing_symbol = enclosing_symbol_table.symbol(enclosing_symbol_id);
                if !enclosing_symbol.is_bound() {
                    continue;
                }

                // Snapshot the bindings of this symbol that are visible at this point in this
                // enclosing scope.
                let key = EagerBindingsKey {
                    enclosing_scope: enclosing_scope_id,
                    enclosing_symbol: enclosing_symbol_id,
                    nested_scope: popped_scope_id,
                };
                let eager_bindings = self.use_def_maps[enclosing_scope_id]
                    .snapshot_eager_bindings(enclosing_symbol_id);
                self.eager_bindings.insert(key, eager_bindings);
            }

            // Lazy scopes are "sticky": once we see a lazy scope we stop doing lookups
            // eagerly, even if we would encounter another eager enclosing scope later on.
            if !enclosing_scope_kind.is_eager() {
                break;
            }
        }

        popped_scope_id
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

    fn current_visibility_constraints_mut(&mut self) -> &mut VisibilityConstraintsBuilder {
        let scope_id = self.current_scope();
        &mut self.use_def_maps[scope_id].visibility_constraints
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

    fn mark_symbol_declared(&mut self, id: ScopedSymbolId) {
        self.current_symbol_table().mark_symbol_declared(id);
    }

    fn mark_symbol_used(&mut self, id: ScopedSymbolId) {
        self.current_symbol_table().mark_symbol_used(id);
    }

    fn add_entry_for_definition_key(&mut self, key: DefinitionNodeKey) -> &mut Definitions<'db> {
        self.definitions_by_node.entry(key).or_default()
    }

    /// Add a [`Definition`] associated with the `definition_node` AST node.
    ///
    /// ## Panics
    ///
    /// This method panics if `debug_assertions` are enabled and the `definition_node` AST node
    /// already has a [`Definition`] associated with it. This is an important invariant to maintain
    /// for all nodes *except* [`ast::Alias`] nodes representing `*` imports.
    fn add_definition(
        &mut self,
        symbol: ScopedSymbolId,
        definition_node: impl Into<DefinitionNodeRef<'db>> + std::fmt::Debug + Copy,
    ) -> Definition<'db> {
        let (definition, num_definitions) =
            self.push_additional_definition(symbol, definition_node);
        debug_assert_eq!(
            num_definitions,
            1,
            "Attempted to create multiple `Definition`s associated with AST node {definition_node:?}"
        );
        definition
    }

    /// Push a new [`Definition`] onto the list of definitions
    /// associated with the `definition_node` AST node.
    ///
    /// Returns a 2-element tuple, where the first element is the newly created [`Definition`]
    /// and the second element is the number of definitions that are now associated with
    /// `definition_node`.
    ///
    /// This method should only be used when adding a definition associated with a `*` import.
    /// All other nodes can only ever be associated with exactly 1 or 0 [`Definition`]s.
    /// For any node other than an [`ast::Alias`] representing a `*` import,
    /// prefer to use `self.add_definition()`, which ensures that this invariant is maintained.
    fn push_additional_definition(
        &mut self,
        symbol: ScopedSymbolId,
        definition_node: impl Into<DefinitionNodeRef<'db>>,
    ) -> (Definition<'db>, usize) {
        let definition_node: DefinitionNodeRef<'_> = definition_node.into();
        #[allow(unsafe_code)]
        // SAFETY: `definition_node` is guaranteed to be a child of `self.module`
        let kind = unsafe { definition_node.into_owned(self.module.clone()) };
        let category = kind.category(self.file.is_stub(self.db.upcast()));
        let is_reexported = kind.is_reexported();

        let definition = Definition::new(
            self.db,
            self.file,
            self.current_scope(),
            symbol,
            kind,
            is_reexported,
            countme::Count::default(),
        );

        let num_definitions = {
            let definitions = self.add_entry_for_definition_key(definition_node.key());
            definitions.push(definition);
            definitions.len()
        };

        if category.is_binding() {
            self.mark_symbol_bound(symbol);
        }
        if category.is_declaration() {
            self.mark_symbol_declared(symbol);
        }

        let use_def = self.current_use_def_map_mut();
        match category {
            DefinitionCategory::DeclarationAndBinding => {
                use_def.record_declaration_and_binding(symbol, definition);
            }
            DefinitionCategory::Declaration => use_def.record_declaration(symbol, definition),
            DefinitionCategory::Binding => use_def.record_binding(symbol, definition),
        }

        let mut try_node_stack_manager = std::mem::take(&mut self.try_node_context_stack_manager);
        try_node_stack_manager.record_definition(self);
        self.try_node_context_stack_manager = try_node_stack_manager;

        (definition, num_definitions)
    }

    fn record_expression_narrowing_constraint(
        &mut self,
        precide_node: &ast::Expr,
    ) -> Predicate<'db> {
        let predicate = self.build_predicate(precide_node);
        self.record_narrowing_constraint(predicate);
        predicate
    }

    fn build_predicate(&mut self, predicate_node: &ast::Expr) -> Predicate<'db> {
        let expression = self.add_standalone_expression(predicate_node);
        Predicate {
            node: PredicateNode::Expression(expression),
            is_positive: true,
        }
    }

    /// Adds a new predicate to the list of all predicates, but does not record it. Returns the
    /// predicate ID for later recording using
    /// [`SemanticIndexBuilder::record_narrowing_constraint_id`].
    fn add_predicate(&mut self, predicate: Predicate<'db>) -> ScopedPredicateId {
        self.current_use_def_map_mut().add_predicate(predicate)
    }

    /// Negates a predicate and adds it to the list of all predicates, does not record it.
    fn add_negated_predicate(&mut self, predicate: Predicate<'db>) -> ScopedPredicateId {
        let negated = Predicate {
            node: predicate.node,
            is_positive: false,
        };
        self.current_use_def_map_mut().add_predicate(negated)
    }

    /// Records a previously added narrowing constraint by adding it to all live bindings.
    fn record_narrowing_constraint_id(&mut self, predicate: ScopedPredicateId) {
        self.current_use_def_map_mut()
            .record_narrowing_constraint(predicate);
    }

    /// Adds and records a narrowing constraint, i.e. adds it to all live bindings.
    fn record_narrowing_constraint(&mut self, predicate: Predicate<'db>) {
        let use_def = self.current_use_def_map_mut();
        let predicate_id = use_def.add_predicate(predicate);
        use_def.record_narrowing_constraint(predicate_id);
    }

    /// Negates the given predicate and then adds it as a narrowing constraint to all live
    /// bindings.
    fn record_negated_narrowing_constraint(
        &mut self,
        predicate: Predicate<'db>,
    ) -> ScopedPredicateId {
        let id = self.add_negated_predicate(predicate);
        self.record_narrowing_constraint_id(id);
        id
    }

    /// Records a previously added visibility constraint by applying it to all live bindings
    /// and declarations.
    fn record_visibility_constraint_id(&mut self, constraint: ScopedVisibilityConstraintId) {
        self.current_use_def_map_mut()
            .record_visibility_constraint(constraint);
    }

    /// Negates the given visibility constraint and then adds it to all live bindings and declarations.
    fn record_negated_visibility_constraint(
        &mut self,
        constraint: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        let id = self
            .current_visibility_constraints_mut()
            .add_not_constraint(constraint);
        self.record_visibility_constraint_id(id);
        id
    }

    /// Records a visibility constraint by applying it to all live bindings and declarations.
    fn record_visibility_constraint(
        &mut self,
        predicate: Predicate<'db>,
    ) -> ScopedVisibilityConstraintId {
        let predicate_id = self.current_use_def_map_mut().add_predicate(predicate);
        let id = self
            .current_visibility_constraints_mut()
            .add_atom(predicate_id);
        self.record_visibility_constraint_id(id);
        id
    }

    /// Records that all remaining statements in the current block are unreachable, and therefore
    /// not visible.
    fn mark_unreachable(&mut self) {
        self.current_use_def_map_mut().mark_unreachable();
    }

    /// Records a visibility constraint that always evaluates to "ambiguous".
    fn record_ambiguous_visibility(&mut self) {
        self.current_use_def_map_mut()
            .record_visibility_constraint(ScopedVisibilityConstraintId::AMBIGUOUS);
    }

    /// Simplifies (resets) visibility constraints on all live bindings and declarations that did
    /// not see any new definitions since the given snapshot.
    fn simplify_visibility_constraints(&mut self, snapshot: FlowSnapshot) {
        self.current_use_def_map_mut()
            .simplify_visibility_constraints(snapshot);
    }

    fn push_assignment(&mut self, assignment: CurrentAssignment<'db>) {
        self.current_assignments.push(assignment);
    }

    fn pop_assignment(&mut self) {
        let popped_assignment = self.current_assignments.pop();
        debug_assert!(popped_assignment.is_some());
    }

    fn current_assignment(&self) -> Option<CurrentAssignment<'db>> {
        self.current_assignments.last().copied()
    }

    fn current_assignment_mut(&mut self) -> Option<&mut CurrentAssignment<'db>> {
        self.current_assignments.last_mut()
    }

    /// Records the fact that we saw an attribute assignment of the form
    /// `object.attr: <annotation>( = …)` or `object.attr = <value>`.
    fn register_attribute_assignment(
        &mut self,
        object: &ast::Expr,
        attr: &'db ast::Identifier,
        attribute_assignment: AttributeAssignment<'db>,
    ) {
        if let Some(class_body_scope) = self.is_method_of_class() {
            // We only care about attribute assignments to the first parameter of a method,
            // i.e. typically `self` or `cls`.
            let accessed_object_refers_to_first_parameter =
                object.as_name_expr().map(|name| name.id.as_str())
                    == self.current_first_parameter_name;

            if accessed_object_refers_to_first_parameter {
                self.attribute_assignments
                    .entry(class_body_scope)
                    .or_default()
                    .entry(attr.id().clone())
                    .or_default()
                    .push(attribute_assignment);
            }
        }
    }

    fn predicate_kind(&mut self, pattern: &ast::Pattern) -> PatternPredicateKind<'db> {
        match pattern {
            ast::Pattern::MatchValue(pattern) => {
                let value = self.add_standalone_expression(&pattern.value);
                PatternPredicateKind::Value(value)
            }
            ast::Pattern::MatchSingleton(singleton) => {
                PatternPredicateKind::Singleton(singleton.value)
            }
            ast::Pattern::MatchClass(pattern) => {
                let cls = self.add_standalone_expression(&pattern.cls);
                PatternPredicateKind::Class(cls)
            }
            ast::Pattern::MatchOr(pattern) => {
                let predicates = pattern
                    .patterns
                    .iter()
                    .map(|pattern| self.predicate_kind(pattern))
                    .collect();
                PatternPredicateKind::Or(predicates)
            }
            _ => PatternPredicateKind::Unsupported,
        }
    }

    fn add_pattern_narrowing_constraint(
        &mut self,
        subject: Expression<'db>,
        pattern: &ast::Pattern,
        guard: Option<&ast::Expr>,
    ) -> Predicate<'db> {
        // This is called for the top-level pattern of each match arm. We need to create a
        // standalone expression for each arm of a match statement, since they can introduce
        // constraints on the match subject. (Or more accurately, for the match arm's pattern,
        // since its the pattern that introduces any constraints, not the body.) Ideally, that
        // standalone expression would wrap the match arm's pattern as a whole. But a standalone
        // expression can currently only wrap an ast::Expr, which patterns are not. So, we need to
        // choose an Expr that can “stand in” for the pattern, which we can wrap in a standalone
        // expression.
        //
        // See the comment in TypeInferenceBuilder::infer_match_pattern for more details.

        let kind = self.predicate_kind(pattern);
        let guard = guard.map(|guard| self.add_standalone_expression(guard));

        let pattern_predicate = PatternPredicate::new(
            self.db,
            self.file,
            self.current_scope(),
            subject,
            kind,
            guard,
            countme::Count::default(),
        );
        let predicate = Predicate {
            node: PredicateNode::Pattern(pattern_predicate),
            is_positive: true,
        };
        self.record_narrowing_constraint(predicate);
        predicate
    }

    /// Record an expression that needs to be a Salsa ingredient, because we need to infer its type
    /// standalone (type narrowing tests, RHS of an assignment.)
    fn add_standalone_expression(&mut self, expression_node: &ast::Expr) -> Expression<'db> {
        self.add_standalone_expression_impl(expression_node, ExpressionKind::Normal)
    }

    /// Same as [`SemanticIndexBuilder::add_standalone_expression`], but marks the expression as a
    /// *type* expression, which makes sure that it will later be inferred as such.
    fn add_standalone_type_expression(&mut self, expression_node: &ast::Expr) -> Expression<'db> {
        self.add_standalone_expression_impl(expression_node, ExpressionKind::TypeExpression)
    }

    fn add_standalone_expression_impl(
        &mut self,
        expression_node: &ast::Expr,
        expression_kind: ExpressionKind,
    ) -> Expression<'db> {
        let expression = Expression::new(
            self.db,
            self.file,
            self.current_scope(),
            #[allow(unsafe_code)]
            unsafe {
                AstNodeRef::new(self.module.clone(), expression_node)
            },
            expression_kind,
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
                self.mark_symbol_declared(symbol);
                if let Some(bounds) = bound {
                    self.visit_expr(bounds);
                }
                if let Some(default) = default {
                    self.visit_expr(default);
                }
                match type_param {
                    ast::TypeParam::TypeVar(node) => self.add_definition(symbol, node),
                    ast::TypeParam::ParamSpec(node) => self.add_definition(symbol, node),
                    ast::TypeParam::TypeVarTuple(node) => self.add_definition(symbol, node),
                };
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

        self.push_assignment(CurrentAssignment::Comprehension {
            node: generator,
            first: true,
        });
        self.visit_expr(&generator.target);
        self.pop_assignment();

        for expr in &generator.ifs {
            self.visit_expr(expr);
        }

        for generator in generators_iter {
            self.add_standalone_expression(&generator.iter);
            self.visit_expr(&generator.iter);

            self.push_assignment(CurrentAssignment::Comprehension {
                node: generator,
                first: false,
            });
            self.visit_expr(&generator.target);
            self.pop_assignment();

            for expr in &generator.ifs {
                self.visit_expr(expr);
            }
        }

        visit_outer_elt(self);
        self.pop_scope();
    }

    fn declare_parameters(&mut self, parameters: &'db ast::Parameters) {
        for parameter in parameters.iter_non_variadic_params() {
            self.declare_parameter(parameter);
        }
        if let Some(vararg) = parameters.vararg.as_ref() {
            let symbol = self.add_symbol(vararg.name.id().clone());
            self.add_definition(
                symbol,
                DefinitionNodeRef::VariadicPositionalParameter(vararg),
            );
        }
        if let Some(kwarg) = parameters.kwarg.as_ref() {
            let symbol = self.add_symbol(kwarg.name.id().clone());
            self.add_definition(symbol, DefinitionNodeRef::VariadicKeywordParameter(kwarg));
        }
    }

    fn declare_parameter(&mut self, parameter: &'db ast::ParameterWithDefault) {
        let symbol = self.add_symbol(parameter.name().id().clone());

        let definition = self.add_definition(symbol, parameter);

        // Insert a mapping from the inner Parameter node to the same definition. This
        // ensures that calling `HasType::inferred_type` on the inner parameter returns
        // a valid type (and doesn't panic)
        let existing_definition = self.definitions_by_node.insert(
            (&parameter.parameter).into(),
            Definitions::single(definition),
        );
        debug_assert_eq!(existing_definition, None);
    }

    /// Add an unpackable assignment for the given [`Unpackable`].
    ///
    /// This method handles assignments that can contain unpacking like assignment statements,
    /// for statements, etc.
    fn add_unpackable_assignment(
        &mut self,
        unpackable: &Unpackable<'db>,
        target: &'db ast::Expr,
        value: Expression<'db>,
    ) {
        // We only handle assignments to names and unpackings here, other targets like
        // attribute and subscript are handled separately as they don't create a new
        // definition.

        let current_assignment = match target {
            ast::Expr::List(_) | ast::Expr::Tuple(_) => {
                let unpack = Some(Unpack::new(
                    self.db,
                    self.file,
                    self.current_scope(),
                    // SAFETY: `target` belongs to the `self.module` tree
                    #[allow(unsafe_code)]
                    unsafe {
                        AstNodeRef::new(self.module.clone(), target)
                    },
                    UnpackValue::new(unpackable.kind(), value),
                    countme::Count::default(),
                ));
                Some(unpackable.as_current_assignment(unpack))
            }
            ast::Expr::Name(_) => Some(unpackable.as_current_assignment(None)),
            ast::Expr::Attribute(ast::ExprAttribute {
                value: object,
                attr,
                ..
            }) => {
                self.register_attribute_assignment(
                    object,
                    attr,
                    unpackable.as_attribute_assignment(value),
                );
                None
            }
            _ => None,
        };

        if let Some(current_assignment) = current_assignment {
            self.push_assignment(current_assignment);
        }

        self.visit_expr(target);

        if current_assignment.is_some() {
            // Only need to pop in the case where we pushed something
            self.pop_assignment();
        }
    }

    pub(super) fn build(mut self) -> SemanticIndex<'db> {
        let module = self.module;
        self.visit_body(module.suite());

        // Pop the root scope
        self.pop_scope();
        assert!(self.scope_stack.is_empty());

        assert_eq!(&self.current_assignments, &[]);

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
        self.eager_bindings.shrink_to_fit();

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
            imported_modules: Arc::new(self.imported_modules),
            has_future_annotations: self.has_future_annotations,
            attribute_assignments: self
                .attribute_assignments
                .into_iter()
                .map(|(k, v)| (k, Arc::new(v)))
                .collect(),
            eager_bindings: self.eager_bindings,
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
                let ast::StmtFunctionDef {
                    decorator_list,
                    parameters,
                    type_params,
                    name,
                    returns,
                    body,
                    is_async: _,
                    range: _,
                } = function_def;
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                self.with_type_params(
                    NodeWithScopeRef::FunctionTypeParameters(function_def),
                    type_params.as_deref(),
                    |builder| {
                        builder.visit_parameters(parameters);
                        if let Some(returns) = returns {
                            builder.visit_annotation(returns);
                        }

                        builder.push_scope(NodeWithScopeRef::Function(function_def));

                        builder.declare_parameters(parameters);

                        let mut first_parameter_name = parameters
                            .iter_non_variadic_params()
                            .next()
                            .map(|first_param| first_param.parameter.name.id().as_str());
                        std::mem::swap(
                            &mut builder.current_first_parameter_name,
                            &mut first_parameter_name,
                        );

                        // TODO: Fix how we determine the public types of symbols in a
                        // function-like scope: https://github.com/astral-sh/ruff/issues/15777
                        //
                        // In the meantime, visit the function body, but treat the last statement
                        // specially if it is a return. If it is, this would cause all definitions
                        // in the function to be marked as non-visible with our current treatment
                        // of terminal statements. Since we currently model the externally visible
                        // definitions in a function scope as the set of bindings that are visible
                        // at the end of the body, we then consider this function to have no
                        // externally visible definitions. To get around this, we take a flow
                        // snapshot just before processing the return statement, and use _that_ as
                        // the "end-of-body" state that we resolve external references against.
                        if let Some((last_stmt, first_stmts)) = body.split_last() {
                            builder.visit_body(first_stmts);
                            let pre_return_state = matches!(last_stmt, ast::Stmt::Return(_))
                                .then(|| builder.flow_snapshot());
                            builder.visit_stmt(last_stmt);
                            let scope_start_visibility =
                                builder.current_use_def_map().scope_start_visibility;
                            if let Some(pre_return_state) = pre_return_state {
                                builder.flow_restore(pre_return_state);
                                builder.current_use_def_map_mut().scope_start_visibility =
                                    scope_start_visibility;
                            }
                        }

                        builder.current_first_parameter_name = first_parameter_name;
                        builder.pop_scope()
                    },
                );
                // The default value of the parameters needs to be evaluated in the
                // enclosing scope.
                for default in parameters
                    .iter_non_variadic_params()
                    .filter_map(|param| param.default.as_deref())
                {
                    self.visit_expr(default);
                }
                // The symbol for the function name itself has to be evaluated
                // at the end to match the runtime evaluation of parameter defaults
                // and return-type annotations.
                let symbol = self.add_symbol(name.id.clone());
                self.add_definition(symbol, function_def);
            }
            ast::Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }

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

                // In Python runtime semantics, a class is registered after its scope is evaluated.
                let symbol = self.add_symbol(class.name.id.clone());
                self.add_definition(symbol, class);
            }
            ast::Stmt::TypeAlias(type_alias) => {
                let symbol = self.add_symbol(
                    type_alias
                        .name
                        .as_name_expr()
                        .map(|name| name.id.clone())
                        .unwrap_or("<unknown>".into()),
                );
                self.add_definition(symbol, type_alias);
                self.visit_expr(&type_alias.name);

                self.with_type_params(
                    NodeWithScopeRef::TypeAliasTypeParameters(type_alias),
                    type_alias.type_params.as_deref(),
                    |builder| {
                        builder.push_scope(NodeWithScopeRef::TypeAlias(type_alias));
                        builder.visit_expr(&type_alias.value);
                        builder.pop_scope()
                    },
                );
            }
            ast::Stmt::Import(node) => {
                for alias in &node.names {
                    // Mark the imported module, and all of its parents, as being imported in this
                    // file.
                    if let Some(module_name) = ModuleName::new(&alias.name) {
                        self.imported_modules.extend(module_name.ancestors());
                    }

                    let (symbol_name, is_reexported) = if let Some(asname) = &alias.asname {
                        (asname.id.clone(), asname.id == alias.name.id)
                    } else {
                        (Name::new(alias.name.id.split('.').next().unwrap()), false)
                    };

                    let symbol = self.add_symbol(symbol_name);
                    self.add_definition(
                        symbol,
                        ImportDefinitionNodeRef {
                            alias,
                            is_reexported,
                        },
                    );
                }
            }
            ast::Stmt::ImportFrom(node) => {
                let mut found_star = false;
                for (alias_index, alias) in node.names.iter().enumerate() {
                    if &alias.name == "*" {
                        // The following line maintains the invariant that every AST node that
                        // implements `Into<DefinitionNodeKey>` must have an entry in the
                        // `definitions_by_node` map. Maintaining this invariant ensures that
                        // `SemanticIndex::definitions` can always look up the definitions for a
                        // given AST node without panicking.
                        //
                        // The reason why maintaining this invariant requires special handling here
                        // is that some `Alias` nodes may be associated with 0 definitions:
                        // - If the import statement has invalid syntax: multiple `*` names in the `names` list
                        //   (e.g. `from foo import *, bar, *`)
                        // - If the `*` import refers to a module that has 0 exported names.
                        // - If the module being imported from cannot be resolved.
                        self.add_entry_for_definition_key(alias.into());

                        if found_star {
                            continue;
                        }

                        found_star = true;

                        // Wildcard imports are invalid syntax everywhere except the top-level scope,
                        // and thus do not bind any definitions anywhere else
                        if !self.current_scope_is_global_scope() {
                            continue;
                        }

                        let Ok(module_name) =
                            ModuleName::from_import_statement(self.db, self.file, node)
                        else {
                            continue;
                        };

                        let Some(module) = resolve_module(self.db, &module_name) else {
                            continue;
                        };

                        for export in exported_names(self.db, module.file()) {
                            let symbol_id = self.add_symbol(export.clone());
                            let node_ref = StarImportDefinitionNodeRef { node, symbol_id };
                            self.push_additional_definition(symbol_id, node_ref);
                        }

                        continue;
                    }

                    let (symbol_name, is_reexported) = if let Some(asname) = &alias.asname {
                        (&asname.id, asname.id == alias.name.id)
                    } else {
                        (&alias.name.id, false)
                    };

                    // Look for imports `from __future__ import annotations`, ignore `as ...`
                    // We intentionally don't enforce the rules about location of `__future__`
                    // imports here, we assume the user's intent was to apply the `__future__`
                    // import, so we still check using it (and will also emit a diagnostic about a
                    // miss-placed `__future__` import.)
                    self.has_future_annotations |= alias.name.id == "annotations"
                        && node.module.as_deref() == Some("__future__");

                    let symbol = self.add_symbol(symbol_name.clone());

                    self.add_definition(
                        symbol,
                        ImportFromDefinitionNodeRef {
                            node,
                            alias_index,
                            is_reexported,
                        },
                    );
                }
            }
            ast::Stmt::Assign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);

                self.visit_expr(&node.value);
                let value = self.add_standalone_expression(&node.value);

                for target in &node.targets {
                    self.add_unpackable_assignment(&Unpackable::Assign(node), target, value);
                }
            }
            ast::Stmt::AnnAssign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);
                self.visit_expr(&node.annotation);
                let annotation = self.add_standalone_type_expression(&node.annotation);
                if let Some(value) = &node.value {
                    self.visit_expr(value);
                }

                // See https://docs.python.org/3/library/ast.html#ast.AnnAssign
                if matches!(
                    *node.target,
                    ast::Expr::Attribute(_) | ast::Expr::Subscript(_) | ast::Expr::Name(_)
                ) {
                    self.push_assignment(node.into());
                    self.visit_expr(&node.target);

                    if let ast::Expr::Attribute(ast::ExprAttribute {
                        value: object,
                        attr,
                        ..
                    }) = &*node.target
                    {
                        self.register_attribute_assignment(
                            object,
                            attr,
                            AttributeAssignment::Annotated { annotation },
                        );
                    }

                    self.pop_assignment();
                } else {
                    self.visit_expr(&node.target);
                }
            }
            ast::Stmt::AugAssign(
                aug_assign @ ast::StmtAugAssign {
                    range: _,
                    target,
                    op: _,
                    value,
                },
            ) => {
                debug_assert_eq!(&self.current_assignments, &[]);
                self.visit_expr(value);

                // See https://docs.python.org/3/library/ast.html#ast.AugAssign
                if matches!(
                    **target,
                    ast::Expr::Attribute(_) | ast::Expr::Subscript(_) | ast::Expr::Name(_)
                ) {
                    self.push_assignment(aug_assign.into());
                    self.visit_expr(target);
                    self.pop_assignment();
                } else {
                    self.visit_expr(target);
                }
            }
            ast::Stmt::If(node) => {
                self.visit_expr(&node.test);
                let mut no_branch_taken = self.flow_snapshot();
                let mut last_predicate = self.record_expression_narrowing_constraint(&node.test);
                self.visit_body(&node.body);

                let visibility_constraint_id = self.record_visibility_constraint(last_predicate);
                let mut vis_constraints = vec![visibility_constraint_id];

                let mut post_clauses: Vec<FlowSnapshot> = vec![];
                let elif_else_clauses = node
                    .elif_else_clauses
                    .iter()
                    .map(|clause| (clause.test.as_ref(), clause.body.as_slice()));
                let has_else = node
                    .elif_else_clauses
                    .last()
                    .is_some_and(|clause| clause.test.is_none());
                let elif_else_clauses = elif_else_clauses.chain(if has_else {
                    // if there's an `else` clause already, we don't need to add another
                    None
                } else {
                    // if there's no `else` branch, we should add a no-op `else` branch
                    Some((None, Default::default()))
                });
                for (clause_test, clause_body) in elif_else_clauses {
                    // snapshot after every block except the last; the last one will just become
                    // the state that we merge the other snapshots into
                    post_clauses.push(self.flow_snapshot());
                    // we can only take an elif/else branch if none of the previous ones were
                    // taken
                    self.flow_restore(no_branch_taken.clone());
                    self.record_negated_narrowing_constraint(last_predicate);

                    let elif_predicate = if let Some(elif_test) = clause_test {
                        self.visit_expr(elif_test);
                        // A test expression is evaluated whether the branch is taken or not
                        no_branch_taken = self.flow_snapshot();
                        let predicate = self.record_expression_narrowing_constraint(elif_test);
                        Some(predicate)
                    } else {
                        None
                    };

                    self.visit_body(clause_body);

                    for id in &vis_constraints {
                        self.record_negated_visibility_constraint(*id);
                    }
                    if let Some(elif_predicate) = elif_predicate {
                        last_predicate = elif_predicate;
                        let id = self.record_visibility_constraint(elif_predicate);
                        vis_constraints.push(id);
                    }
                }

                for post_clause_state in post_clauses {
                    self.flow_merge(post_clause_state);
                }

                self.simplify_visibility_constraints(no_branch_taken);
            }
            ast::Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _,
            }) => {
                self.visit_expr(test);

                let pre_loop = self.flow_snapshot();
                let predicate = self.record_expression_narrowing_constraint(test);

                // We need multiple copies of the visibility constraint for the while condition,
                // since we need to model situations where the first evaluation of the condition
                // returns True, but a later evaluation returns False.
                let first_predicate_id = self.current_use_def_map_mut().add_predicate(predicate);
                let later_predicate_id = self.current_use_def_map_mut().add_predicate(predicate);
                let first_vis_constraint_id = self
                    .current_visibility_constraints_mut()
                    .add_atom(first_predicate_id);
                let later_vis_constraint_id = self
                    .current_visibility_constraints_mut()
                    .add_atom(later_predicate_id);

                let outer_loop = self.push_loop();
                self.visit_body(body);
                let this_loop = self.pop_loop(outer_loop);

                // If the body is executed, we know that we've evaluated the condition at least
                // once, and that the first evaluation was True. We might not have evaluated the
                // condition more than once, so we can't assume that later evaluations were True.
                // So the body's full visibility constraint is `first`.
                let body_vis_constraint_id = first_vis_constraint_id;
                self.record_visibility_constraint_id(body_vis_constraint_id);

                // We execute the `else` once the condition evaluates to false. This could happen
                // without ever executing the body, if the condition is false the first time it's
                // tested. So the starting flow state of the `else` clause is the union of:
                //   - the pre-loop state with a visibility constraint that the first evaluation of
                //     the while condition was false,
                //   - the post-body state (which already has a visibility constraint that the
                //     first evaluation was true) with a visibility constraint that a _later_
                //     evaluation of the while condition was false.
                // To model this correctly, we need two copies of the while condition constraint,
                // since the first and later evaluations might produce different results.
                let post_body = self.flow_snapshot();
                self.flow_restore(pre_loop.clone());
                self.record_negated_visibility_constraint(first_vis_constraint_id);
                self.flow_merge(post_body);
                self.record_negated_narrowing_constraint(predicate);
                self.visit_body(orelse);
                self.record_negated_visibility_constraint(later_vis_constraint_id);

                // Breaking out of a while loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                for break_state in this_loop.break_states {
                    let snapshot = self.flow_snapshot();
                    self.flow_restore(break_state);
                    self.record_visibility_constraint_id(body_vis_constraint_id);
                    self.flow_merge(snapshot);
                }

                self.simplify_visibility_constraints(pre_loop);
            }
            ast::Stmt::With(ast::StmtWith {
                items,
                body,
                is_async,
                ..
            }) => {
                for item @ ast::WithItem {
                    range: _,
                    context_expr,
                    optional_vars,
                } in items
                {
                    self.visit_expr(context_expr);
                    if let Some(optional_vars) = optional_vars.as_deref() {
                        let context_manager = self.add_standalone_expression(context_expr);
                        self.add_unpackable_assignment(
                            &Unpackable::WithItem {
                                item,
                                is_async: *is_async,
                            },
                            optional_vars,
                            context_manager,
                        );
                    }
                }
                self.visit_body(body);
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
                debug_assert_eq!(&self.current_assignments, &[]);

                let iter_expr = self.add_standalone_expression(iter);
                self.visit_expr(iter);

                self.record_ambiguous_visibility();

                let pre_loop = self.flow_snapshot();

                self.add_unpackable_assignment(&Unpackable::For(for_stmt), target, iter_expr);

                let outer_loop = self.push_loop();
                self.visit_body(body);
                let this_loop = self.pop_loop(outer_loop);

                // We may execute the `else` clause without ever executing the body, so merge in
                // the pre-loop state before visiting `else`.
                self.flow_merge(pre_loop);
                self.visit_body(orelse);

                // Breaking out of a `for` loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                for break_state in this_loop.break_states {
                    self.flow_merge(break_state);
                }
            }
            ast::Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range: _,
            }) => {
                debug_assert_eq!(self.current_match_case, None);

                let subject_expr = self.add_standalone_expression(subject);
                self.visit_expr(subject);
                if cases.is_empty() {
                    return;
                }

                let after_subject = self.flow_snapshot();
                let mut vis_constraints = vec![];
                let mut post_case_snapshots = vec![];
                for (i, case) in cases.iter().enumerate() {
                    if i != 0 {
                        post_case_snapshots.push(self.flow_snapshot());
                        self.flow_restore(after_subject.clone());
                    }

                    self.current_match_case = Some(CurrentMatchCase::new(&case.pattern));
                    self.visit_pattern(&case.pattern);
                    self.current_match_case = None;
                    let predicate = self.add_pattern_narrowing_constraint(
                        subject_expr,
                        &case.pattern,
                        case.guard.as_deref(),
                    );
                    if let Some(expr) = &case.guard {
                        self.visit_expr(expr);
                    }
                    self.visit_body(&case.body);
                    for id in &vis_constraints {
                        self.record_negated_visibility_constraint(*id);
                    }
                    let vis_constraint_id = self.record_visibility_constraint(predicate);
                    vis_constraints.push(vis_constraint_id);
                }

                // If there is no final wildcard match case, pretend there is one. This is similar to how
                // we add an implicit `else` block in if-elif chains, in case it's not present.
                if !cases
                    .last()
                    .is_some_and(|case| case.guard.is_none() && case.pattern.is_wildcard())
                {
                    post_case_snapshots.push(self.flow_snapshot());
                    self.flow_restore(after_subject.clone());

                    for id in &vis_constraints {
                        self.record_negated_visibility_constraint(*id);
                    }
                }

                for post_clause_state in post_case_snapshots {
                    self.flow_merge(post_clause_state);
                }

                self.simplify_visibility_constraints(after_subject);
            }
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range: _,
            }) => {
                self.record_ambiguous_visibility();

                // Save the state prior to visiting any of the `try` block.
                //
                // Potentially none of the `try` block could have been executed prior to executing
                // the `except` block(s) and/or the `finally` block.
                // We will merge this state with all of the intermediate
                // states during the `try` block before visiting those suites.
                let pre_try_block_state = self.flow_snapshot();

                self.try_node_context_stack_manager.push_context();

                // Visit the `try` block!
                self.visit_body(body);

                let mut post_except_states = vec![];

                // Take a record also of all the intermediate states we encountered
                // while visiting the `try` block
                let try_block_snapshots = self.try_node_context_stack_manager.pop_context();

                if !handlers.is_empty() {
                    // Save the state immediately *after* visiting the `try` block
                    // but *before* we prepare for visiting the `except` block(s).
                    //
                    // We will revert to this state prior to visiting the the `else` block,
                    // as there necessarily must have been 0 `except` blocks executed
                    // if we hit the `else` block.
                    let post_try_block_state = self.flow_snapshot();

                    // Prepare for visiting the `except` block(s)
                    self.flow_restore(pre_try_block_state);
                    for state in try_block_snapshots {
                        self.flow_merge(state);
                    }

                    let pre_except_state = self.flow_snapshot();
                    let num_handlers = handlers.len();

                    for (i, except_handler) in handlers.iter().enumerate() {
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
                        // Each `except` block is mutually exclusive with all other `except` blocks.
                        post_except_states.push(self.flow_snapshot());

                        // It's unnecessary to do the `self.flow_restore()` call for the final except handler,
                        // as we'll immediately call `self.flow_restore()` to a different state
                        // as soon as this loop over the handlers terminates.
                        if i < (num_handlers - 1) {
                            self.flow_restore(pre_except_state.clone());
                        }
                    }

                    // If we get to the `else` block, we know that 0 of the `except` blocks can have been executed,
                    // and the entire `try` block must have been executed:
                    self.flow_restore(post_try_block_state);
                }

                self.visit_body(orelse);

                for post_except_state in post_except_states {
                    self.flow_merge(post_except_state);
                }

                // TODO: there's lots of complexity here that isn't yet handled by our model.
                // In order to accurately model the semantics of `finally` suites, we in fact need to visit
                // the suite twice: once under the (current) assumption that either the `try + else` suite
                // ran to completion or exactly one `except` branch ran to completion, and then again under
                // the assumption that potentially none of the branches ran to completion and we in fact
                // jumped from a `try`, `else` or `except` branch straight into the `finally` branch.
                // This requires rethinking some fundamental assumptions semantic indexing makes.
                // For more details, see:
                // - https://astral-sh.notion.site/Exception-handler-control-flow-11348797e1ca80bb8ce1e9aedbbe439d
                // - https://github.com/astral-sh/ruff/pull/13633#discussion_r1788626702
                self.visit_body(finalbody);
            }

            ast::Stmt::Raise(_) | ast::Stmt::Return(_) | ast::Stmt::Continue(_) => {
                walk_stmt(self, stmt);
                // Everything in the current block after a terminal statement is unreachable.
                self.mark_unreachable();
            }

            ast::Stmt::Break(_) => {
                let snapshot = self.flow_snapshot();
                if let Some(current_loop) = self.current_loop_mut() {
                    current_loop.push_break(snapshot);
                }
                // Everything in the current block after a terminal statement is unreachable.
                self.mark_unreachable();
            }

            _ => {
                walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        self.scopes_by_expression
            .insert(expr.into(), self.current_scope());
        let expression_id = self.current_ast_ids().record_expression(expr);

        match expr {
            ast::Expr::Name(name_node @ ast::ExprName { id, ctx, .. }) => {
                let (is_use, is_definition) = match (ctx, self.current_assignment()) {
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

                if is_use {
                    self.mark_symbol_used(symbol);
                    let use_id = self.current_ast_ids().record_use(expr);
                    self.current_use_def_map_mut().record_use(symbol, use_id);
                }

                if is_definition {
                    match self.current_assignment() {
                        Some(CurrentAssignment::Assign { node, unpack }) => {
                            self.add_definition(
                                symbol,
                                AssignmentDefinitionNodeRef {
                                    unpack,
                                    value: &node.value,
                                    name: name_node,
                                },
                            );
                        }
                        Some(CurrentAssignment::AnnAssign(ann_assign)) => {
                            self.add_definition(symbol, ann_assign);
                        }
                        Some(CurrentAssignment::AugAssign(aug_assign)) => {
                            self.add_definition(symbol, aug_assign);
                        }
                        Some(CurrentAssignment::For { node, unpack }) => {
                            self.add_definition(
                                symbol,
                                ForStmtDefinitionNodeRef {
                                    unpack,
                                    iterable: &node.iter,
                                    name: name_node,
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
                        Some(CurrentAssignment::WithItem {
                            item,
                            is_async,
                            unpack,
                        }) => {
                            self.add_definition(
                                symbol,
                                WithItemDefinitionNodeRef {
                                    unpack,
                                    context_expr: &item.context_expr,
                                    name: name_node,
                                    is_async,
                                },
                            );
                        }
                        None => {}
                    }
                }

                if let Some(unpack_position) = self
                    .current_assignment_mut()
                    .and_then(CurrentAssignment::unpack_position_mut)
                {
                    *unpack_position = UnpackPosition::Other;
                }

                walk_expr(self, expr);
            }
            ast::Expr::Named(node) => {
                // TODO walrus in comprehensions is implicitly nonlocal
                self.visit_expr(&node.value);

                // See https://peps.python.org/pep-0572/#differences-between-assignment-expressions-and-assignment-statements
                if node.target.is_name_expr() {
                    self.push_assignment(node.into());
                    self.visit_expr(&node.target);
                    self.pop_assignment();
                } else {
                    self.visit_expr(&node.target);
                }
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
                if let Some(parameters) = lambda.parameters.as_ref() {
                    self.declare_parameters(parameters);
                }

                self.visit_expr(lambda.body.as_ref());
                self.pop_scope();
            }
            ast::Expr::If(ast::ExprIf {
                body, test, orelse, ..
            }) => {
                self.visit_expr(test);
                let pre_if = self.flow_snapshot();
                let predicate = self.record_expression_narrowing_constraint(test);
                self.visit_expr(body);
                let visibility_constraint = self.record_visibility_constraint(predicate);
                let post_body = self.flow_snapshot();
                self.flow_restore(pre_if.clone());

                self.record_negated_narrowing_constraint(predicate);
                self.visit_expr(orelse);
                self.record_negated_visibility_constraint(visibility_constraint);
                self.flow_merge(post_body);
                self.simplify_visibility_constraints(pre_if);
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
            ast::Expr::BoolOp(ast::ExprBoolOp {
                values,
                range: _,
                op,
            }) => {
                let pre_op = self.flow_snapshot();

                let mut snapshots = vec![];
                let mut visibility_constraints = vec![];

                for (index, value) in values.iter().enumerate() {
                    self.visit_expr(value);

                    for vid in &visibility_constraints {
                        self.record_visibility_constraint_id(*vid);
                    }

                    // For the last value, we don't need to model control flow. There is short-circuiting
                    // anymore.
                    if index < values.len() - 1 {
                        let predicate = self.build_predicate(value);
                        let predicate_id = match op {
                            ast::BoolOp::And => self.add_predicate(predicate),
                            ast::BoolOp::Or => self.add_negated_predicate(predicate),
                        };
                        let visibility_constraint = self
                            .current_visibility_constraints_mut()
                            .add_atom(predicate_id);

                        let after_expr = self.flow_snapshot();

                        // We first model the short-circuiting behavior. We take the short-circuit
                        // path here if all of the previous short-circuit paths were not taken, so
                        // we record all previously existing visibility constraints, and negate the
                        // one for the current expression.
                        for vid in &visibility_constraints {
                            self.record_visibility_constraint_id(*vid);
                        }
                        self.record_negated_visibility_constraint(visibility_constraint);
                        snapshots.push(self.flow_snapshot());

                        // Then we model the non-short-circuiting behavior. Here, we need to delay
                        // the application of the visibility constraint until after the expression
                        // has been evaluated, so we only push it onto the stack here.
                        self.flow_restore(after_expr);
                        self.record_narrowing_constraint_id(predicate_id);
                        visibility_constraints.push(visibility_constraint);
                    }
                }

                for snapshot in snapshots {
                    self.flow_merge(snapshot);
                }

                self.simplify_visibility_constraints(pre_op);
            }
            ast::Expr::Attribute(ast::ExprAttribute {
                value: object,
                attr,
                ctx: ExprContext::Store,
                range: _,
            }) => {
                if let Some(unpack) = self
                    .current_assignment()
                    .as_ref()
                    .and_then(CurrentAssignment::unpack)
                {
                    self.register_attribute_assignment(
                        object,
                        attr,
                        AttributeAssignment::Unpack {
                            attribute_expression_id: expression_id,
                            unpack,
                        },
                    );
                }

                walk_expr(self, expr);
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

#[derive(Copy, Clone, Debug, PartialEq)]
enum CurrentAssignment<'a> {
    Assign {
        node: &'a ast::StmtAssign,
        unpack: Option<(UnpackPosition, Unpack<'a>)>,
    },
    AnnAssign(&'a ast::StmtAnnAssign),
    AugAssign(&'a ast::StmtAugAssign),
    For {
        node: &'a ast::StmtFor,
        unpack: Option<(UnpackPosition, Unpack<'a>)>,
    },
    Named(&'a ast::ExprNamed),
    Comprehension {
        node: &'a ast::Comprehension,
        first: bool,
    },
    WithItem {
        item: &'a ast::WithItem,
        is_async: bool,
        unpack: Option<(UnpackPosition, Unpack<'a>)>,
    },
}

impl<'a> CurrentAssignment<'a> {
    fn unpack(&self) -> Option<Unpack<'a>> {
        match self {
            Self::Assign { unpack, .. }
            | Self::For { unpack, .. }
            | Self::WithItem { unpack, .. } => unpack.map(|(_, unpack)| unpack),
            Self::AnnAssign(_)
            | Self::AugAssign(_)
            | Self::Named(_)
            | Self::Comprehension { .. } => None,
        }
    }

    fn unpack_position_mut(&mut self) -> Option<&mut UnpackPosition> {
        match self {
            Self::Assign { unpack, .. }
            | Self::For { unpack, .. }
            | Self::WithItem { unpack, .. } => unpack.as_mut().map(|(position, _)| position),
            Self::AnnAssign(_)
            | Self::AugAssign(_)
            | Self::Named(_)
            | Self::Comprehension { .. } => None,
        }
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

impl<'a> From<&'a ast::ExprNamed> for CurrentAssignment<'a> {
    fn from(value: &'a ast::ExprNamed) -> Self {
        Self::Named(value)
    }
}

#[derive(Debug, PartialEq)]
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

enum Unpackable<'a> {
    Assign(&'a ast::StmtAssign),
    For(&'a ast::StmtFor),
    WithItem {
        item: &'a ast::WithItem,
        is_async: bool,
    },
}

impl<'a> Unpackable<'a> {
    const fn kind(&self) -> UnpackKind {
        match self {
            Unpackable::Assign(_) => UnpackKind::Assign,
            Unpackable::For(_) => UnpackKind::Iterable,
            Unpackable::WithItem { .. } => UnpackKind::ContextManager,
        }
    }

    fn as_current_assignment(&self, unpack: Option<Unpack<'a>>) -> CurrentAssignment<'a> {
        let unpack = unpack.map(|unpack| (UnpackPosition::First, unpack));
        match self {
            Unpackable::Assign(stmt) => CurrentAssignment::Assign { node: stmt, unpack },
            Unpackable::For(stmt) => CurrentAssignment::For { node: stmt, unpack },
            Unpackable::WithItem { item, is_async } => CurrentAssignment::WithItem {
                item,
                is_async: *is_async,
                unpack,
            },
        }
    }

    fn as_attribute_assignment(&self, expression: Expression<'a>) -> AttributeAssignment<'a> {
        match self {
            Unpackable::Assign(_) => AttributeAssignment::Unannotated { value: expression },
            Unpackable::For(_) => AttributeAssignment::Iterable {
                iterable: expression,
            },
            Unpackable::WithItem { .. } => AttributeAssignment::ContextManager {
                context_manager: expression,
            },
        }
    }
}
