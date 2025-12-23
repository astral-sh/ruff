use std::cell::{OnceCell, RefCell};
use std::sync::Arc;

use except_handlers::TryNodeContextStackManager;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_db::source::{SourceText, source_text};
use ruff_index::IndexVec;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_pattern, walk_stmt};
use ruff_python_ast::{self as ast, NodeIndex, PySourceType, PythonVersion};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxChecker, SemanticSyntaxContext, SemanticSyntaxError, SemanticSyntaxErrorKind,
};
use ruff_text_size::TextRange;
use ty_module_resolver::{ModuleName, resolve_module};

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::ast_ids::AstIdsBuilder;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::definition::{
    AnnotatedAssignmentDefinitionNodeRef, AssignmentDefinitionNodeRef,
    ComprehensionDefinitionNodeRef, Definition, DefinitionCategory, DefinitionNodeKey,
    DefinitionNodeRef, Definitions, ExceptHandlerDefinitionNodeRef, ForStmtDefinitionNodeRef,
    ImportDefinitionNodeRef, ImportFromDefinitionNodeRef, ImportFromSubmoduleDefinitionNodeRef,
    MatchPatternDefinitionNodeRef, StarImportDefinitionNodeRef, WithItemDefinitionNodeRef,
};
use crate::semantic_index::expression::{Expression, ExpressionKind};
use crate::semantic_index::place::{PlaceExpr, PlaceTableBuilder, ScopedPlaceId};
use crate::semantic_index::predicate::{
    CallableAndCallExpr, ClassPatternKind, PatternPredicate, PatternPredicateKind, Predicate,
    PredicateNode, PredicateOrLiteral, ScopedPredicateId, StarImportPlaceholderPredicate,
};
use crate::semantic_index::re_exports::exported_names;
use crate::semantic_index::reachability_constraints::{
    ReachabilityConstraintsBuilder, ScopedReachabilityConstraintId,
};
use crate::semantic_index::scope::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeKind, NodeWithScopeRef,
};
use crate::semantic_index::scope::{Scope, ScopeId, ScopeKind, ScopeLaziness};
use crate::semantic_index::symbol::{ScopedSymbolId, Symbol};
use crate::semantic_index::use_def::{
    EnclosingSnapshotKey, FlowSnapshot, ScopedEnclosingSnapshotId, UseDefMapBuilder,
};
use crate::semantic_index::{ExpressionsScopeMap, SemanticIndex, VisibleAncestorsIter};
use crate::semantic_model::HasTrackedScope;
use crate::unpack::{EvaluationMode, Unpack, UnpackKind, UnpackPosition, UnpackValue};
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

pub(super) struct SemanticIndexBuilder<'db, 'ast> {
    // Builder state
    db: &'db dyn Db,
    file: File,
    source_type: PySourceType,
    module: &'ast ParsedModuleRef,
    scope_stack: Vec<ScopeInfo>,
    /// The assignments we're currently visiting, with
    /// the most recent visit at the end of the Vec
    current_assignments: Vec<CurrentAssignment<'ast, 'db>>,
    /// The match case we're currently visiting.
    current_match_case: Option<CurrentMatchCase<'ast>>,
    /// The name of the first function parameter of the innermost function that we're currently visiting.
    current_first_parameter_name: Option<&'ast str>,

    /// Per-scope contexts regarding nested `try`/`except` statements
    try_node_context_stack_manager: TryNodeContextStackManager,

    /// Flags about the file's global scope
    has_future_annotations: bool,
    /// Whether we are currently visiting an `if TYPE_CHECKING` block.
    in_type_checking_block: bool,

    // Used for checking semantic syntax errors
    python_version: PythonVersion,
    source_text: OnceCell<SourceText>,
    semantic_checker: SemanticSyntaxChecker,

    // Semantic Index fields
    scopes: IndexVec<FileScopeId, Scope>,
    scope_ids_by_scope: IndexVec<FileScopeId, ScopeId<'db>>,
    place_tables: IndexVec<FileScopeId, PlaceTableBuilder>,
    ast_ids: IndexVec<FileScopeId, AstIdsBuilder>,
    use_def_maps: IndexVec<FileScopeId, UseDefMapBuilder<'db>>,
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,
    scopes_by_expression: ExpressionsScopeMapBuilder,
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definitions<'db>>,
    expressions_by_node: FxHashMap<ExpressionNodeKey, Expression<'db>>,
    imported_modules: FxHashSet<ModuleName>,
    seen_submodule_imports: FxHashSet<String>,
    /// Hashset of all [`FileScopeId`]s that correspond to [generator functions].
    ///
    /// [generator functions]: https://docs.python.org/3/glossary.html#term-generator
    generator_functions: FxHashSet<FileScopeId>,
    /// Snapshots of enclosing-scope place states visible from nested scopes.
    enclosing_snapshots: FxHashMap<EnclosingSnapshotKey, ScopedEnclosingSnapshotId>,
    /// Errors collected by the `semantic_checker`.
    semantic_syntax_errors: RefCell<Vec<SemanticSyntaxError>>,
}

impl<'db, 'ast> SemanticIndexBuilder<'db, 'ast> {
    pub(super) fn new(db: &'db dyn Db, file: File, module_ref: &'ast ParsedModuleRef) -> Self {
        let mut builder = Self {
            db,
            file,
            source_type: file.source_type(db),
            module: module_ref,
            scope_stack: Vec::new(),
            current_assignments: vec![],
            current_match_case: None,
            current_first_parameter_name: None,
            try_node_context_stack_manager: TryNodeContextStackManager::default(),

            has_future_annotations: false,
            in_type_checking_block: false,

            scopes: IndexVec::new(),
            place_tables: IndexVec::new(),
            ast_ids: IndexVec::new(),
            scope_ids_by_scope: IndexVec::new(),
            use_def_maps: IndexVec::new(),

            scopes_by_expression: ExpressionsScopeMapBuilder::new(),
            scopes_by_node: FxHashMap::default(),
            definitions_by_node: FxHashMap::default(),
            expressions_by_node: FxHashMap::default(),

            seen_submodule_imports: FxHashSet::default(),
            imported_modules: FxHashSet::default(),
            generator_functions: FxHashSet::default(),

            enclosing_snapshots: FxHashMap::default(),

            python_version: Program::get(db).python_version(db),
            source_text: OnceCell::new(),
            semantic_checker: SemanticSyntaxChecker::default(),
            semantic_syntax_errors: RefCell::default(),
        };

        builder.push_scope_with_parent(
            NodeWithScopeRef::Module,
            None,
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
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

    /// Returns the scope ID of the current scope if the current scope
    /// is a method inside a class body or an eagerly executed scope inside a method.
    /// Returns `None` otherwise, e.g. if the current scope is a function body outside of a class, or if the current scope is not a
    /// function body.
    fn is_method_or_eagerly_executed_in_method(&self) -> Option<FileScopeId> {
        let mut scopes_rev = self
            .scope_stack
            .iter()
            .rev()
            .skip_while(|scope| self.scopes[scope.file_scope_id].is_eager());
        let current = scopes_rev.next()?;

        if self.scopes[current.file_scope_id].kind() != ScopeKind::Function {
            return None;
        }

        let maybe_method = current.file_scope_id;
        let parent = scopes_rev.next()?;

        match self.scopes[parent.file_scope_id].kind() {
            ScopeKind::Class => Some(maybe_method),
            ScopeKind::TypeParams => {
                // If the function is generic, the parent scope is an annotation scope.
                // In this case, we need to go up one level higher to find the class scope.
                let grandparent = scopes_rev.next()?;

                if self.scopes[grandparent.file_scope_id].kind() == ScopeKind::Class {
                    Some(maybe_method)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Checks if a symbol name is bound in any intermediate eager scopes
    /// between the current scope and the specified method scope.
    ///
    fn is_symbol_bound_in_intermediate_eager_scopes(
        &self,
        symbol_name: &str,
        method_scope_id: FileScopeId,
    ) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            let scope_id = scope_info.file_scope_id;

            if scope_id == method_scope_id {
                break;
            }

            if let Some(symbol_id) = self.place_tables[scope_id].symbol_id(symbol_name) {
                let symbol = self.place_tables[scope_id].symbol(symbol_id);
                if symbol.is_bound() {
                    return true;
                }
            }
        }

        false
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
        reachability: ScopedReachabilityConstraintId,
    ) {
        let children_start = self.scopes.next_index() + 1;

        // Note `node` is guaranteed to be a child of `self.module`
        let node_with_kind = node.to_kind(self.module);

        let scope = Scope::new(
            parent,
            node_with_kind,
            children_start..children_start,
            reachability,
            self.in_type_checking_block,
        );
        let is_class_scope = scope.kind().is_class();
        self.try_node_context_stack_manager.enter_nested_scope();

        let file_scope_id = self.scopes.push(scope);
        self.place_tables.push(PlaceTableBuilder::default());
        self.use_def_maps
            .push(UseDefMapBuilder::new(is_class_scope));
        let ast_id_scope = self.ast_ids.push(AstIdsBuilder::default());

        let scope_id = ScopeId::new(self.db, self.file, file_scope_id);

        self.scope_ids_by_scope.push(scope_id);
        let previous = self.scopes_by_node.insert(node.node_key(), file_scope_id);
        debug_assert_eq!(previous, None);

        debug_assert_eq!(ast_id_scope, file_scope_id);

        self.scope_stack.push(ScopeInfo {
            file_scope_id,
            current_loop: None,
        });
    }

    // Records snapshots of the place states visible from the current eager scope.
    fn record_eager_snapshots(&mut self, popped_scope_id: FileScopeId) {
        let popped_scope = &self.scopes[popped_scope_id];
        let popped_scope_is_annotation_scope = popped_scope.kind().is_annotation();

        // If the scope that we just popped off is an eager scope, we need to "lock" our view of
        // which bindings reach each of the uses in the scope. Loop through each enclosing scope,
        // looking for any that bind each place.
        // TODO: Bindings in eager nested scopes also need to be recorded. For example:
        // ```python
        // class C:
        //     x: int | None = None
        // c = C()
        // class _:
        //     c.x = 1
        // reveal_type(c.x)  # revealed: Literal[1]
        // ```
        for enclosing_scope_info in self.scope_stack.iter().rev() {
            let enclosing_scope_id = enclosing_scope_info.file_scope_id;
            let is_immediately_enclosing_scope = popped_scope.parent() == Some(enclosing_scope_id);
            let enclosing_scope_kind = self.scopes[enclosing_scope_id].kind();
            let enclosing_place_table = &self.place_tables[enclosing_scope_id];

            for nested_place in self.place_tables[popped_scope_id].iter() {
                // Skip this place if this enclosing scope doesn't contain any bindings for it.
                // Note that even if this place is bound in the popped scope,
                // it may refer to the enclosing scope bindings
                // so we also need to snapshot the bindings of the enclosing scope.

                let Some(enclosing_place_id) = enclosing_place_table.place_id(nested_place) else {
                    continue;
                };
                let enclosing_place = enclosing_place_table.place(enclosing_place_id);

                // Snapshot the state of this place that are visible at this point in this
                // enclosing scope.
                let key = EnclosingSnapshotKey {
                    enclosing_scope: enclosing_scope_id,
                    enclosing_place: enclosing_place_id,
                    nested_scope: popped_scope_id,
                    nested_laziness: ScopeLaziness::Eager,
                };
                let eager_snapshot = self.use_def_maps[enclosing_scope_id]
                    .snapshot_enclosing_state(
                        enclosing_place_id,
                        enclosing_scope_kind,
                        enclosing_place,
                        popped_scope_is_annotation_scope && is_immediately_enclosing_scope,
                    );
                self.enclosing_snapshots.insert(key, eager_snapshot);
            }

            // Lazy scopes are "sticky": once we see a lazy scope we stop doing lookups
            // eagerly, even if we would encounter another eager enclosing scope later on.
            if !enclosing_scope_kind.is_eager() {
                break;
            }
        }
    }

    fn bound_scope(&self, enclosing_scope: FileScopeId, symbol: &Symbol) -> Option<FileScopeId> {
        self.scope_stack
            .iter()
            .rev()
            .skip_while(|scope| scope.file_scope_id != enclosing_scope)
            .find_map(|scope_info| {
                let scope_id = scope_info.file_scope_id;
                let place_table = &self.place_tables[scope_id];
                let place_id = place_table.symbol_id(symbol.name())?;
                place_table.place(place_id).is_bound().then_some(scope_id)
            })
    }

    // Records snapshots of the place states visible from the current lazy scope.
    fn record_lazy_snapshots(&mut self, popped_scope_id: FileScopeId) {
        for enclosing_scope_info in self.scope_stack.iter().rev() {
            let enclosing_scope_id = enclosing_scope_info.file_scope_id;
            let enclosing_scope_kind = self.scopes[enclosing_scope_id].kind();
            let enclosing_place_table = &self.place_tables[enclosing_scope_id];

            // We don't record lazy snapshots of attributes or subscripts, because these are difficult to track as they modify.
            for nested_symbol in self.place_tables[popped_scope_id].symbols() {
                // For the same reason, symbols declared as nonlocal or global are not recorded.
                // Also, if the enclosing scope allows its members to be modified from elsewhere, the snapshot will not be recorded.
                // (In the case of class scopes, class variables can be modified from elsewhere, but this has no effect in nested scopes,
                // as class variables are not visible to them)
                if self.scopes[enclosing_scope_id].kind().is_module() {
                    continue;
                }

                // Skip this place if this enclosing scope doesn't contain any bindings for it.
                // Note that even if this place is bound in the popped scope,
                // it may refer to the enclosing scope bindings
                // so we also need to snapshot the bindings of the enclosing scope.
                let Some(enclosed_symbol_id) =
                    enclosing_place_table.symbol_id(nested_symbol.name())
                else {
                    continue;
                };
                let enclosing_place = enclosing_place_table.symbol(enclosed_symbol_id);
                if !enclosing_place.is_bound() {
                    // If the bound scope of a place can be modified from elsewhere, the snapshot will not be recorded.
                    if self
                        .bound_scope(enclosing_scope_id, nested_symbol)
                        .is_none_or(|scope| self.scopes[scope].visibility().is_public())
                    {
                        continue;
                    }
                }

                // Snapshot the state of this place that are visible at this point in this
                // enclosing scope (this may later be invalidated and swept away).
                let key = EnclosingSnapshotKey {
                    enclosing_scope: enclosing_scope_id,
                    enclosing_place: enclosed_symbol_id.into(),
                    nested_scope: popped_scope_id,
                    nested_laziness: ScopeLaziness::Lazy,
                };
                let lazy_snapshot = self.use_def_maps[enclosing_scope_id].snapshot_enclosing_state(
                    enclosed_symbol_id.into(),
                    enclosing_scope_kind,
                    enclosing_place.into(),
                    false,
                );
                self.enclosing_snapshots.insert(key, lazy_snapshot);
            }
        }
    }

    /// Any lazy snapshots of the place that have been reassigned are obsolete, so update them.
    /// ```py
    /// def outer() -> None:
    ///     x = None
    ///
    ///     def inner2() -> None:
    ///         # `inner` can be referenced before its definition,
    ///         # but `inner2` must still be called after the definition of `inner` for this call to be valid.
    ///         inner()
    ///
    ///         # In this scope, `x` may refer to `x = None` or `x = 1`.
    ///         reveal_type(x)  # revealed: None | Literal[1]
    ///
    ///     # Reassignment of `x` after the definition of `inner2`.
    ///     # Update lazy snapshots of `x` for `inner2`.
    ///     x = 1
    ///
    ///     def inner() -> None:
    ///         # In this scope, `x = None` appears as being shadowed by `x = 1`.
    ///         reveal_type(x)  # revealed: Literal[1]
    ///
    ///     # No reassignment of `x` after the definition of `inner`, so we can safely use a lazy snapshot for `inner` as is.
    ///     inner()
    ///     inner2()
    /// ```
    fn update_lazy_snapshots(&mut self, symbol: ScopedSymbolId) {
        let current_scope = self.current_scope();
        let current_place_table = &self.place_tables[current_scope];
        let symbol = current_place_table.symbol(symbol);
        // Optimization: if this is the first binding of the symbol we've seen, there can't be any
        // lazy snapshots of it to update.
        if !symbol.is_reassigned() {
            return;
        }
        for (key, snapshot_id) in &self.enclosing_snapshots {
            if let Some(enclosing_symbol) = key.enclosing_place.as_symbol() {
                let name = self.place_tables[key.enclosing_scope]
                    .symbol(enclosing_symbol)
                    .name();
                let is_reassignment_of_snapshotted_symbol = || {
                    for (ancestor, _) in
                        VisibleAncestorsIter::new(&self.scopes, key.enclosing_scope)
                    {
                        if ancestor == current_scope {
                            return true;
                        }
                        let ancestor_table = &self.place_tables[ancestor];
                        // If there is a symbol binding in an ancestor scope,
                        // then a reassignment in the current scope is not relevant to the snapshot.
                        if ancestor_table
                            .symbol_id(name)
                            .is_some_and(|id| ancestor_table.symbol(id).is_bound())
                        {
                            return false;
                        }
                    }
                    false
                };

                if key.nested_laziness.is_lazy()
                    && symbol.name() == name
                    && is_reassignment_of_snapshotted_symbol()
                {
                    self.use_def_maps[key.enclosing_scope]
                        .update_enclosing_snapshot(*snapshot_id, enclosing_symbol);
                }
            }
        }
    }

    fn sweep_nonlocal_lazy_snapshots(&mut self) {
        self.enclosing_snapshots.retain(|key, _| {
            let place_table = &self.place_tables[key.enclosing_scope];

            let is_bound_and_non_local = || -> bool {
                let ScopedPlaceId::Symbol(symbol_id) = key.enclosing_place else {
                    return false;
                };

                let symbol = place_table.symbol(symbol_id);
                self.scopes
                    .iter_enumerated()
                    .skip_while(|(scope_id, _)| *scope_id != key.enclosing_scope)
                    .any(|(scope_id, _)| {
                        let other_scope_place_table = &self.place_tables[scope_id];
                        let Some(symbol_id) = other_scope_place_table.symbol_id(symbol.name())
                        else {
                            return false;
                        };
                        let symbol = other_scope_place_table.symbol(symbol_id);
                        symbol.is_nonlocal() && symbol.is_bound()
                    })
            };

            key.nested_laziness.is_eager() || !is_bound_and_non_local()
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

        if popped_scope.is_eager() {
            self.record_eager_snapshots(popped_scope_id);
        } else {
            self.record_lazy_snapshots(popped_scope_id);
        }

        popped_scope_id
    }

    fn current_place_table(&self) -> &PlaceTableBuilder {
        let scope_id = self.current_scope();
        &self.place_tables[scope_id]
    }

    fn current_place_table_mut(&mut self) -> &mut PlaceTableBuilder {
        let scope_id = self.current_scope();
        &mut self.place_tables[scope_id]
    }

    fn current_use_def_map_mut(&mut self) -> &mut UseDefMapBuilder<'db> {
        let scope_id = self.current_scope();
        &mut self.use_def_maps[scope_id]
    }

    fn current_use_def_map(&self) -> &UseDefMapBuilder<'db> {
        let scope_id = self.current_scope();
        &self.use_def_maps[scope_id]
    }

    fn current_reachability_constraints_mut(&mut self) -> &mut ReachabilityConstraintsBuilder {
        let scope_id = self.current_scope();
        &mut self.use_def_maps[scope_id].reachability_constraints
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

    /// Add a symbol to the place table and the use-def map.
    /// Return the [`ScopedPlaceId`] that uniquely identifies the symbol in both.
    fn add_symbol(&mut self, name: Name) -> ScopedSymbolId {
        let (symbol_id, added) = self.current_place_table_mut().add_symbol(Symbol::new(name));
        if added {
            self.current_use_def_map_mut().add_place(symbol_id.into());
        }
        symbol_id
    }

    /// Add a place to the place table and the use-def map.
    /// Return the [`ScopedPlaceId`] that uniquely identifies the place in both.
    fn add_place(&mut self, place_expr: PlaceExpr) -> ScopedPlaceId {
        let (place_id, added) = self.current_place_table_mut().add_place(place_expr);
        if added {
            self.current_use_def_map_mut().add_place(place_id);
        }
        place_id
    }

    #[track_caller]
    fn mark_place_bound(&mut self, id: ScopedPlaceId) {
        self.current_place_table_mut().mark_bound(id);
    }

    #[track_caller]
    fn mark_place_declared(&mut self, id: ScopedPlaceId) {
        self.current_place_table_mut().mark_declared(id);
    }

    #[track_caller]
    fn mark_symbol_used(&mut self, id: ScopedSymbolId) {
        self.current_place_table_mut().symbol_mut(id).mark_used();
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
        place: ScopedPlaceId,
        definition_node: impl Into<DefinitionNodeRef<'ast, 'db>> + std::fmt::Debug + Copy,
    ) -> Definition<'db> {
        let (definition, num_definitions) = self.push_additional_definition(place, definition_node);
        debug_assert_eq!(
            num_definitions, 1,
            "Attempted to create multiple `Definition`s associated with AST node {definition_node:?}"
        );
        definition
    }

    fn delete_associated_bindings(&mut self, place: ScopedPlaceId) {
        let scope = self.current_scope();
        // Don't delete associated bindings if the scope is a class scope & place is a name (it's never visible to nested scopes)
        if self.scopes[scope].kind() == ScopeKind::Class && place.is_symbol() {
            return;
        }
        for associated_place in self.place_tables[scope]
            .associated_place_ids(place)
            .iter()
            .copied()
        {
            self.use_def_maps[scope].delete_binding(associated_place.into());
        }
    }

    fn delete_binding(&mut self, place: ScopedPlaceId) {
        self.current_use_def_map_mut().delete_binding(place);
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
        place: ScopedPlaceId,
        definition_node: impl Into<DefinitionNodeRef<'ast, 'db>>,
    ) -> (Definition<'db>, usize) {
        let definition_node: DefinitionNodeRef<'ast, 'db> = definition_node.into();

        // Note `definition_node` is guaranteed to be a child of `self.module`
        let kind = definition_node.into_owned(self.module);

        let category = kind.category(self.source_type.is_stub(), self.module);
        let is_reexported = kind.is_reexported();

        let definition: Definition<'db> = Definition::new(
            self.db,
            self.file,
            self.current_scope(),
            place,
            kind,
            is_reexported,
        );

        let num_definitions = {
            let definitions = self.add_entry_for_definition_key(definition_node.key());
            definitions.push(definition);
            definitions.len()
        };

        if category.is_binding() {
            self.mark_place_bound(place);
        }
        if category.is_declaration() {
            self.mark_place_declared(place);
        }

        let use_def = self.current_use_def_map_mut();
        match category {
            DefinitionCategory::DeclarationAndBinding => {
                use_def.record_declaration_and_binding(place, definition);
                self.delete_associated_bindings(place);
            }
            DefinitionCategory::Declaration => use_def.record_declaration(place, definition),
            DefinitionCategory::Binding => {
                use_def.record_binding(place, definition);
                self.delete_associated_bindings(place);
            }
        }

        if category.is_binding() {
            if let Some(id) = place.as_symbol() {
                self.update_lazy_snapshots(id);
            }
        }

        let mut try_node_stack_manager = std::mem::take(&mut self.try_node_context_stack_manager);
        try_node_stack_manager.record_definition(self);
        self.try_node_context_stack_manager = try_node_stack_manager;

        (definition, num_definitions)
    }

    fn record_expression_narrowing_constraint(
        &mut self,
        precide_node: &ast::Expr,
    ) -> PredicateOrLiteral<'db> {
        let predicate = self.build_predicate(precide_node);
        self.record_narrowing_constraint(predicate);
        predicate
    }

    fn build_predicate(&mut self, predicate_node: &ast::Expr) -> PredicateOrLiteral<'db> {
        // Some commonly used test expressions are eagerly evaluated as `true`
        // or `false` here for performance reasons. This list does not need to
        // be exhaustive. More complex expressions will still evaluate to the
        // correct value during type-checking.
        fn resolve_to_literal(node: &ast::Expr) -> Option<bool> {
            match node {
                ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) => Some(*value),
                ast::Expr::Name(ast::ExprName { id, .. }) if id == "TYPE_CHECKING" => Some(true),
                ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Int(n),
                    ..
                }) => Some(*n != 0),
                ast::Expr::EllipsisLiteral(_) => Some(true),
                ast::Expr::NoneLiteral(_) => Some(false),
                ast::Expr::UnaryOp(ast::ExprUnaryOp {
                    op: ast::UnaryOp::Not,
                    operand,
                    ..
                }) => Some(!resolve_to_literal(operand)?),
                _ => None,
            }
        }

        let expression = self.add_standalone_expression(predicate_node);

        match resolve_to_literal(predicate_node) {
            Some(literal) => PredicateOrLiteral::Literal(literal),
            None => PredicateOrLiteral::Predicate(Predicate {
                node: PredicateNode::Expression(expression),
                is_positive: true,
            }),
        }
    }

    /// Adds a new predicate to the list of all predicates, but does not record it. Returns the
    /// predicate ID for later recording using
    /// [`SemanticIndexBuilder::record_narrowing_constraint_id`].
    fn add_predicate(&mut self, predicate: PredicateOrLiteral<'db>) -> ScopedPredicateId {
        self.current_use_def_map_mut().add_predicate(predicate)
    }

    /// Negates a predicate and adds it to the list of all predicates, does not record it.
    fn add_negated_predicate(&mut self, predicate: PredicateOrLiteral<'db>) -> ScopedPredicateId {
        self.current_use_def_map_mut()
            .add_predicate(predicate.negated())
    }

    /// Records a previously added narrowing constraint by adding it to all live bindings.
    fn record_narrowing_constraint_id(&mut self, predicate: ScopedPredicateId) {
        self.current_use_def_map_mut()
            .record_narrowing_constraint(predicate);
    }

    /// Adds and records a narrowing constraint, i.e. adds it to all live bindings.
    fn record_narrowing_constraint(&mut self, predicate: PredicateOrLiteral<'db>) {
        let use_def = self.current_use_def_map_mut();
        let predicate_id = use_def.add_predicate(predicate);
        use_def.record_narrowing_constraint(predicate_id);
    }

    /// Negates the given predicate and then adds it as a narrowing constraint to all live
    /// bindings.
    fn record_negated_narrowing_constraint(
        &mut self,
        predicate: PredicateOrLiteral<'db>,
    ) -> ScopedPredicateId {
        let id = self.add_negated_predicate(predicate);
        self.record_narrowing_constraint_id(id);
        id
    }

    /// Records that all remaining statements in the current block are unreachable.
    fn mark_unreachable(&mut self) {
        self.current_use_def_map_mut().mark_unreachable();
    }

    /// Records a reachability constraint that always evaluates to "ambiguous".
    fn record_ambiguous_reachability(&mut self) {
        self.current_use_def_map_mut()
            .record_reachability_constraint(ScopedReachabilityConstraintId::AMBIGUOUS);
    }

    /// Record a constraint that affects the reachability of the current position in the semantic
    /// index analysis. For example, if we encounter a `if test:` branch, we immediately record
    /// a `test` constraint, because if `test` later (during type checking) evaluates to `False`,
    /// we know that all statements that follow in this path of control flow will be unreachable.
    fn record_reachability_constraint(
        &mut self,
        predicate: PredicateOrLiteral<'db>,
    ) -> ScopedReachabilityConstraintId {
        let predicate_id = self.add_predicate(predicate);
        self.record_reachability_constraint_id(predicate_id)
    }

    /// Similar to [`Self::record_reachability_constraint`], but takes a [`ScopedPredicateId`].
    fn record_reachability_constraint_id(
        &mut self,
        predicate_id: ScopedPredicateId,
    ) -> ScopedReachabilityConstraintId {
        let reachability_constraint = self
            .current_reachability_constraints_mut()
            .add_atom(predicate_id);

        self.current_use_def_map_mut()
            .record_reachability_constraint(reachability_constraint);
        reachability_constraint
    }

    /// Record the negation of a given reachability constraint.
    fn record_negated_reachability_constraint(
        &mut self,
        reachability_constraint: ScopedReachabilityConstraintId,
    ) {
        let negated_constraint = self
            .current_reachability_constraints_mut()
            .add_not_constraint(reachability_constraint);
        self.current_use_def_map_mut()
            .record_reachability_constraint(negated_constraint);
    }

    fn push_assignment(&mut self, assignment: CurrentAssignment<'ast, 'db>) {
        self.current_assignments.push(assignment);
    }

    fn pop_assignment(&mut self) {
        let popped_assignment = self.current_assignments.pop();
        debug_assert!(popped_assignment.is_some());
    }

    fn current_assignment(&self) -> Option<CurrentAssignment<'ast, 'db>> {
        self.current_assignments.last().copied()
    }

    fn current_assignment_mut(&mut self) -> Option<&mut CurrentAssignment<'ast, 'db>> {
        self.current_assignments.last_mut()
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

                PatternPredicateKind::Class(
                    cls,
                    if pattern
                        .arguments
                        .patterns
                        .iter()
                        .all(ast::Pattern::is_irrefutable)
                        && pattern
                            .arguments
                            .keywords
                            .iter()
                            .all(|kw| kw.pattern.is_irrefutable())
                    {
                        ClassPatternKind::Irrefutable
                    } else {
                        ClassPatternKind::Refutable
                    },
                )
            }
            ast::Pattern::MatchOr(pattern) => {
                let predicates = pattern
                    .patterns
                    .iter()
                    .map(|pattern| self.predicate_kind(pattern))
                    .collect();
                PatternPredicateKind::Or(predicates)
            }
            ast::Pattern::MatchAs(pattern) => PatternPredicateKind::As(
                pattern
                    .pattern
                    .as_ref()
                    .map(|p| Box::new(self.predicate_kind(p))),
                pattern.name.as_ref().map(|name| name.id.clone()),
            ),
            _ => PatternPredicateKind::Unsupported,
        }
    }

    fn add_pattern_narrowing_constraint(
        &mut self,
        subject: Expression<'db>,
        pattern: &ast::Pattern,
        guard: Option<&ast::Expr>,
        previous_pattern: Option<PatternPredicate<'db>>,
    ) -> (PredicateOrLiteral<'db>, PatternPredicate<'db>) {
        // This is called for the top-level pattern of each match arm. We need to create a
        // standalone expression for each arm of a match statement, since they can introduce
        // constraints on the match subject. (Or more accurately, for the match arm's pattern,
        // since its the pattern that introduces any constraints, not the body.) Ideally, that
        // standalone expression would wrap the match arm's pattern as a whole. But a standalone
        // expression can currently only wrap an ast::Expr, which patterns are not. So, we need to
        // choose an Expr that can "stand in" for the pattern, which we can wrap in a standalone
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
            previous_pattern.map(Box::new),
        );
        let predicate = PredicateOrLiteral::Predicate(Predicate {
            node: PredicateNode::Pattern(pattern_predicate),
            is_positive: true,
        });
        self.record_narrowing_constraint(predicate);
        (predicate, pattern_predicate)
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
            AstNodeRef::new(self.module, expression_node),
            assigned_to.map(|assigned_to| AstNodeRef::new(self.module, assigned_to)),
            expression_kind,
        );
        self.expressions_by_node
            .insert(expression_node.into(), expression);
        expression
    }

    fn with_type_params(
        &mut self,
        with_scope: NodeWithScopeRef,
        type_params: Option<&'ast ast::TypeParams>,
        nested: impl FnOnce(&mut Self) -> FileScopeId,
    ) -> FileScopeId {
        if let Some(type_params) = type_params {
            self.push_scope(with_scope);

            for type_param in &type_params.type_params {
                let (name, bound, default) = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar {
                        range: _,
                        node_index: _,
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
                self.scopes_by_expression
                    .record_expression(name, self.current_scope());
                let symbol = self.add_symbol(name.id.clone());
                // TODO create Definition for PEP 695 typevars
                // note that the "bound" on the typevar is a totally different thing than whether
                // or not a name is "bound" by a typevar declaration; the latter is always true.
                self.mark_place_bound(symbol.into());
                self.mark_place_declared(symbol.into());
                if let Some(bounds) = bound {
                    self.visit_expr(bounds);
                }
                if let Some(default) = default {
                    self.visit_expr(default);
                }
                match type_param {
                    ast::TypeParam::TypeVar(node) => self.add_definition(symbol.into(), node),
                    ast::TypeParam::ParamSpec(node) => self.add_definition(symbol.into(), node),
                    ast::TypeParam::TypeVarTuple(node) => self.add_definition(symbol.into(), node),
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
        generators: &'ast [ast::Comprehension],
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

        for if_expr in &generator.ifs {
            self.visit_expr(if_expr);
            self.record_expression_narrowing_constraint(if_expr);
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

            for if_expr in &generator.ifs {
                self.visit_expr(if_expr);
                self.record_expression_narrowing_constraint(if_expr);
            }
        }

        visit_outer_elt(self);
        self.pop_scope();
    }

    fn declare_parameters(&mut self, parameters: &'ast ast::Parameters) {
        for parameter in parameters.iter_non_variadic_params() {
            self.declare_parameter(parameter);
        }
        if let Some(vararg) = parameters.vararg.as_ref() {
            let symbol = self.add_symbol(vararg.name.id().clone());
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_parameter();
            self.add_definition(
                symbol.into(),
                DefinitionNodeRef::VariadicPositionalParameter(vararg),
            );
        }
        if let Some(kwarg) = parameters.kwarg.as_ref() {
            let symbol = self.add_symbol(kwarg.name.id().clone());
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_parameter();
            self.add_definition(
                symbol.into(),
                DefinitionNodeRef::VariadicKeywordParameter(kwarg),
            );
        }
    }

    fn declare_parameter(&mut self, parameter: &'ast ast::ParameterWithDefault) {
        let symbol = self.add_symbol(parameter.name().id().clone());

        let definition = self.add_definition(symbol.into(), parameter);

        self.current_place_table_mut()
            .symbol_mut(symbol)
            .mark_parameter();

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
        unpackable: &Unpackable<'ast>,
        target: &'ast ast::Expr,
        value: Expression<'db>,
    ) {
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
                    // Note `target` belongs to the `self.module` tree
                    AstNodeRef::new(self.module, target),
                    UnpackValue::new(unpackable.kind(), value),
                ));
                Some(unpackable.as_current_assignment(unpack))
            }
            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
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
        self.visit_body(self.module.suite());

        // Pop the root scope
        self.pop_scope();
        self.sweep_nonlocal_lazy_snapshots();
        assert!(self.scope_stack.is_empty());

        assert_eq!(&self.current_assignments, &[]);

        for scope in &self.scopes {
            if let Some(parent) = scope.parent() {
                self.use_def_maps[parent]
                    .reachability_constraints
                    .mark_used(scope.reachability());
            }
        }

        let mut place_tables: IndexVec<_, _> = self
            .place_tables
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
        place_tables.shrink_to_fit();
        use_def_maps.shrink_to_fit();
        ast_ids.shrink_to_fit();
        self.definitions_by_node.shrink_to_fit();

        self.scope_ids_by_scope.shrink_to_fit();
        self.scopes_by_node.shrink_to_fit();
        self.generator_functions.shrink_to_fit();
        self.enclosing_snapshots.shrink_to_fit();

        SemanticIndex {
            place_tables,
            scopes: self.scopes,
            definitions_by_node: self.definitions_by_node,
            expressions_by_node: self.expressions_by_node,
            scope_ids_by_scope: self.scope_ids_by_scope,
            ast_ids,
            scopes_by_expression: self.scopes_by_expression.build(),
            scopes_by_node: self.scopes_by_node,
            use_def_maps,
            imported_modules: Arc::new(self.imported_modules),
            has_future_annotations: self.has_future_annotations,
            enclosing_snapshots: self.enclosing_snapshots,
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
            .get_or_init(|| source_text(self.db, self.file))
    }
}

impl<'ast> Visitor<'ast> for SemanticIndexBuilder<'_, 'ast> {
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
                    node_index: _,
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

                        builder.visit_body(body);

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
                let use_id = self.current_ast_ids().record_use(name);
                self.current_use_def_map_mut().record_use(
                    symbol.into(),
                    use_id,
                    NodeKey::from_node(name),
                );

                self.add_definition(symbol.into(), function_def);
                self.mark_symbol_used(symbol);
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
                self.add_definition(symbol.into(), class);
            }
            ast::Stmt::TypeAlias(type_alias) => {
                let symbol = self.add_symbol(
                    type_alias
                        .name
                        .as_name_expr()
                        .map(|name| name.id.clone())
                        .unwrap_or("<unknown>".into()),
                );
                self.add_definition(symbol.into(), type_alias);
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
                        self.scopes_by_expression
                            .record_expression(asname, self.current_scope());
                        (asname.id.clone(), asname.id == alias.name.id)
                    } else {
                        (Name::new(alias.name.id.split('.').next().unwrap()), false)
                    };

                    let symbol = self.add_symbol(symbol_name);
                    self.add_definition(
                        symbol.into(),
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

                // If we see:
                //
                // * `from .x.y import z` (or `from whatever.thispackage.x.y`)
                // * And we are in an `__init__.py(i)` (hereafter `thispackage`)
                // * And this is the first time we've seen `from .x` in this module
                // * And we're in the global scope
                //
                // We introduce a local definition `x = <module 'thispackage.x'>` that occurs
                // before the `z = ...` declaration the import introduces. This models the fact
                // that the *first* time that you import 'thispackage.x' the python runtime creates
                // `x` as a variable in the global scope of `thispackage`.
                //
                // This is not a perfect simulation of actual runtime behaviour for *various*
                // reasons but it works well for most practical purposes. In particular it's nice
                // that `x` can be freely overwritten, and that we don't assume that an import
                // in one function is visible in another function.
                let mut is_self_import = false;
                if self.file.is_package(self.db)
                    && let Ok(module_name) = ModuleName::from_identifier_parts(
                        self.db,
                        self.file,
                        node.module.as_deref(),
                        node.level,
                    )
                    && let Ok(thispackage) = ModuleName::package_for_file(self.db, self.file)
                {
                    // Record whether this is equivalent to `from . import ...`
                    is_self_import = module_name == thispackage;

                    if node.module.is_some()
                        && let Some(relative_submodule) = module_name.relative_to(&thispackage)
                        && let Some(direct_submodule) = relative_submodule.components().next()
                        && !self.seen_submodule_imports.contains(direct_submodule)
                        && self.current_scope().is_global()
                    {
                        self.seen_submodule_imports
                            .insert(direct_submodule.to_owned());

                        let direct_submodule_name = Name::new(direct_submodule);
                        let symbol = self.add_symbol(direct_submodule_name);
                        self.add_definition(
                            symbol.into(),
                            ImportFromSubmoduleDefinitionNodeRef { node },
                        );
                    }
                }

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

                        let Some(module) = resolve_module(self.db, self.file, &module_name) else {
                            continue;
                        };

                        let Some(referenced_module) = module.file(self.db) else {
                            continue;
                        };

                        // In order to understand the reachability of definitions created by a `*` import,
                        // we need to know the reachability of the global-scope definitions in the
                        // `referenced_module` the symbols imported from. Much like predicates for `if`
                        // statements can only have their reachability constraints resolved at type-inference
                        // time, the reachability of these global-scope definitions in the external module
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

                            let star_import_predicate = self.add_predicate(star_import.into());

                            let associated_member_ids = self.place_tables[self.current_scope()]
                                .associated_place_ids(ScopedPlaceId::Symbol(symbol_id));
                            let pre_definition = self
                                .current_use_def_map()
                                .single_symbol_snapshot(symbol_id, associated_member_ids);

                            let pre_definition_reachability =
                                self.current_use_def_map().reachability;

                            // Temporarily modify the reachability to include the star import predicate,
                            // in order for the new definition to pick it up.
                            let reachability_constraints =
                                &mut self.current_use_def_map_mut().reachability_constraints;
                            let star_import_reachability =
                                reachability_constraints.add_atom(star_import_predicate);
                            let definition_reachability = reachability_constraints
                                .add_and_constraint(
                                    pre_definition_reachability,
                                    star_import_reachability,
                                );
                            self.current_use_def_map_mut().reachability = definition_reachability;

                            self.push_additional_definition(symbol_id.into(), node_ref);

                            self.current_use_def_map_mut()
                                .record_and_negate_star_import_reachability_constraint(
                                    star_import_reachability,
                                    symbol_id,
                                    pre_definition,
                                );

                            // Restore the reachability to its pre-definition state
                            self.current_use_def_map_mut().reachability =
                                pre_definition_reachability;
                        }

                        continue;
                    }

                    let (symbol_name, is_reexported) = if let Some(asname) = &alias.asname {
                        self.scopes_by_expression
                            .record_expression(asname, self.current_scope());
                        // It's re-exported if it's `from ... import x as x`
                        (&asname.id, asname.id == alias.name.id)
                    } else {
                        // As a non-standard rule to handle stubs in the wild, we consider
                        // `from . import x` and `from whatever.thispackage import x` in an
                        // `__init__.pyi` to re-export `x` (as long as it wasn't renamed)
                        (&alias.name.id, is_self_import)
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
                        symbol.into(),
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
                node_index: _,
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
                // flow states and simplification of reachability constraints, since there is no way
                // of getting out of that `msg` branch. We simply restore to the post-test state.

                self.visit_expr(test);
                let predicate = self.build_predicate(test);

                if let Some(msg) = msg {
                    let post_test = self.flow_snapshot();
                    let negated_predicate = predicate.negated();
                    self.record_narrowing_constraint(negated_predicate);
                    self.record_reachability_constraint(negated_predicate);
                    self.visit_expr(msg);
                    self.flow_restore(post_test);
                }

                self.record_narrowing_constraint(predicate);
                self.record_reachability_constraint(predicate);
            }

            ast::Stmt::Assign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);

                self.visit_expr(&node.value);

                // Optimization for the common case: if there's just one target, and it's not an
                // unpacking, and the target is a simple name, we don't need the RHS to be a
                // standalone expression at all.
                if let [target] = &node.targets[..]
                    && target.is_name_expr()
                {
                    self.push_assignment(CurrentAssignment::Assign { node, unpack: None });
                    self.visit_expr(target);
                    self.pop_assignment();
                } else {
                    let value = self.add_standalone_assigned_expression(&node.value, node);

                    for target in &node.targets {
                        self.add_unpackable_assignment(&Unpackable::Assign(node), target, value);
                    }
                }
            }
            ast::Stmt::AnnAssign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);
                self.visit_expr(&node.annotation);
                if let Some(value) = &node.value {
                    self.visit_expr(value);
                    if self.is_method_or_eagerly_executed_in_method().is_some() {
                        // Record the right-hand side of the assignment as a standalone expression
                        // if we're inside a method. This allows type inference to infer the type
                        // of the value for annotated assignments like `self.CONSTANT: Final = 1`,
                        // where the type itself is not part of the annotation.
                        self.add_standalone_expression(value);
                    }
                }

                if let ast::Expr::Name(name) = &*node.target {
                    let symbol_id = self.add_symbol(name.id.clone());
                    let symbol = self.current_place_table().symbol(symbol_id);
                    // Check whether the variable has been declared global.
                    if symbol.is_global() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::AnnotatedGlobal(name.id.as_str().into()),
                            range: name.range,
                            python_version: self.python_version,
                        });
                    }
                    // Check whether the variable has been declared nonlocal.
                    if symbol.is_nonlocal() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::AnnotatedNonlocal(
                                name.id.as_str().into(),
                            ),
                            range: name.range,
                            python_version: self.python_version,
                        });
                    }
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
                    node_index: _,
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
                let mut last_reachability_constraint =
                    self.record_reachability_constraint(last_predicate);

                let is_outer_block_in_type_checking = self.in_type_checking_block;

                let if_block_in_type_checking = is_if_type_checking(&node.test);

                // Track if we're in a chain that started with "not TYPE_CHECKING"
                let mut is_in_not_type_checking_chain = is_if_not_type_checking(&node.test);

                self.in_type_checking_block =
                    if_block_in_type_checking || is_outer_block_in_type_checking;

                self.visit_body(&node.body);

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
                    self.record_negated_reachability_constraint(last_reachability_constraint);

                    if let Some(elif_test) = clause_test {
                        self.visit_expr(elif_test);
                        // A test expression is evaluated whether the branch is taken or not
                        no_branch_taken = self.flow_snapshot();

                        last_predicate = self.record_expression_narrowing_constraint(elif_test);

                        last_reachability_constraint =
                            self.record_reachability_constraint(last_predicate);
                    }

                    // Determine if this clause is in type checking context
                    let clause_in_type_checking = if let Some(elif_test) = clause_test {
                        if is_if_type_checking(elif_test) {
                            // This block has "TYPE_CHECKING" condition
                            true
                        } else if is_if_not_type_checking(elif_test) {
                            // This block has "not TYPE_CHECKING" condition so we update the chain state for future blocks
                            is_in_not_type_checking_chain = true;
                            false
                        } else {
                            // This block has some other condition
                            // It's in type checking only if we're in a "not TYPE_CHECKING" chain
                            is_in_not_type_checking_chain
                        }
                    } else {
                        is_in_not_type_checking_chain
                    };

                    self.in_type_checking_block = clause_in_type_checking;

                    self.visit_body(clause_body);
                }

                for post_clause_state in post_clauses {
                    self.flow_merge(post_clause_state);
                }

                self.in_type_checking_block = is_outer_block_in_type_checking;
            }
            ast::Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _,
                node_index: _,
            }) => {
                self.visit_expr(test);

                let pre_loop = self.flow_snapshot();
                let predicate = self.record_expression_narrowing_constraint(test);
                self.record_reachability_constraint(predicate);

                let outer_loop = self.push_loop();
                self.visit_body(body);
                let this_loop = self.pop_loop(outer_loop);

                // We execute the `else` branch once the condition evaluates to false. This could
                // happen without ever executing the body, if the condition is false the first time
                // it's tested. Or it could happen if a _later_ evaluation of the condition yields
                // false. So we merge in the pre-loop state here into the post-body state:

                self.flow_merge(pre_loop);

                // The `else` branch can only be reached if the loop condition *can* be false. To
                // model this correctly, we need a second copy of the while condition constraint,
                // since the first and later evaluations might produce different results. We would
                // otherwise simplify `predicate AND ~predicate` to `False`.
                let later_predicate_id = self.current_use_def_map_mut().add_predicate(predicate);
                let later_reachability_constraint = self
                    .current_reachability_constraints_mut()
                    .add_atom(later_predicate_id);
                self.record_negated_reachability_constraint(later_reachability_constraint);

                self.record_negated_narrowing_constraint(predicate);

                self.visit_body(orelse);

                // Breaking out of a while loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                for break_state in this_loop.break_states {
                    self.flow_merge(break_state);
                }
            }
            ast::Stmt::With(ast::StmtWith {
                items,
                body,
                is_async,
                ..
            }) => {
                for item @ ast::WithItem {
                    range: _,
                    node_index: _,
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
                    node_index: _,
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

                self.record_ambiguous_reachability();

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
                node_index: _,
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
                let mut previous_pattern: Option<PatternPredicate<'_>> = None;

                for (i, case) in cases.iter().enumerate() {
                    self.current_match_case = Some(CurrentMatchCase::new(&case.pattern));
                    self.visit_pattern(&case.pattern);
                    self.current_match_case = None;
                    // unlike in [Stmt::If], we don't reset [no_case_matched]
                    // here because the effects of visiting a pattern is binding
                    // symbols, and this doesn't occur unless the pattern
                    // actually matches
                    let (match_predicate, match_pattern_predicate) = self
                        .add_pattern_narrowing_constraint(
                            subject_expr,
                            &case.pattern,
                            case.guard.as_deref(),
                            previous_pattern,
                        );
                    previous_pattern = Some(match_pattern_predicate);
                    let reachability_constraint =
                        self.record_reachability_constraint(match_predicate);

                    let match_success_guard_failure = case.guard.as_ref().map(|guard| {
                        let guard_expr = self.add_standalone_expression(guard);
                        // We could also add the guard expression as a reachability constraint, but
                        // it seems unlikely that both the case predicate as well as the guard are
                        // statically known conditions, so we currently don't model that.
                        self.record_ambiguous_reachability();
                        self.visit_expr(guard);
                        let post_guard_eval = self.flow_snapshot();
                        let predicate = PredicateOrLiteral::Predicate(Predicate {
                            node: PredicateNode::Expression(guard_expr),
                            is_positive: true,
                        });
                        self.record_negated_narrowing_constraint(predicate);
                        let match_success_guard_failure = self.flow_snapshot();
                        self.flow_restore(post_guard_eval);
                        self.record_narrowing_constraint(predicate);
                        match_success_guard_failure
                    });

                    self.visit_body(&case.body);

                    post_case_snapshots.push(self.flow_snapshot());

                    if i != cases.len() - 1 || !has_catchall {
                        // We need to restore the state after each case, but not after the last
                        // one. The last one will just become the state that we merge the other
                        // snapshots into.
                        self.flow_restore(no_case_matched.clone());
                        self.record_negated_narrowing_constraint(match_predicate);
                        self.record_negated_reachability_constraint(reachability_constraint);
                        if let Some(match_success_guard_failure) = match_success_guard_failure {
                            self.flow_merge(match_success_guard_failure);
                        } else {
                            assert!(case.guard.is_none());
                        }
                    } else {
                        debug_assert!(match_success_guard_failure.is_none());
                        debug_assert!(case.guard.is_none());
                    }

                    no_case_matched = self.flow_snapshot();
                }

                for post_clause_state in post_case_snapshots {
                    self.flow_merge(post_clause_state);
                }
            }
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range: _,
                node_index: _,
            }) => {
                self.record_ambiguous_reachability();

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
                            node_index: _,
                        } = except_handler;

                        if let Some(handled_exceptions) = handled_exceptions {
                            self.visit_expr(handled_exceptions);
                        }

                        // If `handled_exceptions` above was `None`, it's something like `except as e:`,
                        // which is invalid syntax. However, it's still pretty obvious here that the user
                        // *wanted* `e` to be bound, so we should still create a definition here nonetheless.
                        let symbol = if let Some(symbol_name) = symbol_name {
                            let symbol = self.add_symbol(symbol_name.id.clone());

                            self.add_definition(
                                symbol.into(),
                                DefinitionNodeRef::ExceptHandler(ExceptHandlerDefinitionNodeRef {
                                    handler: except_handler,
                                    is_star: *is_star,
                                }),
                            );
                            Some(symbol)
                        } else {
                            None
                        };

                        self.visit_body(handler_body);
                        // The caught exception is cleared at the end of the except clause
                        if let Some(symbol) = symbol {
                            self.delete_binding(symbol.into());
                        }
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
            ast::Stmt::Global(ast::StmtGlobal {
                range: _,
                node_index: _,
                names,
            }) => {
                for name in names {
                    self.scopes_by_expression
                        .record_expression(name, self.current_scope());
                    let symbol_id = self.add_symbol(name.id.clone());
                    let symbol = self.current_place_table().symbol(symbol_id);
                    // Check whether the variable has already been accessed in this scope.
                    if (symbol.is_bound() || symbol.is_declared() || symbol.is_used())
                        && !symbol.is_parameter()
                    {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::LoadBeforeGlobalDeclaration {
                                name: name.to_string(),
                                start: name.range.start(),
                            },
                            range: name.range,
                            python_version: self.python_version,
                        });
                    }
                    // Check whether the variable has also been declared nonlocal.
                    if symbol.is_nonlocal() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::NonlocalAndGlobal(name.to_string()),
                            range: name.range,
                            python_version: self.python_version,
                        });
                    }
                    self.current_place_table_mut()
                        .symbol_mut(symbol_id)
                        .mark_global();
                }
                walk_stmt(self, stmt);
            }
            ast::Stmt::Nonlocal(ast::StmtNonlocal {
                range: _,
                node_index: _,
                names,
            }) => {
                for name in names {
                    self.scopes_by_expression
                        .record_expression(name, self.current_scope());
                    let symbol_id = self.add_symbol(name.id.clone());
                    let symbol = self.current_place_table().symbol(symbol_id);
                    // Check whether the variable has already been accessed in this scope.
                    if symbol.is_bound() || symbol.is_declared() || symbol.is_used() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::LoadBeforeNonlocalDeclaration {
                                name: name.to_string(),
                                start: name.range.start(),
                            },
                            range: name.range,
                            python_version: self.python_version,
                        });
                    }
                    // Check whether the variable has also been declared global.
                    if symbol.is_global() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::NonlocalAndGlobal(name.to_string()),
                            range: name.range,
                            python_version: self.python_version,
                        });
                    }
                    // The variable is required to exist in an enclosing scope, but that definition
                    // might come later. For example, this is example legal, but we can't check
                    // that here, because we haven't gotten to `x = 1`:
                    // ```py
                    // def f():
                    //     def g():
                    //         nonlocal x
                    //     x = 1
                    // ```
                    self.current_place_table_mut()
                        .symbol_mut(symbol_id)
                        .mark_nonlocal();
                }
                walk_stmt(self, stmt);
            }
            ast::Stmt::Delete(ast::StmtDelete {
                targets,
                range: _,
                node_index: _,
            }) => {
                // We will check the target expressions and then delete them.
                walk_stmt(self, stmt);
                for target in targets {
                    if let Some(mut target) = PlaceExpr::try_from_expr(target) {
                        if let PlaceExpr::Symbol(symbol) = &mut target {
                            // `del x` behaves like an assignment in that it forces all references
                            // to `x` in the current scope (including *prior* references) to refer
                            // to the current scope's binding (unless `x` is declared `global` or
                            // `nonlocal`). For example, this is an UnboundLocalError at runtime:
                            //
                            // ```py
                            // x = 1
                            // def foo():
                            //     print(x)  # can't refer to global `x`
                            //     if False:
                            //         del x
                            // foo()
                            // ```
                            symbol.mark_bound();
                            symbol.mark_used();
                        }

                        let place_id = self.add_place(target);
                        self.delete_binding(place_id);
                    }
                }
            }
            ast::Stmt::Expr(ast::StmtExpr {
                value,
                range: _,
                node_index: _,
            }) => {
                if self.in_module_scope() {
                    if let Some(expr) = dunder_all_extend_argument(value) {
                        self.add_standalone_expression(expr);
                    }
                }

                self.visit_expr(value);

                // If the statement is a call, it could possibly be a call to a function
                // marked with `NoReturn` (for example, `sys.exit()`). In this case, we use a special
                // kind of constraint to mark the following code as unreachable.
                //
                // Ideally, these constraints should be added for every call expression, even those in
                // sub-expressions and in the module-level scope. But doing so makes the number of
                // such constraints so high that it significantly degrades performance. We thus cut
                // scope here and add these constraints only at statement level function calls,
                // like `sys.exit()`, and not within sub-expression like `3 + sys.exit()` etc.
                //
                // We also only add these inside function scopes, since considering module-level
                // constraints can affect the type of imported symbols, leading to a lot more
                // work in third-party code.
                if let ast::Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
                    if !self.source_type.is_stub() && self.in_function_scope() {
                        let callable = self.add_standalone_expression(func);
                        let call_expr = self.add_standalone_expression(value.as_ref());

                        let predicate = Predicate {
                            node: PredicateNode::ReturnsNever(CallableAndCallExpr {
                                callable,
                                call_expr,
                            }),
                            is_positive: false,
                        };
                        self.record_reachability_constraint(PredicateOrLiteral::Predicate(
                            predicate,
                        ));
                    }
                }
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        self.with_semantic_checker(|semantic, context| semantic.visit_expr(expr, context));

        self.scopes_by_expression
            .record_expression(expr, self.current_scope());

        let node_key = NodeKey::from_node(expr);

        match expr {
            ast::Expr::Name(ast::ExprName { ctx, .. })
            | ast::Expr::Attribute(ast::ExprAttribute { ctx, .. })
            | ast::Expr::Subscript(ast::ExprSubscript { ctx, .. }) => {
                if let Some(mut place_expr) = PlaceExpr::try_from_expr(expr) {
                    if let Some(method_scope_id) = self.is_method_or_eagerly_executed_in_method() {
                        if let PlaceExpr::Member(member) = &mut place_expr {
                            if member.is_instance_attribute_candidate() {
                                // We specifically mark attribute assignments to the first parameter of a method,
                                // i.e. typically `self` or `cls`.
                                // However, we must check that the symbol hasn't been shadowed by an intermediate
                                // scope (e.g., a comprehension variable: `for self in [...]`).
                                let accessed_object_refers_to_first_parameter =
                                    self.current_first_parameter_name.is_some_and(|first| {
                                        member.symbol_name() == first
                                            && !self.is_symbol_bound_in_intermediate_eager_scopes(
                                                first,
                                                method_scope_id,
                                            )
                                    });

                                if accessed_object_refers_to_first_parameter {
                                    member.mark_instance_attribute();
                                }
                            }
                        }
                    }

                    let (is_use, is_definition) = match (ctx, self.current_assignment()) {
                        (ast::ExprContext::Store, Some(CurrentAssignment::AugAssign(_))) => {
                            // For augmented assignment, the target expression is also used.
                            (true, true)
                        }
                        (ast::ExprContext::Load, _) => (true, false),
                        (ast::ExprContext::Store, _) => (false, true),
                        (ast::ExprContext::Del, _) => (true, true),
                        (ast::ExprContext::Invalid, _) => (false, false),
                    };
                    let place_id = self.add_place(place_expr);

                    if is_use {
                        if let ScopedPlaceId::Symbol(symbol_id) = place_id {
                            self.mark_symbol_used(symbol_id);
                        }
                        let use_id = self.current_ast_ids().record_use(expr);
                        self.current_use_def_map_mut()
                            .record_use(place_id, use_id, node_key);
                    }

                    if is_definition {
                        match self.current_assignment() {
                            Some(CurrentAssignment::Assign { node, unpack }) => {
                                self.add_definition(
                                    place_id,
                                    AssignmentDefinitionNodeRef {
                                        unpack,
                                        value: &node.value,
                                        target: expr,
                                    },
                                );
                            }
                            Some(CurrentAssignment::AnnAssign(ann_assign)) => {
                                self.add_standalone_type_expression(&ann_assign.annotation);
                                self.add_definition(
                                    place_id,
                                    AnnotatedAssignmentDefinitionNodeRef {
                                        node: ann_assign,
                                        annotation: &ann_assign.annotation,
                                        value: ann_assign.value.as_deref(),
                                        target: expr,
                                    },
                                );
                            }
                            Some(CurrentAssignment::AugAssign(aug_assign)) => {
                                self.add_definition(place_id, aug_assign);
                            }
                            Some(CurrentAssignment::For { node, unpack }) => {
                                self.add_definition(
                                    place_id,
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
                                self.add_definition(place_id, named);
                            }
                            Some(CurrentAssignment::Comprehension {
                                unpack,
                                node,
                                first,
                            }) => {
                                self.add_definition(
                                    place_id,
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
                                    place_id,
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
                }

                // Track reachability of attribute expressions to silence `unresolved-attribute`
                // diagnostics in unreachable code.
                if expr.is_attribute_expr() {
                    self.current_use_def_map_mut()
                        .record_node_reachability(node_key);
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
                let post_body = self.flow_snapshot();
                self.flow_restore(pre_if);

                self.record_negated_narrowing_constraint(predicate);
                self.record_negated_reachability_constraint(reachability_constraint);
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
            ast::Expr::BoolOp(ast::ExprBoolOp {
                values,
                range: _,
                node_index: _,
                op,
            }) => {
                let mut snapshots = vec![];
                let mut reachability_constraints = vec![];

                for (index, value) in values.iter().enumerate() {
                    for id in &reachability_constraints {
                        self.current_use_def_map_mut()
                            .record_reachability_constraint(*id); // TODO: nicer API
                    }

                    self.visit_expr(value);

                    // For the last value, we don't need to model control flow. There is no short-circuiting
                    // anymore.
                    if index < values.len() - 1 {
                        let predicate = self.build_predicate(value);
                        let predicate_id = match op {
                            ast::BoolOp::And => self.add_predicate(predicate),
                            ast::BoolOp::Or => self.add_negated_predicate(predicate),
                        };
                        let reachability_constraint = self
                            .current_reachability_constraints_mut()
                            .add_atom(predicate_id);

                        let after_expr = self.flow_snapshot();

                        // We first model the short-circuiting behavior. We take the short-circuit
                        // path here if all of the previous short-circuit paths were not taken, so
                        // we record all previously existing reachability constraints, and negate the
                        // one for the current expression.

                        self.record_negated_reachability_constraint(reachability_constraint);
                        snapshots.push(self.flow_snapshot());

                        // Then we model the non-short-circuiting behavior. Here, we need to delay
                        // the application of the reachability constraint until after the expression
                        // has been evaluated, so we only push it onto the stack here.
                        self.flow_restore(after_expr);
                        self.record_narrowing_constraint_id(predicate_id);
                        reachability_constraints.push(reachability_constraint);
                    }
                }

                for snapshot in snapshots {
                    self.flow_merge(snapshot);
                }
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
            node_index: _,
        }) = pattern
        {
            let symbol = self.add_symbol(name.id().clone());
            let state = self.current_match_case.as_ref().unwrap();
            self.add_definition(
                symbol.into(),
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
                symbol.into(),
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

impl SemanticSyntaxContext for SemanticIndexBuilder<'_, '_> {
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

    // We handle the one syntax error that relies on this method (`NonlocalWithoutBinding`) directly
    // in `TypeInferenceBuilder::infer_nonlocal_statement`, so this just returns `true`.
    fn has_nonlocal_binding(&self, _name: &str) -> bool {
        true
    }

    fn in_async_context(&self) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_info.file_scope_id];
            match scope.kind() {
                ScopeKind::Class | ScopeKind::Lambda => return false,
                ScopeKind::Function => {
                    return scope.node().expect_function().node(self.module).is_async;
                }
                ScopeKind::Comprehension
                | ScopeKind::Module
                | ScopeKind::TypeAlias
                | ScopeKind::TypeParams => {}
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
                    if matches!(scope.node(), NodeWithScopeKind::GeneratorExpression(_)) =>
                {
                    return true;
                }
                ScopeKind::Comprehension
                | ScopeKind::Module
                | ScopeKind::TypeAlias
                | ScopeKind::TypeParams => {}
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
                ScopeKind::Module | ScopeKind::TypeAlias | ScopeKind::TypeParams => {}
            }
        }
        false
    }

    fn in_sync_comprehension(&self) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_info.file_scope_id];
            let generators = match scope.node() {
                NodeWithScopeKind::ListComprehension(node) => &node.node(self.module).generators,
                NodeWithScopeKind::SetComprehension(node) => &node.node(self.module).generators,
                NodeWithScopeKind::DictComprehension(node) => &node.node(self.module).generators,
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

    fn in_generator_context(&self) -> bool {
        for scope_info in &self.scope_stack {
            let scope = &self.scopes[scope_info.file_scope_id];
            if matches!(scope.node(), NodeWithScopeKind::GeneratorExpression(_)) {
                return true;
            }
        }
        false
    }

    fn in_notebook(&self) -> bool {
        self.source_text().is_notebook()
    }

    fn report_semantic_error(&self, error: SemanticSyntaxError) {
        if self.db.should_check_file(self.file) {
            self.semantic_syntax_errors.borrow_mut().push(error);
        }
    }

    fn in_loop_context(&self) -> bool {
        self.current_scope_info().current_loop.is_some()
    }

    fn is_bound_parameter(&self, name: &str) -> bool {
        self.scopes[self.current_scope()]
            .node()
            .as_function()
            .is_some_and(|func| func.node(self.module).parameters.includes(name))
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum CurrentAssignment<'ast, 'db> {
    Assign {
        node: &'ast ast::StmtAssign,
        unpack: Option<(UnpackPosition, Unpack<'db>)>,
    },
    AnnAssign(&'ast ast::StmtAnnAssign),
    AugAssign(&'ast ast::StmtAugAssign),
    For {
        node: &'ast ast::StmtFor,
        unpack: Option<(UnpackPosition, Unpack<'db>)>,
    },
    Named(&'ast ast::ExprNamed),
    Comprehension {
        node: &'ast ast::Comprehension,
        first: bool,
        unpack: Option<(UnpackPosition, Unpack<'db>)>,
    },
    WithItem {
        item: &'ast ast::WithItem,
        is_async: bool,
        unpack: Option<(UnpackPosition, Unpack<'db>)>,
    },
}

impl CurrentAssignment<'_, '_> {
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

impl<'ast> From<&'ast ast::StmtAnnAssign> for CurrentAssignment<'ast, '_> {
    fn from(value: &'ast ast::StmtAnnAssign) -> Self {
        Self::AnnAssign(value)
    }
}

impl<'ast> From<&'ast ast::StmtAugAssign> for CurrentAssignment<'ast, '_> {
    fn from(value: &'ast ast::StmtAugAssign) -> Self {
        Self::AugAssign(value)
    }
}

impl<'ast> From<&'ast ast::ExprNamed> for CurrentAssignment<'ast, '_> {
    fn from(value: &'ast ast::ExprNamed) -> Self {
        Self::Named(value)
    }
}

#[derive(Debug, PartialEq)]
struct CurrentMatchCase<'ast> {
    /// The pattern that's part of the current match case.
    pattern: &'ast ast::Pattern,

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

enum Unpackable<'ast> {
    Assign(&'ast ast::StmtAssign),
    For(&'ast ast::StmtFor),
    WithItem {
        item: &'ast ast::WithItem,
        is_async: bool,
    },
    Comprehension {
        first: bool,
        node: &'ast ast::Comprehension,
    },
}

impl<'ast> Unpackable<'ast> {
    const fn kind(&self) -> UnpackKind {
        match self {
            Unpackable::Assign(_) => UnpackKind::Assign,
            Unpackable::For(ast::StmtFor { is_async, .. }) => UnpackKind::Iterable {
                mode: EvaluationMode::from_is_async(*is_async),
            },
            Unpackable::Comprehension {
                node: ast::Comprehension { is_async, .. },
                ..
            } => UnpackKind::Iterable {
                mode: EvaluationMode::from_is_async(*is_async),
            },
            Unpackable::WithItem { is_async, .. } => UnpackKind::ContextManager {
                mode: EvaluationMode::from_is_async(*is_async),
            },
        }
    }

    fn as_current_assignment<'db>(
        &self,
        unpack: Option<Unpack<'db>>,
    ) -> CurrentAssignment<'ast, 'db> {
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
                node_index: _,
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

/// Builds an interval-map that matches expressions (by their node index) to their enclosing scopes.
///
/// The interval map is built in a two-step process because the expression ids are assigned in source order,
/// but we visit the expressions in semantic order. Few expressions are registered out of order.
///
/// 1. build a point vector that maps node indices to their corresponding file scopes.
/// 2. Sort the expressions by their starting id. Then condense the point vector into an interval map
///    by collapsing adjacent node indices with the same scope
///    into a single interval.
struct ExpressionsScopeMapBuilder {
    expression_and_scope: Vec<(NodeIndex, FileScopeId)>,
}

impl ExpressionsScopeMapBuilder {
    fn new() -> Self {
        Self {
            expression_and_scope: vec![],
        }
    }

    fn record_expression(&mut self, expression: &impl HasTrackedScope, scope: FileScopeId) {
        self.expression_and_scope
            .push((expression.node_index().load(), scope));
    }

    fn build(mut self) -> ExpressionsScopeMap {
        self.expression_and_scope
            .sort_unstable_by_key(|(index, _)| *index);

        let mut iter = self.expression_and_scope.into_iter();
        let Some(first) = iter.next() else {
            return ExpressionsScopeMap::default();
        };

        let mut interval_map = Vec::new();

        let mut current_scope = first.1;
        let mut range = first.0..=first.0;

        for (index, scope) in iter {
            if scope == current_scope {
                range = *range.start()..=index;
                continue;
            }

            interval_map.push((range, current_scope));

            current_scope = scope;
            range = index..=index;
        }

        interval_map.push((range, current_scope));

        ExpressionsScopeMap(interval_map.into_boxed_slice())
    }
}

/// Returns if the expression is a `TYPE_CHECKING` expression.
fn is_if_type_checking(expr: &ast::Expr) -> bool {
    fn is_dotted_name(expr: &ast::Expr) -> bool {
        match expr {
            ast::Expr::Name(_) => true,
            ast::Expr::Attribute(ast::ExprAttribute { value, .. }) => is_dotted_name(value),
            _ => false,
        }
    }

    match expr {
        ast::Expr::Name(ast::ExprName { id, .. }) => id == "TYPE_CHECKING",
        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            attr == "TYPE_CHECKING" && is_dotted_name(value)
        }
        _ => false,
    }
}

/// Returns if the expression is a `not TYPE_CHECKING` expression.
fn is_if_not_type_checking(expr: &ast::Expr) -> bool {
    matches!(
        expr,
        ast::Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::Not,
            operand,
            ..
        }) if is_if_type_checking(operand)
    )
}
