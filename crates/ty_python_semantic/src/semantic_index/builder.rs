use std::cell::{OnceCell, RefCell};
use std::sync::Arc;

use except_handlers::TryNodeContextStackManager;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_db::source::{SourceText, source_text};
use ruff_index::IndexVec;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_pattern, walk_stmt};
use ruff_python_ast::{self as ast, PySourceType, PythonVersion};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxChecker, SemanticSyntaxContext, SemanticSyntaxError, SemanticSyntaxErrorKind,
};
use ruff_text_size::TextRange;

use crate::ast_node_ref::AstNodeRef;
use crate::module_name::ModuleName;
use crate::module_resolver::resolve_module;
use crate::node_key::NodeKey;
use crate::semantic_index::SemanticIndex;
use crate::semantic_index::ast_ids::AstIdsBuilder;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::definition::{
    AnnotatedAssignmentDefinitionKind, AnnotatedAssignmentDefinitionNodeRef,
    AssignmentDefinitionKind, AssignmentDefinitionNodeRef, ComprehensionDefinitionKind,
    ComprehensionDefinitionNodeRef, Definition, DefinitionCategory, DefinitionKind,
    DefinitionNodeKey, DefinitionNodeRef, Definitions, ExceptHandlerDefinitionNodeRef,
    ForStmtDefinitionKind, ForStmtDefinitionNodeRef, ImportDefinitionNodeRef,
    ImportFromDefinitionNodeRef, MatchPatternDefinitionNodeRef, StarImportDefinitionNodeRef,
    TargetKind, WithItemDefinitionKind, WithItemDefinitionNodeRef,
};
use crate::semantic_index::expression::{Expression, ExpressionKind};
use crate::semantic_index::predicate::{
    PatternPredicate, PatternPredicateKind, Predicate, PredicateNode, ScopedPredicateId,
    StarImportPlaceholderPredicate,
};
use crate::semantic_index::re_exports::exported_names;
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeKind, NodeWithScopeRef, Scope, ScopeId, ScopeKind,
    ScopedSymbolId, SymbolTableBuilder,
};
use crate::semantic_index::use_def::{
    EagerSnapshotKey, FlowSnapshot, ScopedEagerSnapshotId, UseDefMapBuilder,
};
use crate::semantic_index::visibility_constraints::{
    ScopedVisibilityConstraintId, VisibilityConstraintsBuilder,
};
use crate::unpack::{Unpack, UnpackKind, UnpackPosition, UnpackValue};
use crate::{Db, Program};

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
    source_type: PySourceType,
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

    // Used for checking semantic syntax errors
    python_version: PythonVersion,
    source_text: OnceCell<SourceText>,
    semantic_checker: SemanticSyntaxChecker,

    // Semantic Index fields
    scopes: IndexVec<FileScopeId, Scope>,
    scope_ids_by_scope: IndexVec<FileScopeId, ScopeId<'db>>,
    symbol_tables: IndexVec<FileScopeId, SymbolTableBuilder>,
    instance_attribute_tables: IndexVec<FileScopeId, SymbolTableBuilder>,
    ast_ids: IndexVec<FileScopeId, AstIdsBuilder>,
    use_def_maps: IndexVec<FileScopeId, UseDefMapBuilder<'db>>,
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,
    scopes_by_expression: FxHashMap<ExpressionNodeKey, FileScopeId>,
    globals_by_scope: FxHashMap<FileScopeId, FxHashSet<ScopedSymbolId>>,
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definitions<'db>>,
    expressions_by_node: FxHashMap<ExpressionNodeKey, Expression<'db>>,
    imported_modules: FxHashSet<ModuleName>,
    /// Hashset of all [`FileScopeId`]s that correspond to [generator functions].
    ///
    /// [generator functions]: https://docs.python.org/3/glossary.html#term-generator
    generator_functions: FxHashSet<FileScopeId>,
    eager_snapshots: FxHashMap<EagerSnapshotKey, ScopedEagerSnapshotId>,
    /// Errors collected by the `semantic_checker`.
    semantic_syntax_errors: RefCell<Vec<SemanticSyntaxError>>,
}

impl<'db> SemanticIndexBuilder<'db> {
    pub(super) fn new(db: &'db dyn Db, file: File, parsed: &'db ParsedModule) -> Self {
        let mut builder = Self {
            db,
            file,
            source_type: file.source_type(db.upcast()),
            module: parsed,
            scope_stack: Vec::new(),
            current_assignments: vec![],
            current_match_case: None,
            current_first_parameter_name: None,
            try_node_context_stack_manager: TryNodeContextStackManager::default(),

            has_future_annotations: false,

            scopes: IndexVec::new(),
            symbol_tables: IndexVec::new(),
            instance_attribute_tables: IndexVec::new(),
            ast_ids: IndexVec::new(),
            scope_ids_by_scope: IndexVec::new(),
            use_def_maps: IndexVec::new(),

            scopes_by_expression: FxHashMap::default(),
            scopes_by_node: FxHashMap::default(),
            definitions_by_node: FxHashMap::default(),
            expressions_by_node: FxHashMap::default(),
            globals_by_scope: FxHashMap::default(),

            imported_modules: FxHashSet::default(),
            generator_functions: FxHashSet::default(),

            eager_snapshots: FxHashMap::default(),

            python_version: Program::get(db).python_version(db),
            source_text: OnceCell::new(),
            semantic_checker: SemanticSyntaxChecker::default(),
            semantic_syntax_errors: RefCell::default(),
        };

        builder.push_scope_with_parent(
            NodeWithScopeRef::Module,
            None,
            ScopedVisibilityConstraintId::ALWAYS_TRUE,
        );

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

    /// Returns the scope ID of the surrounding class body scope if the current scope
    /// is a method inside a class body. Returns `None` otherwise, e.g. if the current
    /// scope is a function body outside of a class, or if the current scope is not a
    /// function body.
    fn is_method_of_class(&self) -> Option<FileScopeId> {
        let mut scopes_rev = self.scope_stack.iter().rev();
        let current = scopes_rev.next()?;

        if self.scopes[current.file_scope_id].kind() != ScopeKind::Function {
            return None;
        }

        let parent = scopes_rev.next()?;

        match self.scopes[parent.file_scope_id].kind() {
            ScopeKind::Class => Some(parent.file_scope_id),
            ScopeKind::Annotation => {
                // If the function is generic, the parent scope is an annotation scope.
                // In this case, we need to go up one level higher to find the class scope.
                let grandparent = scopes_rev.next()?;

                if self.scopes[grandparent.file_scope_id].kind() == ScopeKind::Class {
                    Some(grandparent.file_scope_id)
                } else {
                    None
                }
            }
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
        let reachability = self.current_use_def_map().reachability;
        self.push_scope_with_parent(node, Some(parent), reachability);
    }

    fn push_scope_with_parent(
        &mut self,
        node: NodeWithScopeRef,
        parent: Option<FileScopeId>,
        reachability: ScopedVisibilityConstraintId,
    ) {
        let children_start = self.scopes.next_index() + 1;

        // SAFETY: `node` is guaranteed to be a child of `self.module`
        #[expect(unsafe_code)]
        let node_with_kind = unsafe { node.to_kind(self.module.clone()) };

        let scope = Scope::new(
            parent,
            node_with_kind,
            children_start..children_start,
            reachability,
        );
        let is_class_scope = scope.kind().is_class();
        self.try_node_context_stack_manager.enter_nested_scope();

        let file_scope_id = self.scopes.push(scope);
        self.symbol_tables.push(SymbolTableBuilder::default());
        self.instance_attribute_tables
            .push(SymbolTableBuilder::default());
        self.use_def_maps
            .push(UseDefMapBuilder::new(is_class_scope));
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

                // Snapshot the state of this symbol that are visible at this point in this
                // enclosing scope.
                let key = EagerSnapshotKey {
                    enclosing_scope: enclosing_scope_id,
                    enclosing_symbol: enclosing_symbol_id,
                    nested_scope: popped_scope_id,
                };
                let eager_snapshot = self.use_def_maps[enclosing_scope_id].snapshot_eager_state(
                    enclosing_symbol_id,
                    enclosing_scope_kind,
                    enclosing_symbol.is_bound(),
                );
                self.eager_snapshots.insert(key, eager_snapshot);
            }

            // Lazy scopes are "sticky": once we see a lazy scope we stop doing lookups
            // eagerly, even if we would encounter another eager enclosing scope later on.
            // Also, narrowing constraints outside a lazy scope are not applicable.
            // TODO: If the symbol has never been rewritten, they are applicable.
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

    fn current_attribute_table(&mut self) -> &mut SymbolTableBuilder {
        let scope_id = self.current_scope();
        &mut self.instance_attribute_tables[scope_id]
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

    /// Add a symbol to the symbol table and the use-def map.
    /// Return the [`ScopedSymbolId`] that uniquely identifies the symbol in both.
    fn add_symbol(&mut self, name: Name) -> ScopedSymbolId {
        let (symbol_id, added) = self.current_symbol_table().add_symbol(name);
        if added {
            self.current_use_def_map_mut().add_symbol(symbol_id);
        }
        symbol_id
    }

    fn add_attribute(&mut self, name: Name) -> ScopedSymbolId {
        let (symbol_id, added) = self.current_attribute_table().add_symbol(name);
        if added {
            self.current_use_def_map_mut().add_attribute(symbol_id);
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
            num_definitions, 1,
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
        #[expect(unsafe_code)]
        // SAFETY: `definition_node` is guaranteed to be a child of `self.module`
        let kind = unsafe { definition_node.into_owned(self.module.clone()) };
        let category = kind.category(self.source_type.is_stub());
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

    fn add_attribute_definition(
        &mut self,
        symbol: ScopedSymbolId,
        definition_kind: DefinitionKind<'db>,
    ) -> Definition {
        let definition = Definition::new(
            self.db,
            self.file,
            self.current_scope(),
            symbol,
            definition_kind,
            false,
            countme::Count::default(),
        );
        self.current_use_def_map_mut()
            .record_attribute_binding(symbol, definition);
        definition
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
        self.current_use_def_map_mut()
            .add_predicate(predicate.negated())
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

    /// Record a constraint that affects the reachability of the current position in the semantic
    /// index analysis. For example, if we encounter a `if test:` branch, we immediately record
    /// a `test` constraint, because if `test` later (during type checking) evaluates to `False`,
    /// we know that all statements that follow in this path of control flow will be unreachable.
    fn record_reachability_constraint(
        &mut self,
        predicate: Predicate<'db>,
    ) -> ScopedVisibilityConstraintId {
        let predicate_id = self.add_predicate(predicate);
        self.record_reachability_constraint_id(predicate_id)
    }

    /// Similar to [`Self::record_reachability_constraint`], but takes a [`ScopedPredicateId`].
    fn record_reachability_constraint_id(
        &mut self,
        predicate_id: ScopedPredicateId,
    ) -> ScopedVisibilityConstraintId {
        let visibility_constraint = self
            .current_visibility_constraints_mut()
            .add_atom(predicate_id);
        self.current_use_def_map_mut()
            .record_reachability_constraint(visibility_constraint);
        visibility_constraint
    }

    /// Record the negation of a given reachability/visibility constraint.
    fn record_negated_reachability_constraint(
        &mut self,
        reachability_constraint: ScopedVisibilityConstraintId,
    ) {
        let negated_constraint = self
            .current_visibility_constraints_mut()
            .add_not_constraint(reachability_constraint);
        self.current_use_def_map_mut()
            .record_reachability_constraint(negated_constraint);
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
        definition_kind: DefinitionKind<'db>,
    ) {
        if self.is_method_of_class().is_some() {
            // We only care about attribute assignments to the first parameter of a method,
            // i.e. typically `self` or `cls`.
            let accessed_object_refers_to_first_parameter =
                object.as_name_expr().map(|name| name.id.as_str())
                    == self.current_first_parameter_name;

            if accessed_object_refers_to_first_parameter {
                let symbol = self.add_attribute(attr.id().clone());
                self.add_attribute_definition(symbol, definition_kind);
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
        self.add_standalone_expression_impl(expression_node, ExpressionKind::Normal, None)
    }

    /// Record an expression that is immediately assigned to a target, and that needs to be a Salsa
    /// ingredient, because we need to infer its type standalone (type narrowing tests, RHS of an
    /// assignment.)
    fn add_standalone_assigned_expression(
        &mut self,
        expression_node: &ast::Expr,
        assigned_to: &ast::StmtAssign,
    ) -> Expression<'db> {
        self.add_standalone_expression_impl(
            expression_node,
            ExpressionKind::Normal,
            Some(assigned_to),
        )
    }

    /// Same as [`SemanticIndexBuilder::add_standalone_expression`], but marks the expression as a
    /// *type* expression, which makes sure that it will later be inferred as such.
    fn add_standalone_type_expression(&mut self, expression_node: &ast::Expr) -> Expression<'db> {
        self.add_standalone_expression_impl(expression_node, ExpressionKind::TypeExpression, None)
    }

    fn add_standalone_expression_impl(
        &mut self,
        expression_node: &ast::Expr,
        expression_kind: ExpressionKind,
        assigned_to: Option<&ast::StmtAssign>,
    ) -> Expression<'db> {
        let expression = Expression::new(
            self.db,
            self.file,
            self.current_scope(),
            #[expect(unsafe_code)]
            unsafe {
                AstNodeRef::new(self.module.clone(), expression_node)
            },
            #[expect(unsafe_code)]
            assigned_to
                .map(|assigned_to| unsafe { AstNodeRef::new(self.module.clone(), assigned_to) }),
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
        let value = self.add_standalone_expression(&generator.iter);
        self.visit_expr(&generator.iter);
        self.push_scope(scope);

        self.add_unpackable_assignment(
            &Unpackable::Comprehension {
                node: generator,
                first: true,
            },
            &generator.target,
            value,
        );

        for expr in &generator.ifs {
            self.visit_expr(expr);
        }

        for generator in generators_iter {
            let value = self.add_standalone_expression(&generator.iter);
            self.visit_expr(&generator.iter);

            self.add_unpackable_assignment(
                &Unpackable::Comprehension {
                    node: generator,
                    first: false,
                },
                &generator.target,
                value,
            );

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
                if matches!(unpackable, Unpackable::Comprehension { .. }) {
                    debug_assert_eq!(
                        self.scopes[self.current_scope()].node().scope_kind(),
                        ScopeKind::Comprehension
                    );
                }
                // The first iterator of the comprehension is evaluated in the outer scope, while all subsequent
                // nodes are evaluated in the inner scope.
                // SAFETY: The current scope is the comprehension, and the comprehension scope must have a parent scope.
                let value_file_scope =
                    if let Unpackable::Comprehension { first: true, .. } = unpackable {
                        self.scope_stack
                            .iter()
                            .rev()
                            .nth(1)
                            .expect("The comprehension scope must have a parent scope")
                            .file_scope_id
                    } else {
                        self.current_scope()
                    };
                let unpack = Some(Unpack::new(
                    self.db,
                    self.file,
                    value_file_scope,
                    self.current_scope(),
                    // SAFETY: `target` belongs to the `self.module` tree
                    #[expect(unsafe_code)]
                    unsafe {
                        AstNodeRef::new(self.module.clone(), target)
                    },
                    UnpackValue::new(unpackable.kind(), value),
                    countme::Count::default(),
                ));
                Some(unpackable.as_current_assignment(unpack))
            }
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                Some(unpackable.as_current_assignment(None))
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

        let mut instance_attribute_tables: IndexVec<_, _> = self
            .instance_attribute_tables
            .into_iter()
            .map(SymbolTableBuilder::finish)
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
        instance_attribute_tables.shrink_to_fit();
        use_def_maps.shrink_to_fit();
        ast_ids.shrink_to_fit();
        self.scopes_by_expression.shrink_to_fit();
        self.definitions_by_node.shrink_to_fit();

        self.scope_ids_by_scope.shrink_to_fit();
        self.scopes_by_node.shrink_to_fit();
        self.generator_functions.shrink_to_fit();
        self.eager_snapshots.shrink_to_fit();
        self.globals_by_scope.shrink_to_fit();

        SemanticIndex {
            symbol_tables,
            instance_attribute_tables,
            scopes: self.scopes,
            definitions_by_node: self.definitions_by_node,
            expressions_by_node: self.expressions_by_node,
            scope_ids_by_scope: self.scope_ids_by_scope,
            globals_by_scope: self.globals_by_scope,
            ast_ids,
            scopes_by_expression: self.scopes_by_expression,
            scopes_by_node: self.scopes_by_node,
            use_def_maps,
            imported_modules: Arc::new(self.imported_modules),
            has_future_annotations: self.has_future_annotations,
            eager_snapshots: self.eager_snapshots,
            semantic_syntax_errors: self.semantic_syntax_errors.into_inner(),
            generator_functions: self.generator_functions,
        }
    }

    fn with_semantic_checker(&mut self, f: impl FnOnce(&mut SemanticSyntaxChecker, &Self)) {
        let mut checker = std::mem::take(&mut self.semantic_checker);
        f(&mut checker, self);
        self.semantic_checker = checker;
    }

    fn source_text(&self) -> &SourceText {
        self.source_text
            .get_or_init(|| source_text(self.db.upcast(), self.file))
    }
}

impl<'db, 'ast> Visitor<'ast> for SemanticIndexBuilder<'db>
where
    'ast: 'db,
{
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        self.with_semantic_checker(|semantic, context| semantic.visit_stmt(stmt, context));

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

                // Record a use of the function name in the scope that it is defined in, so that it
                // can be used to find previously defined functions with the same name. This is
                // used to collect all the overloaded definitions of a function. This needs to be
                // done on the `Identifier` node as opposed to `ExprName` because that's what the
                // AST uses.
                self.mark_symbol_used(symbol);
                let use_id = self.current_ast_ids().record_use(name);
                self.current_use_def_map_mut()
                    .record_use(symbol, use_id, NodeKey::from_node(name));

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
                self.current_use_def_map_mut()
                    .record_node_reachability(NodeKey::from_node(node));

                for (alias_index, alias) in node.names.iter().enumerate() {
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
                            node,
                            alias_index,
                            is_reexported,
                        },
                    );
                }
            }
            ast::Stmt::ImportFrom(node) => {
                self.current_use_def_map_mut()
                    .record_node_reachability(NodeKey::from_node(node));

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
                        if !self.in_module_scope() {
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

                        let Some(referenced_module) = module.file() else {
                            continue;
                        };

                        // In order to understand the visibility of definitions created by a `*` import,
                        // we need to know the visibility of the global-scope definitions in the
                        // `referenced_module` the symbols imported from. Much like predicates for `if`
                        // statements can only have their visibility constraints resolved at type-inference
                        // time, the visibility of these global-scope definitions in the external module
                        // cannot be resolved at this point. As such, we essentially model each definition
                        // stemming from a `from exporter *` import as something like:
                        //
                        // ```py
                        // if <external_definition_is_visible>:
                        //     from exporter import name
                        // ```
                        //
                        // For more details, see the doc-comment on `StarImportPlaceholderPredicate`.
                        for export in exported_names(self.db, referenced_module) {
                            let symbol_id = self.add_symbol(export.clone());
                            let node_ref = StarImportDefinitionNodeRef { node, symbol_id };
                            let star_import = StarImportPlaceholderPredicate::new(
                                self.db,
                                self.file,
                                symbol_id,
                                referenced_module,
                            );

                            let pre_definition =
                                self.current_use_def_map().single_symbol_snapshot(symbol_id);
                            self.push_additional_definition(symbol_id, node_ref);
                            self.current_use_def_map_mut()
                                .record_and_negate_star_import_visibility_constraint(
                                    star_import,
                                    symbol_id,
                                    pre_definition,
                                );
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

            ast::Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _,
            }) => {
                // We model an `assert test, msg` statement here. Conceptually, we can think of
                // this as being equivalent to the following:
                //
                // ```py
                // if not test:
                //     msg
                //     <halt>
                //
                // <whatever code comes after>
                // ```
                //
                // Importantly, the `msg` expression is only evaluated if the `test` expression is
                // falsy. This is why we apply the negated `test` predicate as a narrowing and
                // reachability constraint on the `msg` expression.
                //
                // The other important part is the `<halt>`. This lets us skip the usual merging of
                // flow states and simplification of visibility constraints, since there is no way
                // of getting out of that `msg` branch. We simply restore to the post-test state.

                self.visit_expr(test);
                let predicate = self.build_predicate(test);

                if let Some(msg) = msg {
                    let post_test = self.flow_snapshot();
                    let negated_predicate = predicate.negated();
                    self.record_narrowing_constraint(negated_predicate);
                    self.record_reachability_constraint(negated_predicate);
                    self.visit_expr(msg);
                    self.record_visibility_constraint(negated_predicate);
                    self.flow_restore(post_test);
                }

                self.record_narrowing_constraint(predicate);
                self.record_visibility_constraint(predicate);
                self.record_reachability_constraint(predicate);
            }

            ast::Stmt::Assign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);

                self.visit_expr(&node.value);
                let value = self.add_standalone_assigned_expression(&node.value, node);

                for target in &node.targets {
                    self.add_unpackable_assignment(&Unpackable::Assign(node), target, value);
                }
            }
            ast::Stmt::AnnAssign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);
                self.visit_expr(&node.annotation);
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
                    self.pop_assignment();
                } else {
                    self.visit_expr(&node.target);
                }
            }
            ast::Stmt::AugAssign(
                aug_assign @ ast::StmtAugAssign {
                    range: _,
                    target,
                    op,
                    value,
                },
            ) => {
                debug_assert_eq!(&self.current_assignments, &[]);
                self.visit_expr(value);

                match &**target {
                    ast::Expr::Name(ast::ExprName { id, .. })
                        if id == "__all__" && op.is_add() && self.in_module_scope() =>
                    {
                        if let ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) =
                            &**value
                        {
                            if attr == "__all__" {
                                self.add_standalone_expression(value);
                            }
                        }

                        self.push_assignment(aug_assign.into());
                        self.visit_expr(target);
                        self.pop_assignment();
                    }
                    ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                        self.push_assignment(aug_assign.into());
                        self.visit_expr(target);
                        self.pop_assignment();
                    }
                    _ => {
                        self.visit_expr(target);
                    }
                }
            }
            ast::Stmt::If(node) => {
                self.visit_expr(&node.test);
                let mut no_branch_taken = self.flow_snapshot();
                let mut last_predicate = self.record_expression_narrowing_constraint(&node.test);
                let mut reachability_constraint =
                    self.record_reachability_constraint(last_predicate);
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
                    self.record_negated_reachability_constraint(reachability_constraint);

                    let elif_predicate = if let Some(elif_test) = clause_test {
                        self.visit_expr(elif_test);
                        // A test expression is evaluated whether the branch is taken or not
                        no_branch_taken = self.flow_snapshot();
                        reachability_constraint =
                            self.record_reachability_constraint(last_predicate);
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
                self.record_reachability_constraint(predicate);

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

                let mut no_case_matched = self.flow_snapshot();

                let has_catchall = cases
                    .last()
                    .is_some_and(|case| case.guard.is_none() && case.pattern.is_wildcard());

                let mut post_case_snapshots = vec![];
                let mut match_predicate;

                for (i, case) in cases.iter().enumerate() {
                    self.current_match_case = Some(CurrentMatchCase::new(&case.pattern));
                    self.visit_pattern(&case.pattern);
                    self.current_match_case = None;
                    // unlike in [Stmt::If], we don't reset [no_case_matched]
                    // here because the effects of visiting a pattern is binding
                    // symbols, and this doesn't occur unless the pattern
                    // actually matches
                    match_predicate = self.add_pattern_narrowing_constraint(
                        subject_expr,
                        &case.pattern,
                        case.guard.as_deref(),
                    );
                    let vis_constraint_id = self.record_reachability_constraint(match_predicate);

                    let match_success_guard_failure = case.guard.as_ref().map(|guard| {
                        let guard_expr = self.add_standalone_expression(guard);
                        self.visit_expr(guard);
                        let post_guard_eval = self.flow_snapshot();
                        let predicate = Predicate {
                            node: PredicateNode::Expression(guard_expr),
                            is_positive: true,
                        };
                        self.record_negated_narrowing_constraint(predicate);
                        let match_success_guard_failure = self.flow_snapshot();
                        self.flow_restore(post_guard_eval);
                        self.record_narrowing_constraint(predicate);
                        match_success_guard_failure
                    });

                    self.record_visibility_constraint_id(vis_constraint_id);

                    self.visit_body(&case.body);

                    post_case_snapshots.push(self.flow_snapshot());

                    if i != cases.len() - 1 || !has_catchall {
                        // We need to restore the state after each case, but not after the last
                        // one. The last one will just become the state that we merge the other
                        // snapshots into.
                        self.flow_restore(no_case_matched.clone());
                        self.record_negated_narrowing_constraint(match_predicate);
                        if let Some(match_success_guard_failure) = match_success_guard_failure {
                            self.flow_merge(match_success_guard_failure);
                        } else {
                            assert!(case.guard.is_none());
                        }
                    } else {
                        debug_assert!(match_success_guard_failure.is_none());
                        debug_assert!(case.guard.is_none());
                    }

                    self.record_negated_visibility_constraint(vis_constraint_id);
                    no_case_matched = self.flow_snapshot();
                }

                for post_clause_state in post_case_snapshots {
                    self.flow_merge(post_clause_state);
                }

                self.simplify_visibility_constraints(no_case_matched);
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
                    // We will revert to this state prior to visiting the `else` block,
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
            ast::Stmt::Global(ast::StmtGlobal { range: _, names }) => {
                for name in names {
                    let symbol_id = self.add_symbol(name.id.clone());
                    let symbol_table = self.current_symbol_table();
                    let symbol = symbol_table.symbol(symbol_id);
                    if symbol.is_bound() || symbol.is_declared() || symbol.is_used() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::LoadBeforeGlobalDeclaration {
                                name: name.to_string(),
                                start: name.range.start(),
                            },
                            range: name.range,
                            python_version: self.python_version,
                        });
                    }
                    let scope_id = self.current_scope();
                    self.globals_by_scope
                        .entry(scope_id)
                        .or_default()
                        .insert(symbol_id);
                }
                walk_stmt(self, stmt);
            }
            ast::Stmt::Delete(ast::StmtDelete { targets, range: _ }) => {
                for target in targets {
                    if let ast::Expr::Name(ast::ExprName { id, .. }) = target {
                        let symbol_id = self.add_symbol(id.clone());
                        self.current_symbol_table().mark_symbol_used(symbol_id);
                    }
                }
                walk_stmt(self, stmt);
            }
            ast::Stmt::Expr(ast::StmtExpr { value, range: _ }) if self.in_module_scope() => {
                if let Some(expr) = dunder_all_extend_argument(value) {
                    self.add_standalone_expression(expr);
                }
                self.visit_expr(value);
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        self.with_semantic_checker(|semantic, context| semantic.visit_expr(expr, context));

        self.scopes_by_expression
            .insert(expr.into(), self.current_scope());
        self.current_ast_ids().record_expression(expr);

        let node_key = NodeKey::from_node(expr);

        match expr {
            ast::Expr::Name(ast::ExprName { id, ctx, .. }) => {
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
                    self.current_use_def_map_mut()
                        .record_use(symbol, use_id, node_key);
                }

                if is_definition {
                    match self.current_assignment() {
                        Some(CurrentAssignment::Assign { node, unpack }) => {
                            self.add_definition(
                                symbol,
                                AssignmentDefinitionNodeRef {
                                    unpack,
                                    value: &node.value,
                                    target: expr,
                                },
                            );
                        }
                        Some(CurrentAssignment::AnnAssign(ann_assign)) => {
                            self.add_definition(
                                symbol,
                                AnnotatedAssignmentDefinitionNodeRef {
                                    node: ann_assign,
                                    annotation: &ann_assign.annotation,
                                    value: ann_assign.value.as_deref(),
                                    target: expr,
                                },
                            );
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
                                    target: expr,
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
                        Some(CurrentAssignment::Comprehension {
                            unpack,
                            node,
                            first,
                        }) => {
                            self.add_definition(
                                symbol,
                                ComprehensionDefinitionNodeRef {
                                    unpack,
                                    iterable: &node.iter,
                                    target: expr,
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
                                    target: expr,
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
                let reachability_constraint = self.record_reachability_constraint(predicate);
                self.visit_expr(body);
                let visibility_constraint = self.record_visibility_constraint(predicate);
                let post_body = self.flow_snapshot();
                self.flow_restore(pre_if.clone());

                self.record_negated_narrowing_constraint(predicate);
                self.record_negated_reachability_constraint(reachability_constraint);
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

                    // For the last value, we don't need to model control flow. There is no short-circuiting
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
                        self.record_reachability_constraint_id(predicate_id);
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
                ctx,
                range: _,
            }) => {
                if ctx.is_store() {
                    match self.current_assignment() {
                        Some(CurrentAssignment::Assign { node, unpack, .. }) => {
                            // SAFETY: `value` and `expr` belong to the `self.module` tree
                            #[expect(unsafe_code)]
                            let assignment = AssignmentDefinitionKind::new(
                                TargetKind::from(unpack),
                                unsafe { AstNodeRef::new(self.module.clone(), &node.value) },
                                unsafe { AstNodeRef::new(self.module.clone(), expr) },
                            );
                            self.register_attribute_assignment(
                                object,
                                attr,
                                DefinitionKind::Assignment(assignment),
                            );
                        }
                        Some(CurrentAssignment::AnnAssign(ann_assign)) => {
                            self.add_standalone_type_expression(&ann_assign.annotation);
                            // SAFETY: `annotation`, `value` and `expr` belong to the `self.module` tree
                            #[expect(unsafe_code)]
                            let assignment = AnnotatedAssignmentDefinitionKind::new(
                                unsafe {
                                    AstNodeRef::new(self.module.clone(), &ann_assign.annotation)
                                },
                                ann_assign.value.as_deref().map(|value| unsafe {
                                    AstNodeRef::new(self.module.clone(), value)
                                }),
                                unsafe { AstNodeRef::new(self.module.clone(), expr) },
                            );
                            self.register_attribute_assignment(
                                object,
                                attr,
                                DefinitionKind::AnnotatedAssignment(assignment),
                            );
                        }
                        Some(CurrentAssignment::For { node, unpack, .. }) => {
                            // // SAFETY: `iter` and `expr` belong to the `self.module` tree
                            #[expect(unsafe_code)]
                            let assignment = ForStmtDefinitionKind::new(
                                TargetKind::from(unpack),
                                unsafe { AstNodeRef::new(self.module.clone(), &node.iter) },
                                unsafe { AstNodeRef::new(self.module.clone(), expr) },
                                node.is_async,
                            );
                            self.register_attribute_assignment(
                                object,
                                attr,
                                DefinitionKind::For(assignment),
                            );
                        }
                        Some(CurrentAssignment::WithItem {
                            item,
                            unpack,
                            is_async,
                            ..
                        }) => {
                            // SAFETY: `context_expr` and `expr` belong to the `self.module` tree
                            #[expect(unsafe_code)]
                            let assignment = WithItemDefinitionKind::new(
                                TargetKind::from(unpack),
                                unsafe { AstNodeRef::new(self.module.clone(), &item.context_expr) },
                                unsafe { AstNodeRef::new(self.module.clone(), expr) },
                                is_async,
                            );
                            self.register_attribute_assignment(
                                object,
                                attr,
                                DefinitionKind::WithItem(assignment),
                            );
                        }
                        Some(CurrentAssignment::Comprehension {
                            unpack,
                            node,
                            first,
                        }) => {
                            // SAFETY: `iter` and `expr` belong to the `self.module` tree
                            #[expect(unsafe_code)]
                            let assignment = ComprehensionDefinitionKind {
                                target_kind: TargetKind::from(unpack),
                                iterable: unsafe {
                                    AstNodeRef::new(self.module.clone(), &node.iter)
                                },
                                target: unsafe { AstNodeRef::new(self.module.clone(), expr) },
                                first,
                                is_async: node.is_async,
                            };
                            // Temporarily move to the scope of the method to which the instance attribute is defined.
                            // SAFETY: `self.scope_stack` is not empty because the targets in comprehensions should always introduce a new scope.
                            let scope = self.scope_stack.pop().expect("The popped scope must be a comprehension, which must have a parent scope");
                            self.register_attribute_assignment(
                                object,
                                attr,
                                DefinitionKind::Comprehension(assignment),
                            );
                            self.scope_stack.push(scope);
                        }
                        Some(CurrentAssignment::AugAssign(_)) => {
                            // TODO:
                        }
                        Some(CurrentAssignment::Named(_)) => {
                            // A named expression whose target is an attribute is syntactically prohibited
                        }
                        None => {}
                    }
                }

                // Track reachability of attribute expressions to silence `unresolved-attribute`
                // diagnostics in unreachable code.
                self.current_use_def_map_mut()
                    .record_node_reachability(node_key);

                walk_expr(self, expr);
            }
            ast::Expr::StringLiteral(_) => {
                // Track reachability of string literals, as they could be a stringified annotation
                // with child expressions whose reachability we are interested in.
                self.current_use_def_map_mut()
                    .record_node_reachability(node_key);

                walk_expr(self, expr);
            }
            ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                let scope = self.current_scope();
                if self.scopes[scope].kind() == ScopeKind::Function {
                    self.generator_functions.insert(scope);
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

impl SemanticSyntaxContext for SemanticIndexBuilder<'_> {
    fn future_annotations_or_stub(&self) -> bool {
        self.has_future_annotations
    }

    fn python_version(&self) -> PythonVersion {
        self.python_version
    }

    fn source(&self) -> &str {
        self.source_text().as_str()
    }

    // We handle the one syntax error that relies on this method (`LoadBeforeGlobalDeclaration`)
    // directly in `visit_stmt`, so this just returns a placeholder value.
    fn global(&self, _name: &str) -> Option<TextRange> {
        None
    }

    fn in_async_context(&self) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_info.file_scope_id];
            match scope.kind() {
                ScopeKind::Class | ScopeKind::Lambda => return false,
                ScopeKind::Function => {
                    return scope.node().expect_function().is_async;
                }
                ScopeKind::Comprehension
                | ScopeKind::Module
                | ScopeKind::TypeAlias
                | ScopeKind::Annotation => {}
            }
        }
        false
    }

    fn in_await_allowed_context(&self) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_info.file_scope_id];
            match scope.kind() {
                ScopeKind::Class => return false,
                ScopeKind::Function | ScopeKind::Lambda => return true,
                ScopeKind::Comprehension
                | ScopeKind::Module
                | ScopeKind::TypeAlias
                | ScopeKind::Annotation => {}
            }
        }
        false
    }

    fn in_yield_allowed_context(&self) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_info.file_scope_id];
            match scope.kind() {
                ScopeKind::Class | ScopeKind::Comprehension => return false,
                ScopeKind::Function | ScopeKind::Lambda => return true,
                ScopeKind::Module | ScopeKind::TypeAlias | ScopeKind::Annotation => {}
            }
        }
        false
    }

    fn in_sync_comprehension(&self) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_info.file_scope_id];
            let generators = match scope.node() {
                NodeWithScopeKind::ListComprehension(node) => &node.generators,
                NodeWithScopeKind::SetComprehension(node) => &node.generators,
                NodeWithScopeKind::DictComprehension(node) => &node.generators,
                _ => continue,
            };
            if generators
                .iter()
                .all(|comprehension| !comprehension.is_async)
            {
                return true;
            }
        }
        false
    }

    fn in_module_scope(&self) -> bool {
        self.scope_stack.len() == 1
    }

    fn in_function_scope(&self) -> bool {
        let kind = self.scopes[self.current_scope()].kind();
        matches!(kind, ScopeKind::Function | ScopeKind::Lambda)
    }

    fn in_generator_scope(&self) -> bool {
        matches!(
            self.scopes[self.current_scope()].node(),
            NodeWithScopeKind::GeneratorExpression(_)
        )
    }

    fn in_notebook(&self) -> bool {
        self.source_text().is_notebook()
    }

    fn report_semantic_error(&self, error: SemanticSyntaxError) {
        if self.db.is_file_open(self.file) {
            self.semantic_syntax_errors.borrow_mut().push(error);
        }
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
        unpack: Option<(UnpackPosition, Unpack<'a>)>,
    },
    WithItem {
        item: &'a ast::WithItem,
        is_async: bool,
        unpack: Option<(UnpackPosition, Unpack<'a>)>,
    },
}

impl CurrentAssignment<'_> {
    fn unpack_position_mut(&mut self) -> Option<&mut UnpackPosition> {
        match self {
            Self::Assign { unpack, .. }
            | Self::For { unpack, .. }
            | Self::WithItem { unpack, .. }
            | Self::Comprehension { unpack, .. } => unpack.as_mut().map(|(position, _)| position),
            Self::AnnAssign(_) | Self::AugAssign(_) | Self::Named(_) => None,
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
    Comprehension {
        first: bool,
        node: &'a ast::Comprehension,
    },
}

impl<'a> Unpackable<'a> {
    const fn kind(&self) -> UnpackKind {
        match self {
            Unpackable::Assign(_) => UnpackKind::Assign,
            Unpackable::For(_) | Unpackable::Comprehension { .. } => UnpackKind::Iterable,
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
            Unpackable::Comprehension { node, first } => CurrentAssignment::Comprehension {
                node,
                first: *first,
                unpack,
            },
        }
    }
}

/// Returns the single argument to `__all__.extend()`, if it is a call to `__all__.extend()`
/// where it looks like the argument might be a `submodule.__all__` expression.
/// Else, returns `None`.
fn dunder_all_extend_argument(value: &ast::Expr) -> Option<&ast::Expr> {
    let ast::ExprCall {
        func,
        arguments:
            ast::Arguments {
                args,
                keywords,
                range: _,
            },
        ..
    } = value.as_call_expr()?;

    let ast::ExprAttribute { value, attr, .. } = func.as_attribute_expr()?;

    let ast::ExprName { id, .. } = value.as_name_expr()?;

    if id != "__all__" {
        return None;
    }

    if attr != "extend" {
        return None;
    }

    if !keywords.is_empty() {
        return None;
    }

    let [single_argument] = &**args else {
        return None;
    };

    let ast::ExprAttribute { value, attr, .. } = single_argument.as_attribute_expr()?;

    (attr == "__all__").then_some(value)
}
