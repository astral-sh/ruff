use std::cell::{OnceCell, RefCell};
use std::hash::BuildHasher;
use std::sync::Arc;

use except_handlers::{ExceptionContextStackManager, ExceptionHandlers};
use itertools::Itertools;
use ruff_python_ast::helpers::{Truthiness, any_over_expr, is_dotted_name};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use ruff_db::parsed::ParsedModuleRef;

use ruff_db::source::{SourceText, source_text};
use ruff_index::IndexVec;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_keyword, walk_pattern, walk_stmt};
use ruff_python_ast::{
    self as ast, AnyParameter, AstNode, BoolOp, CmpOp, Comprehension, ElifElseClause,
    ExceptHandlerKind, Expr, ExprBoolOp, ExprContext, ExprIf, ExprKind, ExprLambda, ExprNamed,
    Identifier, Keyword, MAX_REAL_INDEX, MappingPatternItem, NodeIndex, NodeList, Number,
    ParameterWithDefault, Parameters, Pattern, PatternKeyword, PatternKind, PySourceType,
    PythonVersion, Stmt, StmtAnnAssign, StmtAssign, StmtAugAssign, StmtFor, StmtKind,
    TypeParamKind, TypeParams, UnaryOp, WithItem,
};
use ruff_python_parser::semantic_errors::{
    LazyImportContext, SemanticSyntaxChecker, SemanticSyntaxContext, SemanticSyntaxError,
    SemanticSyntaxErrorKind, YieldOutsideFunctionKind,
};
use ruff_text_size::{Ranged, TextRange};
use smallvec::SmallVec;
use ty_module_resolver::{
    ImportingFile, ModuleName, ResolverEnvironment, resolve_module_for_import_from,
};

use crate::HasTrackedScope;
use crate::ProgramFile;
use crate::ast_ids::node_key::ExpressionNodeKey;
use crate::ast_ids::{AstIdsBuilder, ScopedUseId};
use crate::ast_node_ref::AstNodeRef;
use crate::definition::{
    AnnotatedAssignmentDefinitionNodeRef, AssignmentDefinitionNodeRef, BindingsOwner,
    ComprehensionDefinitionNodeRef, Definition, DefinitionCategory, DefinitionKind,
    DefinitionNodeKey, DefinitionNodeRef, Definitions, DictKeyAssignmentNodeRef,
    ExceptHandlerDefinitionNodeRef, ForStmtDefinitionNodeRef, ImportDefinitionNodeRef,
    ImportFromDefinitionNodeRef, ImportFromSubmoduleDefinitionNodeRef,
    LambdaParameterDefinitionNodeRef, LoopHeaderDefinitionNodeRef, LoopStmtRef,
    MatchPatternDefinitionNodeRef, NestedBindingExecution, NestedBindingsDefinitionKind,
    ParameterDefinitionNodeRef, StarImportDefinitionNodeRef, WithItemDefinitionNodeRef,
};
use crate::expression::{Expression, ExpressionContext, ExpressionKind};
use crate::frozen::{FrozenMap, FrozenSet};
use crate::member::MemberExprBuilder;
use crate::place::{
    PlaceExpr, PlaceTable, PlaceTableBuilder, PossiblyNarrowedPlacesBuilder, ScopedPlaceId,
    match_subject_place_expressions,
};
use crate::predicate::{
    CallableAndCallExpr, ClassPatternKeywordPredicateKind, ClassPatternPredicateKind,
    MappingPatternEntryPredicateKind, MappingPatternPredicateKind, PatternPredicate,
    PatternPredicateKind, Predicate, PredicateNode, PredicateOrLiteral, ScopedPredicateId,
    SequencePatternPredicateKind, StarImportPlaceholderPredicate, StatementCall,
    SubjectElementPatternPredicate,
};
use crate::re_exports::exported_names;
use crate::reachability_constraints::{
    ReachabilityConstraintsBuilder, ScopedReachabilityConstraintId,
};
use crate::scope::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeKind, NodeWithScopeRef, Scope, ScopeId, ScopeKind,
    ScopeLaziness,
};
use crate::statement::StatementInner;
use crate::symbol::{ScopedSymbolId, Symbol};
use crate::unpack::{Unpack, UnpackKind, UnpackPosition, UnpackValue};
use crate::use_def::{
    EnclosingSnapshotKey, FlowSnapshot, FutureDefinitions, ImportedQualifierAction, LiveBinding,
    LiveBindingStatus, PreviousDefinitions, ScopedDefinitionId, ScopedEnclosingSnapshotId,
    UseDefMapBuilder, UseDefMapInterner,
};
use crate::{Db, Statement, StatementNodeKey};
use crate::{
    DefinitionsByNode, EvaluationMode, ExpressionsScopeMap, LoopHeader, LoopHeaderId,
    NarrowingAliasPredicate, PossiblyNarrowedPlaces, SemanticIndex, VisibleAncestorsIter,
};

use super::place::PlaceExprRef;

mod except_handlers;
mod loop_bindings_visitor;

#[derive(Clone, Debug, Default)]
struct Loop {
    /// Flow states at each `break` in the current loop.
    break_states: Vec<FlowSnapshot>,
    /// Flow states at each `continue` in the current loop.
    continue_states: Vec<FlowSnapshot>,
}

/// A narrowing alias: a variable whose RHS is a narrowing expression
/// (e.g., `is_none = x is None`).
#[derive(Clone, Debug)]
struct NarrowingAlias<'ast> {
    /// The RHS expression (e.g., `x is None`).
    expression: Expr<'ast>,
    /// The scope whose place table should be used to resolve the aliased expression.
    expression_scope: FileScopeId,
    /// Places that, if reassigned, should invalidate this alias.
    narrowed_places: PossiblyNarrowedPlaces,
}

struct ScopeInfo<'ast> {
    file_scope_id: FileScopeId,
    /// Current loop state; None if we are not currently visiting a loop
    current_loop: Option<Loop>,
    /// Saved narrowing aliases from the enclosing scope, restored on `pop_scope`.
    narrowing_aliases: FxHashMap<Name, NarrowingAlias<'ast>>,
    /// `global` and `nonlocal` declarations from scopes nested under this one. This is used for:
    ///
    /// 1. Visibility of nested writes. A nested function that binds a variable might affect the
    ///    inferred type of that variable in an outer scope. After each nested scope is closed, we
    ///    install synthetic definitions to refer to visible nested writes, which we read from this
    ///    map. (Note that *which kind* of nested writes is visible isn't necessarily known at that
    ///    point; we stash both kinds and decide which one to use at inference time.)
    /// 2. Semantic syntax errors for invalid `nonlocal` declarations. A `nonlocal` declaration is
    ///    required to resolve to a local variable, and that variable must not be declared `global`
    ///    or defined in the global scope. (This is why we track all `nonlocal` declarations, even
    ///    if there's no binding in their scope.)
    ///
    /// When we're trying to figure out what scope a `nonlocal` declaration resolves to, we have to
    /// remember that the definition of a local variable can come after a nested function that
    /// mentions it. Similarly, when we're trying to figure out whether `global` bindings from a
    /// nested scopes are visible in the current scope, we have to remember that a `global`
    /// declaration can also come after nested function definitions. We build up these maps as we
    /// encounter each `global` and `nonlocal` keyword, but we generally need to wait until scopes
    /// are popped (or later, at inference time) to analyze them.
    nested_global_or_nonlocal_declarations: NestedGlobalOrNonlocalDeclarations,
    /// Text ranges for `global` and `nonlocal` declarations in this scope, which we use to
    /// populate `nested_global_or_nonlocal_declarations` when we reach end of scope.
    this_scope_global_or_nonlocal_declarations: FxHashMap<Name, TextRange>,
    /// Free symbol uses from nested scopes that may resolve to this scope.
    pending_captures: PendingCaptures,
}

type NestedGlobalOrNonlocalDeclarations = FxHashMap<Name, SmallVec<[NestedDeclaration; 1]>>;

/// Captures cannot be resolved until the enclosing scope is complete because a later binding can
/// make the name local. For example, `inner` captures `outer`'s local `value`, even though the
/// binding appears after `inner` is defined:
///
/// ```python
/// def outer():
///     def inner():
///         return value
///
///     value = 1
/// ```
///
/// Each pending capture remembers the bindings visible when the nested scope was created and, for
/// lazy scopes, any bindings added afterward.
type PendingCaptures = FxHashMap<Name, SmallVec<[PendingCapture; 1]>>;

#[derive(Debug)]
struct PendingCapture {
    nested_scope: FileScopeId,
    laziness: ScopeLaziness,
    binding_definition_ids: SmallVec<[ScopedDefinitionId; 2]>,
}

#[derive(Debug)]
struct UnresolvedCapture {
    /// The scope containing the free symbol use. Retaining this lets final resolution apply class
    /// scope visibility rules correctly as the capture is propagated outward.
    nested_scope: FileScopeId,
    name: Name,
    laziness: ScopeLaziness,
}

impl UnresolvedCapture {
    fn through_scope(mut self, scope_laziness: ScopeLaziness) -> Self {
        if scope_laziness.is_lazy() {
            self.laziness = ScopeLaziness::Lazy;
        }
        self
    }
}

#[derive(Copy, Clone, Debug, get_size2::GetSize)]
pub struct NestedDeclaration {
    pub kind: GlobalOrNonlocal,
    pub file_scope_id: FileScopeId,
    pub range: TextRange,
    pub is_bound: bool,
}

impl NestedDeclaration {
    pub fn is_global(&self) -> bool {
        matches!(self.kind, GlobalOrNonlocal::Global)
    }
}

#[derive(Copy, Clone, Debug, get_size2::GetSize)]
pub enum GlobalOrNonlocal {
    Global,
    Nonlocal,
}

struct ConditionFlowSnapshots {
    truthy: FlowSnapshot,
    falsy: FlowSnapshot,
}

impl ConditionFlowSnapshots {
    fn into_short_circuit_and_continuation(self, op: BoolOp) -> (FlowSnapshot, FlowSnapshot) {
        match op {
            BoolOp::And => (self.falsy, self.truthy),
            BoolOp::Or => (self.truthy, self.falsy),
        }
    }
}

enum ConditionFlowSnapshot {
    Fallback,
    Branches(ConditionFlowSnapshots),
}

impl ConditionFlowSnapshot {
    fn into_truthy(self) -> Option<FlowSnapshot> {
        match self {
            Self::Fallback => None,
            Self::Branches(snapshots) => Some(snapshots.truthy),
        }
    }

    fn into_branches(self) -> Option<ConditionFlowSnapshots> {
        match self {
            Self::Fallback => None,
            Self::Branches(snapshots) => Some(snapshots),
        }
    }
}

pub(super) struct SemanticIndexBuilder<'db, 'ast> {
    // Builder state
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    source_type: PySourceType,
    module: &'ast ParsedModuleRef,
    scope_stack: Vec<ScopeInfo<'ast>>,
    /// The assignments we're currently visiting, with
    /// the most recent visit at the end of the Vec.
    current_assignments: Vec<CurrentAssignment<'ast, 'db>>,
    /// The statements we're currently visiting, with
    /// the most recent visit at the end of the Vec.
    current_statements: Vec<CurrentStatement<'ast, 'db>>,
    /// The match case we're currently visiting.
    current_match_case: Option<CurrentMatchCase<'ast, 'db>>,
    /// The name of the first function parameter of the innermost function that we're currently visiting.
    current_first_parameter_name: Option<&'ast str>,

    /// The scope of a boolean test whose subexpressions we are currently visiting.
    ///
    /// See [`Self::visit_boolean_test`] for details on how this field is used.
    active_boolean_test_scope: Option<FileScopeId>,

    /// Per-scope exception contexts for nested `try` and `with` statements.
    exception_context_stack_manager: ExceptionContextStackManager,

    /// Flags about the file's global scope
    has_future_annotations: bool,
    /// Whether we are currently visiting an `if TYPE_CHECKING` block.
    in_type_checking_block: bool,

    // Used for checking semantic syntax errors
    resolver_environment: ResolverEnvironment<'db>,
    python_version: PythonVersion,
    source_text: OnceCell<SourceText>,
    semantic_checker: SemanticSyntaxChecker,
    /// Whether the current statement is inside a `try` statement, including its `except`, `else`,
    /// and `finally` suites. Used for semantic syntax checks independently of handler activity.
    in_try_statement: bool,

    // Semantic Index fields
    scopes: IndexVec<FileScopeId, Scope>,
    scope_ids_by_scope: IndexVec<FileScopeId, ScopeId<'db>>,
    place_tables: IndexVec<FileScopeId, PlaceTableBuilder>,
    ast_ids: AstIdsBuilder,
    // Box to avoid copying large builders when this index grows.
    use_def_maps: IndexVec<FileScopeId, Box<UseDefMapBuilder<'db>>>,
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,
    scopes_by_expression: ExpressionsScopeMapBuilder,
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definitions<'db>>,
    expressions_by_node: FxHashMap<ExpressionNodeKey, Expression<'db>>,
    unpacks_by_target: FxHashMap<ExpressionNodeKey, Unpack<'db>>,
    condition_flow_snapshots_by_node: FxHashMap<ExpressionNodeKey, ConditionFlowSnapshots>,
    statements_by_node: FxHashMap<StatementNodeKey, Statement<'db>>,
    imported_modules: FxHashSet<ModuleName>,
    seen_submodule_imports: FxHashSet<String>,
    // A map from a lambda expression to its enclosing statement.
    enclosing_lambda_statements: FxHashMap<ExpressionNodeKey, Statement<'db>>,
    // A map from a constraining use of a collection initializer to its definition.
    collections_by_use: FxHashMap<ExpressionNodeKey, Definition<'db>>,
    // A map from a collection initializer definition to statements containing a constraining use.
    uses_by_collection: FxHashMap<Definition<'db>, Vec<(Statement<'db>, ExpressionNodeKey)>>,
    /// Hashset of all [`FileScopeId`]s that correspond to [generator functions].
    ///
    /// [generator functions]: https://docs.python.org/3/glossary.html#term-generator
    generator_functions: FxHashSet<FileScopeId>,
    /// Hashset of all [`FileScopeId`]s that correspond to asynchronous comprehensions.
    async_comprehensions: FxHashSet<FileScopeId>,
    /// Node indices of syntactic annotation roots.
    annotations: Vec<NodeIndex>,
    /// Snapshots of enclosing-scope place states visible from nested scopes.
    enclosing_snapshots: FxHashMap<EnclosingSnapshotKey, ScopedEnclosingSnapshotId>,
    /// Errors collected by the `semantic_checker`.
    semantic_syntax_errors: RefCell<Vec<SemanticSyntaxError>>,

    /// Maps alias variable names to their narrowing expressions (same-scope only).
    /// TODO: cross-scope alias narrowing support
    narrowing_aliases: FxHashMap<Name, NarrowingAlias<'ast>>,

    /// Alias metadata for predicate leaf names in the current file.
    alias_predicates: FxHashMap<ExpressionNodeKey, NarrowingAliasPredicate<'db>>,
}

impl<'db, 'ast> SemanticIndexBuilder<'db, 'ast> {
    pub(super) fn new(
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        module_ref: &'ast ParsedModuleRef,
    ) -> Self {
        let mut builder = Self {
            db,
            file,
            source_type: file.file(db).source_type(db),
            module: module_ref,
            scope_stack: Vec::new(),
            current_assignments: Vec::new(),
            current_statements: Vec::new(),
            current_match_case: None,
            current_first_parameter_name: None,
            active_boolean_test_scope: None,
            exception_context_stack_manager: ExceptionContextStackManager::default(),

            has_future_annotations: false,
            in_type_checking_block: false,

            scopes: IndexVec::new(),
            place_tables: IndexVec::new(),
            ast_ids: AstIdsBuilder::default(),
            scope_ids_by_scope: IndexVec::new(),
            use_def_maps: IndexVec::new(),

            scopes_by_expression: ExpressionsScopeMapBuilder::new(),
            scopes_by_node: FxHashMap::default(),
            definitions_by_node: FxHashMap::default(),
            expressions_by_node: FxHashMap::default(),
            unpacks_by_target: FxHashMap::default(),
            condition_flow_snapshots_by_node: FxHashMap::default(),
            statements_by_node: FxHashMap::default(),
            enclosing_lambda_statements: FxHashMap::default(),
            collections_by_use: FxHashMap::default(),
            uses_by_collection: FxHashMap::default(),

            seen_submodule_imports: FxHashSet::default(),
            imported_modules: FxHashSet::default(),
            generator_functions: FxHashSet::default(),
            async_comprehensions: FxHashSet::default(),
            annotations: Vec::new(),

            enclosing_snapshots: FxHashMap::default(),

            resolver_environment: file.resolver_environment(db),
            python_version: file.python_version(db),
            source_text: OnceCell::new(),
            semantic_checker: SemanticSyntaxChecker::new(module_ref.ast()),
            in_try_statement: false,
            semantic_syntax_errors: RefCell::default(),
            narrowing_aliases: FxHashMap::default(),
            alias_predicates: FxHashMap::default(),
        };

        builder.push_scope_with_parent(NodeWithScopeRef::Module, None);
        builder
    }

    fn current_scope_info(&self) -> &ScopeInfo<'ast> {
        self.scope_stack
            .last()
            .expect("SemanticIndexBuilder should have created a root scope")
    }

    fn current_scope_info_mut(&mut self) -> &mut ScopeInfo<'ast> {
        self.scope_stack
            .last_mut()
            .expect("SemanticIndexBuilder should have created a root scope")
    }

    fn current_scope(&self) -> FileScopeId {
        self.current_scope_info().file_scope_id
    }

    fn current_scope_id(&self) -> ScopeId<'db> {
        self.scope_ids_by_scope[self.current_scope()]
    }

    fn mark_current_comprehension_async(&mut self) {
        let scope = self.current_scope();
        if self.scopes[scope].kind() == ScopeKind::Comprehension {
            self.async_comprehensions.insert(scope);
        }
    }

    fn expect_single_definition(
        &self,
        definition_key: impl Into<DefinitionNodeKey> + std::fmt::Debug + Copy,
    ) -> Definition<'db> {
        let definitions = &self.definitions_by_node[&definition_key.into()];
        debug_assert_eq!(
            definitions.len(),
            1,
            "Expected exactly one definition to be associated with AST node {definition_key:?} but found {}",
            definitions.len()
        );
        definitions[0]
    }

    /// Returns an iterator over ancestors of `scope` that are visible for name resolution,
    /// starting with `scope` itself. This follows Python's lexical scoping rules where
    /// class scopes are skipped during name resolution (except for the starting scope
    /// if it happens to be a class scope).
    ///
    /// For example, in this code:
    /// ```python
    /// x = 1
    /// class A:
    ///     x = 2
    ///     def method(self):
    ///         print(x)  # Refers to global x=1, not class x=2
    /// ```
    /// The `method` function can see the global scope but not the class scope.
    fn visible_ancestor_scopes(&self, scope: FileScopeId) -> VisibleAncestorsIter<'_> {
        VisibleAncestorsIter::new(&self.scopes, scope)
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
        self.push_scope_with_parent(node, Some(self.current_scope()));
    }

    fn push_scope_with_parent(&mut self, node: NodeWithScopeRef, parent: Option<FileScopeId>) {
        let children_start = self.scopes.next_index() + 1;

        // Note `node` is guaranteed to be a child of `self.module`
        let node_with_kind = node.to_kind(self.module);

        let scope = Scope::new(parent, node_with_kind, children_start..children_start);
        let scope_kind = scope.kind();
        self.exception_context_stack_manager.enter_nested_scope();

        let file_scope_id = self.scopes.push(scope);
        self.place_tables.push(PlaceTableBuilder::default());
        self.use_def_maps
            .push(Box::new(UseDefMapBuilder::new(scope_kind)));
        let ast_id_scope = self.ast_ids.push_scope();

        let scope_id = ScopeId::new(self.db, self.file, file_scope_id);

        self.scope_ids_by_scope.push(scope_id);
        let previous = self.scopes_by_node.insert(node.node_key(), file_scope_id);
        debug_assert_eq!(previous, None);

        debug_assert_eq!(ast_id_scope, file_scope_id);

        // Save narrowing aliases. They will be restored with `pop_scope` after returning from inspecting the inner scope.
        // TODO: Cross-scope alias narrowing is not supported yet.
        let saved_aliases = std::mem::take(&mut self.narrowing_aliases);
        self.scope_stack.push(ScopeInfo {
            file_scope_id,
            current_loop: None,
            narrowing_aliases: saved_aliases,
            nested_global_or_nonlocal_declarations: FxHashMap::default(),
            this_scope_global_or_nonlocal_declarations: FxHashMap::default(),
            pending_captures: FxHashMap::default(),
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

    fn register_pending_capture(&mut self, capture: UnresolvedCapture) {
        let current_scope = self.current_scope();
        let binding_definition_ids = self.place_tables[current_scope]
            .symbol_id(&capture.name)
            .into_iter()
            .flat_map(|symbol| {
                self.use_def_maps[current_scope].symbol_binding_definition_ids(symbol)
            })
            .collect();

        let captures = self
            .current_scope_info_mut()
            .pending_captures
            .entry(capture.name)
            .or_default();

        if let Some(pending) = captures
            .iter_mut()
            .find(|pending| pending.nested_scope == capture.nested_scope)
        {
            pending
                .binding_definition_ids
                .extend(binding_definition_ids);
            if capture.laziness.is_lazy() {
                pending.laziness = ScopeLaziness::Lazy;
            }
        } else {
            captures.push(PendingCapture {
                nested_scope: capture.nested_scope,
                laziness: capture.laziness,
                binding_definition_ids,
            });
        }
    }

    fn record_pending_capture_binding(
        &mut self,
        symbol: ScopedSymbolId,
        definition_id: ScopedDefinitionId,
    ) {
        let current_scope = self.current_scope();
        let name = self.place_tables[current_scope]
            .symbol(symbol)
            .name()
            .clone();

        let Some(captures) = self
            .current_scope_info_mut()
            .pending_captures
            .get_mut(&name)
        else {
            return;
        };

        for capture in captures {
            if capture.laziness.is_lazy() {
                capture.binding_definition_ids.push(definition_id);
            }
        }
    }

    fn finish_pending_captures(
        &mut self,
        popped_scope_id: FileScopeId,
        popped_scope_laziness: ScopeLaziness,
        pending_captures: PendingCaptures,
    ) -> Vec<UnresolvedCapture> {
        let mut unresolved = Vec::new();

        #[expect(
            clippy::iter_over_hash_type,
            reason = "capture resolution and usage marking are order-independent"
        )]
        for (name, captures) in pending_captures {
            for capture in captures {
                if self.resolve_nested_reference_scope(capture.nested_scope, &name)
                    == Some(popped_scope_id)
                {
                    self.use_def_maps[popped_scope_id]
                        .mark_binding_definitions_used(capture.binding_definition_ids);
                } else {
                    unresolved.push(
                        UnresolvedCapture {
                            nested_scope: capture.nested_scope,
                            name: name.clone(),
                            laziness: capture.laziness,
                        }
                        .through_scope(popped_scope_laziness),
                    );
                }
            }
        }

        for symbol in self.place_tables[popped_scope_id].symbols() {
            if symbol.is_used() && !symbol.is_local() && !symbol.is_global() {
                unresolved.push(UnresolvedCapture {
                    nested_scope: popped_scope_id,
                    name: symbol.name().clone(),
                    laziness: popped_scope_laziness,
                });
            }
        }

        unresolved
    }

    // Records snapshots of the place states visible from the current lazy scope.
    fn record_lazy_snapshots(&mut self, popped_scope_id: FileScopeId) {
        for enclosing_scope_info in self.scope_stack.iter().rev() {
            let enclosing_scope_id = enclosing_scope_info.file_scope_id;
            let enclosing_scope_kind = self.scopes[enclosing_scope_id].kind();
            let enclosing_place_table = &self.place_tables[enclosing_scope_id];

            // We don't record lazy snapshots of attributes or subscripts, because these are difficult to track as they modify.
            for nested_symbol in self.place_tables[popped_scope_id].symbols() {
                // For the same reason, we don't snapshot bindings owned by `global`/`nonlocal`
                // forwarding declarations here; `snapshot_enclosing_state` stores only a
                // constraint for those symbols. Also, if the enclosing scope allows its members to
                // be modified from elsewhere, the snapshot will not be recorded.
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
        #[expect(
            clippy::iter_over_hash_type,
            reason = "each matching snapshot is updated independently"
        )]
        for (key, snapshot_id) in &self.enclosing_snapshots {
            if let Some(enclosing_symbol) = key.enclosing_place.as_symbol() {
                let name = self.place_tables[key.enclosing_scope]
                    .symbol(enclosing_symbol)
                    .name();
                let is_reassignment_of_snapshotted_symbol = || {
                    for (ancestor, _) in self.visible_ancestor_scopes(key.enclosing_scope) {
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
        if self.enclosing_snapshots.is_empty() {
            return;
        }

        // Only the last scope containing a bound nonlocal matters: a lazy snapshot must be
        // discarded if such a scope occurs at or after its enclosing scope. Build this index once
        // instead of searching all later scopes separately for every snapshot.
        let mut last_nonlocal_binding = FxHashMap::default();
        for (scope_id, place_table) in self.place_tables.iter_enumerated() {
            for symbol in place_table.symbols() {
                if symbol.is_nonlocal() && symbol.is_bound() {
                    last_nonlocal_binding.insert(symbol.name(), scope_id);
                }
            }
        }

        if last_nonlocal_binding.is_empty() {
            return;
        }

        self.enclosing_snapshots.retain(|key, _| {
            if key.nested_laziness.is_eager() {
                return true;
            }
            let ScopedPlaceId::Symbol(symbol_id) = key.enclosing_place else {
                return true;
            };
            let name = self.place_tables[key.enclosing_scope]
                .symbol(symbol_id)
                .name();
            last_nonlocal_binding
                .get(name)
                .is_none_or(|scope| scope.index() < key.enclosing_scope.index())
        });
    }

    /// Finds the nearest visible ancestor scope that actually owns a local binding for `name`.
    fn resolve_nested_reference_scope(
        &self,
        nested_scope: FileScopeId,
        name: &str,
    ) -> Option<FileScopeId> {
        self.visible_ancestor_scopes(nested_scope)
            .skip(1)
            .find_map(|(scope_id, _)| {
                let place_table = &self.place_tables[scope_id];
                let symbol_id = place_table.symbol_id(name)?;
                let symbol = place_table.symbol(symbol_id);

                // Only a true local binding in an ancestor scope can be the resolution target.
                // `global`/`nonlocal` here are forwarding declarations, not owning bindings.
                symbol.is_local().then_some(scope_id)
            })
    }

    /// Returns the `NestedGlobalOrNonlocalDeclarations` that are still visible to the enclosing
    /// scope, including those contributed by `global` and `nonlocal` keywords in the popped scope,
    /// but excluding nested `nonlocal`s that resolved to the popped scope.
    fn pop_scope(&mut self) -> NestedGlobalOrNonlocalDeclarations {
        self.exception_context_stack_manager.exit_scope();

        let ScopeInfo {
            file_scope_id: popped_scope_id,
            narrowing_aliases,
            mut nested_global_or_nonlocal_declarations,
            this_scope_global_or_nonlocal_declarations,
            pending_captures,
            ..
        } = self
            .scope_stack
            .pop()
            .expect("Root scope should be present");
        self.narrowing_aliases = narrowing_aliases;

        let children_end = self.scopes.next_index();

        let popped_scope = &mut self.scopes[popped_scope_id];
        popped_scope.extend_descendants(children_end);
        let popped_scope_kind = popped_scope.kind();

        let popped_scope_laziness = popped_scope.kind().laziness();

        if popped_scope_laziness.is_eager() {
            self.record_eager_snapshots(popped_scope_id);
        } else {
            self.record_lazy_snapshots(popped_scope_id);
        }

        let unresolved_captures =
            self.finish_pending_captures(popped_scope_id, popped_scope_laziness, pending_captures);
        if !self.scope_stack.is_empty() {
            for capture in unresolved_captures {
                self.register_pending_capture(capture);
            }
        }

        // If we've popped the module scope, there is no enclosing scope that needs our nested
        // bindings. Short-circuit here and return an empty map.
        if popped_scope_kind.is_module() {
            debug_assert!(self.scope_stack.is_empty());
            return FxHashMap::default();
        }

        // In the common case where we don't have any nested `global` or `nonlocal` declarations at
        // all, short-circuit so that we don't walk the place table for no reason.
        if nested_global_or_nonlocal_declarations.is_empty()
            && this_scope_global_or_nonlocal_declarations.is_empty()
        {
            return FxHashMap::default();
        }

        // For each symbol in the (non-module) scope we just popped:
        // 1. If the popped scope is function-like (not a class body), see whether it resolves any
        //    `nonlocal` declarations (legally or illegally) from further nested scopes.
        // 2. See whether it contributes any nested `global` or `nonlocal` declarations to the
        //    enclosing scope.
        for symbol in self.place_tables[popped_scope_id].symbols() {
            // Filter out any nested `nonlocal` declaration that resolve in the popped scope.
            // Typically these resolve to a defined (bound or declared) symbol, but they can also
            // resolve (illegally) to an unbound `global` declaration. Remove any resolved
            // declarations from the `nested_global_or_nonlocal_declarations` map, both so that we
            // don't try to resolve them again, and so that synthetic nested binding definitions we
            // install in enclosing scopes don't see them. (Note that this doesn't affect any
            // synthetic nested binding definitions installed in the popped scope, which have
            // already recorded these declarations.)
            if popped_scope_kind.is_function_like()
                && (symbol.is_local() || symbol.is_global())
                && let Some(nested_declarations) =
                    nested_global_or_nonlocal_declarations.get_mut(symbol.name())
            {
                nested_declarations.retain(|declaration| {
                    if matches!(declaration.kind, GlobalOrNonlocal::Nonlocal) {
                        // It's a syntax error for a `nonlocal` declaration to resolve to a
                        // `global` statement in an enclosing scope.
                        if symbol.is_global() {
                            self.report_semantic_error(SemanticSyntaxError {
                                kind: SemanticSyntaxErrorKind::NonlocalWithoutBinding(
                                    symbol.name().to_string(),
                                ),
                                range: declaration.range,
                                python_version: self.python_version(),
                            });
                        }
                        // This `nonlocal` is resolved.
                        false
                    } else {
                        // Nested `global` declarations never "resolve" per se. This is both
                        // because we already know they refer to the global scope, and also because
                        // they can "pass through" intervening scopes where they're not visible to
                        // containing scopes where they're visible again.
                        true
                    }
                });
                // If we've resolved all the nested declarations for a symbol, remove it from the
                // map entirely. It wouldn't break anything to keep an empty list, but this avoids
                // pointless allocations in enclosing scopes.
                if nested_declarations.is_empty() {
                    nested_global_or_nonlocal_declarations.remove(symbol.name());
                }
            }

            // Add in any `global` and `nonlocal` declarations from this (non-module) scope.
            if symbol.is_global() || symbol.is_nonlocal() {
                let kind = if symbol.is_global() {
                    GlobalOrNonlocal::Global
                } else {
                    GlobalOrNonlocal::Nonlocal
                };
                nested_global_or_nonlocal_declarations
                    .entry(symbol.name().clone())
                    .or_default()
                    .push(NestedDeclaration {
                        kind,
                        file_scope_id: popped_scope_id,
                        range: *this_scope_global_or_nonlocal_declarations
                            .get(symbol.name())
                            .expect("should have recorded a TextRange"),
                        // This `is_bound` flag is why we wait until now to record these,
                        // rather than doing it when we encounter the keywords.
                        is_bound: symbol.is_bound(),
                    });
            }
        }

        // If the enclosing scope is the module scope, it's a semantic syntax error error to have
        // any remaining unresolved `nonlocal` declarations.
        if self.scope_stack.len() == 1 {
            debug_assert!(
                self.scopes[self.scope_stack[0].file_scope_id]
                    .kind()
                    .is_module(),
                "the last remaining scope should be the module scope",
            );
            #[expect(
                clippy::iter_over_hash_type,
                reason = "iteration order does not affect the semantic errors produced"
            )]
            for (name, nested_declarations) in &nested_global_or_nonlocal_declarations {
                for declaration in nested_declarations {
                    if matches!(declaration.kind, GlobalOrNonlocal::Nonlocal) {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::NonlocalWithoutBinding(name.to_string()),
                            range: declaration.range,
                            python_version: self.python_version(),
                        });
                    }
                }
            }
        }

        // Now we've updated `nested_global_or_nonlocal_declarations` based on what happened in the
        // popped scope (resolutions and new declarations). Merge the whole map with the caller's
        // own `nested_global_or_nonlocal_declarations` here. Note that we'll *also* return it
        // immediately below, so that the caller can synthesize nested bindings definitions that
        // only respect the bindings within the popped scope, rather than in all the nested scopes
        // they've encountered so far.
        if !popped_scope_kind.is_module() {
            #[expect(
                clippy::iter_over_hash_type,
                reason = "declarations for distinct names are merged independently"
            )]
            for (name, declarations) in &nested_global_or_nonlocal_declarations {
                self.current_scope_info_mut()
                    .nested_global_or_nonlocal_declarations
                    .entry(name.clone())
                    .or_default()
                    .extend_from_slice(declarations);
            }
        }

        // Here's the return described above.
        nested_global_or_nonlocal_declarations
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

    /// If the given expression is a use of an unannotated collection initializer, returns
    /// the definition of the initializer.
    fn unannotated_collection_initializer_binding(
        &self,
        collection_use: Expr<'_>,
    ) -> Option<Definition<'db>> {
        let use_def = self.current_use_def_map();
        let use_id = self
            .ast_ids
            .try_use_id(self.current_scope(), collection_use)?;

        use_def
            .bindings_at_use(use_id)
            .filter_map(|binding| use_def.definition(binding.binding()).definition())
            .filter(|definition| {
                definition
                    .kind(self.db)
                    .as_unannotated_assignment()
                    .is_some_and(|assignment| {
                        is_collection_initializer(assignment.value(self.module))
                    })
            })
            // TODO: Support uses that refer to multiple definitions. This currently seems to lead to
            // cycle-related panics.
            .exactly_one()
            .ok()
    }

    fn unannotated_collection_literal_binding(
        &self,
        collection_use: Expr<'_>,
    ) -> Option<Definition<'db>> {
        let definition = self.unannotated_collection_initializer_binding(collection_use)?;
        definition
            .kind(self.db)
            .as_unannotated_assignment()
            .is_some_and(|assignment| is_collection_literal(assignment.value(self.module)))
            .then_some(definition)
    }

    /// Try to register a narrowing alias for a simple name assignment.
    ///
    /// Any pre-existing alias entry for the `target` name has already been removed by
    /// [`Self::invalidate_narrowing_aliases_for`] in the binding pathway that ran before
    /// this call, so we only need to decide whether to insert a new entry.
    fn try_register_narrowing_alias(&mut self, target: Expr<'_>, value: Option<Expr<'ast>>) {
        let Some(target_name_expr) = target.as_name_expr() else {
            return;
        };
        let Some(value) = value else { return };
        let target_name = target_name_expr.id();

        if !Self::can_register_narrowing_alias(value) {
            return;
        }

        let place_table = self.current_place_table();
        let narrowed_places =
            PossiblyNarrowedPlacesBuilder::new(self.db, place_table).expression(value);

        // Don't register if the target itself is one of the narrowed places (e.g. `x = x is None`),
        // since the alias would be invalidated immediately by this same assignment.
        let target_is_narrowed = place_table
            .symbol_id(target_name)
            .is_some_and(|symbol| narrowed_places.contains(&symbol.into()));

        if !narrowed_places.is_empty() && !target_is_narrowed {
            self.narrowing_aliases.insert(
                target_name.clone(),
                NarrowingAlias {
                    expression: value,
                    expression_scope: self.current_scope(),
                    narrowed_places,
                },
            );
        }
    }

    /// Invalidate any narrowing aliases affected by a new definition of `place`.
    fn invalidate_narrowing_aliases_for(&mut self, place: ScopedPlaceId) {
        let place_table = &self.place_tables[self.current_scope()];
        let associated_members = place_table.associated_place_ids(place);
        let reassigned_alias_name = place
            .as_symbol()
            .map(|symbol_id| place_table.symbol(symbol_id).name());

        self.narrowing_aliases.retain(|name, alias| {
            // Drop aliases that narrow the reassigned place or any of its members.
            //  e.g. `is_none = x is None and ...; x = 1`
            if alias.narrowed_places.contains(&place) {
                return false;
            }

            //  e.g. `is_none = a.x is None; a = A()`
            if associated_members
                .iter()
                .any(|m| alias.narrowed_places.contains(&(*m).into()))
            {
                return false;
            }

            // Drop the alias whose own variable is the reassigned place.
            // e.g. `is_none = x is None; is_none = False`
            reassigned_alias_name != Some(name)
        });
    }

    fn can_register_narrowing_alias(value: Expr<'_>) -> bool {
        match value.classify() {
            // Bare names are too common to treat as alias candidates on every assignment,
            // and doing so would noticeably degrade performance. Excluding them only means
            // we don't infer truthiness narrowing for arbitrary chained aliases.
            ExprKind::Name(_) => false,
            ExprKind::Compare(_) | ExprKind::Call(_) => true,
            ExprKind::UnaryOp(unary) if unary.op() == UnaryOp::Not => {
                Self::can_register_narrowing_alias(unary.operand())
            }
            ExprKind::BoolOp(bool_op) => bool_op
                .values()
                .iter()
                .any(Self::can_register_narrowing_alias),
            ExprKind::If(expr_if) => {
                Self::can_register_narrowing_alias(expr_if.test())
                    || Self::can_register_narrowing_alias(expr_if.body())
                    || Self::can_register_narrowing_alias(expr_if.orelse())
            }
            _ => false,
        }
    }

    /// Walk a predicate expression tree, calling `f` on each leaf position
    /// where an alias Name could appear.
    fn walk_narrowing_alias_predicate<'expr>(expr: Expr<'expr>, f: &mut impl FnMut(Expr<'expr>)) {
        match expr.classify() {
            ExprKind::Name(_) => f(expr),
            ExprKind::UnaryOp(unary) if unary.op() == UnaryOp::Not => {
                Self::walk_narrowing_alias_predicate(unary.operand(), f);
            }
            ExprKind::BoolOp(bool_op) => {
                for value in bool_op.values() {
                    Self::walk_narrowing_alias_predicate(value, f);
                }
            }
            ExprKind::Call(call) => {
                for arg in call.arguments().args() {
                    Self::walk_narrowing_alias_predicate(arg, f);
                }
                for keyword in call.arguments().keywords() {
                    Self::walk_narrowing_alias_predicate(keyword.value(), f);
                }
            }
            ExprKind::If(expr_if) => {
                Self::walk_narrowing_alias_predicate(expr_if.test(), f);
                Self::walk_narrowing_alias_predicate(expr_if.body(), f);
                Self::walk_narrowing_alias_predicate(expr_if.orelse(), f);
            }
            ExprKind::Named(expr_named) => {
                Self::walk_narrowing_alias_predicate(expr_named.value(), f);
            }
            _ => {}
        }
    }

    /// Register alias predicates for alias Names found in a predicate expression.
    fn register_narrowing_alias_predicates(&mut self, expr: Expr<'ast>) {
        Self::walk_narrowing_alias_predicate(expr, &mut |leaf| {
            let Some(name) = leaf.as_name_expr() else {
                return;
            };
            let Some(alias) = self.narrowing_aliases.get(name.id()) else {
                return;
            };
            if self
                .ast_ids
                .try_use_id(self.current_scope(), leaf)
                .is_none()
            {
                return;
            }

            let aliased_expression = Expression::new(
                self.db,
                self.scope_ids_by_scope[alias.expression_scope],
                AstNodeRef::from_view(self.module, alias.expression),
                None,
                ExpressionKind::Normal,
            );
            self.alias_predicates.insert(
                ExpressionNodeKey::from(leaf),
                NarrowingAliasPredicate {
                    expression: aliased_expression,
                },
            );
        });
    }

    /// Add narrowed places from aliased expressions to the possibly-narrowed set.
    fn add_alias_narrowed_places(&self, expr: Expr<'_>, places: &mut PossiblyNarrowedPlaces) {
        Self::walk_narrowing_alias_predicate(expr, &mut |leaf| {
            let key = ExpressionNodeKey::from(leaf);
            if let Some(alias_predicate) = self.alias_predicates.get(&key) {
                let aliased_node = alias_predicate
                    .expression
                    .node_ref(self.db)
                    .node(self.module);
                let aliased_places =
                    PossiblyNarrowedPlacesBuilder::new(self.db, self.current_place_table())
                        .expression(aliased_node);
                places.extend(aliased_places);
            }
        });
    }

    fn flow_snapshot(&self) -> FlowSnapshot {
        self.current_use_def_map().snapshot()
    }

    /// Takes specialized truthy/falsy flow states for condition expressions whose evaluation can
    /// leave different bindings behind depending on the condition outcome.
    fn take_condition_flow_snapshots(&mut self, expr: Expr<'_>) -> Option<ConditionFlowSnapshots> {
        match expr.classify() {
            ExprKind::BoolOp(_) => self
                .condition_flow_snapshots_by_node
                .remove(&ExpressionNodeKey::from(expr)),
            ExprKind::UnaryOp(unary_op) if unary_op.op() == UnaryOp::Not => {
                let snapshots = self.take_condition_flow_snapshots(unary_op.operand())?;
                Some(ConditionFlowSnapshots {
                    truthy: snapshots.falsy,
                    falsy: snapshots.truthy,
                })
            }
            _ => None,
        }
    }

    fn flow_snapshot_for_condition(&mut self, condition: Expr<'_>) -> ConditionFlowSnapshot {
        self.record_exception_checkpoint_if(!Self::condition_evaluation_is_known_safe(condition));

        if let Some(snapshots) = self.take_condition_flow_snapshots(condition) {
            ConditionFlowSnapshot::Branches(snapshots)
        } else {
            ConditionFlowSnapshot::Fallback
        }
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

    fn record_place_use(&mut self, place_id: ScopedPlaceId, expr: Expr<'ast>) {
        if let ScopedPlaceId::Symbol(symbol_id) = place_id {
            self.mark_symbol_used(symbol_id);
        }
        let use_id = self.ast_ids.record_use(self.current_scope(), expr);
        self.current_use_def_map_mut().record_use(place_id, use_id);
    }

    fn record_place_definition(&mut self, place_id: ScopedPlaceId, expr: Expr<'ast>) {
        match self.current_assignment() {
            Some(CurrentAssignment::Assign {
                node,
                unpack,
                owner,
            }) => {
                let assignment = self.add_definition(
                    place_id,
                    AssignmentDefinitionNodeRef {
                        unpack,
                        value: node.value(),
                        target: expr,
                        owner,
                    },
                );

                self.add_dict_key_assignment_definitions(node.targets(), node.value(), assignment);
            }
            Some(CurrentAssignment::AnnAssign {
                node: ann_assign,
                pending,
            }) => {
                self.add_standalone_type_expression(ann_assign.annotation());
                let assignment = if let Some(pending) = pending {
                    self.finish_annotated_assignment(pending)
                } else {
                    self.add_definition(
                        place_id,
                        AnnotatedAssignmentDefinitionNodeRef { node: ann_assign },
                    )
                };

                if let Some(value) = ann_assign.value() {
                    self.add_dict_key_assignment_definitions(
                        [ann_assign.target()],
                        value,
                        assignment,
                    );
                }
            }
            Some(CurrentAssignment::AugAssign(aug_assign)) => {
                self.add_definition(place_id, aug_assign);
            }
            Some(CurrentAssignment::For { node, unpack }) => {
                self.add_definition(
                    place_id,
                    ForStmtDefinitionNodeRef {
                        unpack,
                        node,
                        target: expr,
                    },
                );
            }
            Some(CurrentAssignment::Named(named)) => {
                self.mark_comprehension_named_target(place_id, named.target().range());
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
                        node,
                        target: expr,
                        first,
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
                        item,
                        target: expr,
                        is_async,
                    },
                );
            }
            None => {}
        }
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
    /// for all nodes *except* [`ruff_python_ast::Alias`] nodes representing `*` imports.
    fn add_definition(
        &mut self,
        place: ScopedPlaceId,
        definition_node: impl Into<DefinitionNodeRef<'ast, 'db>> + std::fmt::Debug + Copy,
    ) -> Definition<'db> {
        let definition = self.create_definition(place, definition_node);
        self.record_definition(place, definition, None);
        definition
    }

    /// Create a definition without making its declaration or binding visible in control flow.
    fn create_definition(
        &mut self,
        place: ScopedPlaceId,
        definition_node: impl Into<DefinitionNodeRef<'ast, 'db>> + std::fmt::Debug + Copy,
    ) -> Definition<'db> {
        let (definition, num_definitions) =
            self.create_additional_definition(place, definition_node);
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
    /// Most AST nodes can only be associated with at most one [`Definition`]. Generally prefer
    /// `add_definition` above, which enforces that. This method should currently only be used with
    /// `*` imports and loop headers.
    fn push_additional_definition(
        &mut self,
        place: ScopedPlaceId,
        definition_node: impl Into<DefinitionNodeRef<'ast, 'db>>,
    ) {
        let (definition, _) = self.create_additional_definition(place, definition_node);
        self.record_definition(place, definition, None);
    }

    /// Create a [`Definition`] without recording it in control flow.
    ///
    /// Returns the new definition and the number of definitions now associated with its AST
    /// node. Loop headers are not stored by AST node, so their count is zero. Prefer
    /// [`Self::create_definition`] when the node must have exactly one definition.
    fn create_additional_definition(
        &mut self,
        place: ScopedPlaceId,
        definition_node: impl Into<DefinitionNodeRef<'ast, 'db>>,
    ) -> (Definition<'db>, usize) {
        let definition_node: DefinitionNodeRef<'ast, 'db> = definition_node.into();

        // Note `definition_node` is guaranteed to be a child of `self.module`
        let kind = definition_node.into_owned(self.module);
        let is_loop_header = kind.is_loop_header();
        let is_reexported = kind.is_reexported();

        let definition: Definition<'db> =
            Definition::new(self.db, self.current_scope_id(), place, kind, is_reexported);

        let num_definitions = if is_loop_header {
            // Loop headers are internal use-def definitions. They are retrieved through the loop
            // token rather than by their AST node.
            0
        } else {
            let definitions = self.add_entry_for_definition_key(definition_node.key());
            definitions.push(definition);
            definitions.len()
        };

        (definition, num_definitions)
    }

    /// Records an already-created definition in the current scope.
    ///
    /// `previous_definitions` controls whether a new binding replaces earlier bindings. By
    /// default, ordinary assignments replace them and loop headers keep them. Comprehension
    /// bindings choose explicitly because an assignment that only runs on some paths must keep
    /// the earlier binding.
    fn record_definition(
        &mut self,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        previous_definitions: Option<PreviousDefinitions>,
    ) {
        let kind = definition.kind(self.db);
        let category = kind.category(self.source_type.is_stub(), self.module);
        match category {
            DefinitionCategory::Declaration => {
                self.mark_place_declared(place);
                self.current_use_def_map_mut()
                    .record_declaration(place, definition);
            }
            DefinitionCategory::DeclarationAndBinding => {
                self.mark_place_declared(place);
                self.record_binding_with(definition, |use_def, place| {
                    use_def.record_combined_definition(place, definition, category);
                });
            }
            DefinitionCategory::Binding => {
                let imported_qualifier_action = match kind {
                    DefinitionKind::ImportFrom(_) | DefinitionKind::StarImport(_) => {
                        ImportedQualifierAction::Record
                    }
                    DefinitionKind::Import(_) | DefinitionKind::ImportFromSubmodule(_) => {
                        ImportedQualifierAction::Clear
                    }
                    _ => ImportedQualifierAction::Preserve,
                };
                let previous = previous_definitions.unwrap_or(if kind.is_loop_header() {
                    PreviousDefinitions::AreKept
                } else {
                    PreviousDefinitions::AreShadowed
                });
                self.record_binding_with(definition, |use_def, place| {
                    use_def.record_binding(
                        place,
                        definition,
                        previous,
                        FutureDefinitions::ShadowThisOne,
                        imported_qualifier_action,
                    );
                });
            }
        }
    }

    /// Declare an annotated name assignment whose value will be bound after visiting its RHS.
    /// Other targets and annotations without a RHS are recorded in full by `add_definition`.
    fn begin_annotated_assignment(
        &mut self,
        node: StmtAnnAssign<'ast>,
    ) -> Option<PendingAnnotatedAssignment<'db>> {
        let ExprKind::Name(name) = node.target().classify() else {
            return None;
        };
        node.value()?;

        let place = self.add_symbol(name.id().clone()).into();
        let definition =
            self.create_definition(place, AnnotatedAssignmentDefinitionNodeRef { node });
        self.mark_place_declared(place);
        self.current_use_def_map_mut().record_combined_definition(
            place,
            definition,
            DefinitionCategory::Declaration,
        );
        Some(PendingAnnotatedAssignment { definition })
    }

    /// Bind the value of an annotated assignment whose declaration was recorded before its RHS.
    fn finish_annotated_assignment(
        &mut self,
        pending: PendingAnnotatedAssignment<'db>,
    ) -> Definition<'db> {
        let definition = pending.definition;
        self.record_binding_with(definition, |use_def, place| {
            use_def.record_combined_definition(place, definition, DefinitionCategory::Binding);
        });
        definition
    }

    /// Record one binding while keeping aliases, captures, and lazy snapshots in sync.
    /// The callback receives the definition's place and must append that binding to the current
    /// use-def map.
    fn record_binding_with(
        &mut self,
        definition: Definition<'db>,
        record: impl FnOnce(&mut UseDefMapBuilder<'db>, ScopedPlaceId),
    ) {
        let place = definition.place(self.db);
        let is_loop_header = definition.kind(self.db).is_loop_header();

        // We need to avoid marking places as bound as soon as we encounter a loop header
        // definition for them, because that would lead to false-positive semantic syntax errors in
        // cases like this:
        // ```py
        // while True:
        //     global x  # [invalid-syntax] if `x` is already used or bound
        //     x = 1
        // ```
        if !is_loop_header {
            self.mark_place_bound(place);
            self.invalidate_narrowing_aliases_for(place);
        }

        let definition_id = self.current_use_def_map().next_definition_id();
        record(self.current_use_def_map_mut(), place);

        if !is_loop_header {
            self.delete_associated_bindings(place);
        }

        if let Some(id) = place.as_symbol() {
            self.record_pending_capture_binding(id, definition_id);
            self.update_lazy_snapshots(id);
        }
    }

    // Creates a definition for each key-value assignment in the dictionary.
    //
    // If there are multiple targets, no definitions will be created.
    fn add_dict_key_assignment_definitions(
        &mut self,
        targets: impl IntoIterator<Item = Expr<'ast>> + Copy,
        dict: Expr<'ast>,
        assignment: Definition<'db>,
    ) {
        // TODO: Although we synthesize place expressions for each dictionary key, the definition
        // is still uniquely associated with the AST node of the key expression, and so multiple target
        // places cannot refer to the same key.
        let Ok(target) = targets.into_iter().exactly_one() else {
            return;
        };

        if let Some(target) = MemberExprBuilder::visit_expr(target) {
            self.add_dict_key_assignment_definitions_impl(&target, dict, assignment);
        }
    }

    fn add_dict_key_assignment_definitions_impl(
        &mut self,
        target: &MemberExprBuilder,
        expr: Expr<'ast>,
        assignment: Definition<'db>,
    ) {
        let kind = expr.classify();
        let ExprKind::Dict(dict) = kind else {
            let items = match kind {
                ExprKind::List(list) => list.elts(),
                ExprKind::Tuple(tuple) => tuple.elts(),
                _ => return,
            };

            // Traverse into nested collections that may contain dictionary literals.
            for (i, item) in items
                .iter()
                // Ignore starred expressions and any elements that follow them, as we cannot
                // determine the index to narrow on.
                .take_while(|e| !e.is_starred_expr())
                .enumerate()
            {
                let target = target.with_integer_subscript(i);
                self.add_dict_key_assignment_definitions_impl(&target, item, assignment);
            }
            return;
        };

        for item in dict.items() {
            let Some(key) = item.key() else {
                if let ExprKind::Dict(unpacked) = item.value().classify() {
                    // The unpacked keys belong to the same target. Recording them in order
                    // lets later items replace earlier bindings, including nested keys.
                    self.add_dict_key_assignment_definitions_impl(
                        target,
                        unpacked.into(),
                        assignment,
                    );
                }
                continue;
            };

            let Some(member_expr) = MemberExprBuilder::visit_subscript_expr(target, key) else {
                continue;
            };

            if let Some(place_expr) = PlaceExpr::try_from_member_expr(member_expr.clone()) {
                let place_id = self.add_place(place_expr);

                self.add_definition(
                    place_id,
                    DictKeyAssignmentNodeRef {
                        key,
                        assignment,
                        value: item.value(),
                    },
                );

                // Recurse into nested dictionaries.
                //
                // Note that we must do this _after_ adding the outer place in order to track
                // sub-member places correctly.
                self.add_dict_key_assignment_definitions_impl(
                    &member_expr,
                    item.value(),
                    assignment,
                );
            }
        }
    }

    /// Create loop header definitions for places that are bound or invalidated within a loop.
    /// Return the `LoopHeaderId` referenced by those definitions, the set of place IDs, and the
    /// lower bound `ScopedDefinitionId` for definitions created within the loop.
    fn synthesize_loop_header_definitions(
        &mut self,
        loop_stmt: LoopStmtRef<'ast>,
        bound_places: Vec<PlaceExpr>,
    ) -> (LoopHeaderId, FxHashSet<ScopedPlaceId>, ScopedDefinitionId) {
        let loop_header_id = self.current_use_def_map_mut().reserve_loop_header();
        let bound_places: Vec<_> = bound_places
            .into_iter()
            .map(|place| self.add_place(place))
            .collect();

        // Rebinding `x` also invalidates `x.attr` and `x[index]`. These places need their own
        // headers so that the invalidation reaches uses before the assignment on later
        // iterations. Register all explicit targets first to include their associated places.
        let associated_places: Vec<_> = bound_places
            .iter()
            .flat_map(|place| self.current_place_table().associated_place_ids(*place))
            .copied()
            .map(ScopedPlaceId::from)
            .collect();
        let mut bound_place_ids: FxHashSet<ScopedPlaceId> = FxHashSet::default();
        for place_id in bound_places.into_iter().chain(associated_places) {
            if bound_place_ids.insert(place_id) {
                let loop_header_ref = LoopHeaderDefinitionNodeRef {
                    loop_stmt,
                    place: place_id,
                    loop_header_id,
                };
                // Note that `DefinitionKind::LoopHeader` doesn't shadow prior bindings.
                self.push_additional_definition(place_id, loop_header_ref);
            }
        }
        let loop_min_definition_id = self.current_use_def_map_mut().next_definition_id();
        (loop_header_id, bound_place_ids, loop_min_definition_id)
    }

    /// Build a `LoopHeader` that tracks all the variables bound in a loop, which will be visible
    /// to uses in the same loop via "loop header definitions". We call this after merging control
    /// flow from all the loop-back edges, most importantly at the end of the loop body, and also
    /// at any `continue` statements.
    fn populate_loop_header(
        &mut self,
        loop_header_places: &FxHashSet<ScopedPlaceId>,
        loop_header_id: LoopHeaderId,
        loop_min_definition_id: ScopedDefinitionId,
    ) {
        let mut loop_header = LoopHeader::new();
        let use_def = self.current_use_def_map_mut();
        // Collect all the bindings within the loop that reached a loop back edge. Use the minimum
        // definition ID to filter out all the pre-loop bindings. The loop header doesn't shadow
        // them, so there's no need to duplicate them.
        #[expect(
            clippy::iter_over_hash_type,
            reason = "bindings for distinct places are collected independently"
        )]
        for place_id in loop_header_places {
            for live_binding in use_def.current_bindings(*place_id) {
                if live_binding.binding() >= loop_min_definition_id {
                    loop_header.add_binding(*place_id, live_binding);
                }
            }
        }
        // Mark the reachability and narrowing constraints as used.
        #[expect(
            clippy::iter_over_hash_type,
            reason = "marking reachability constraints as used is idempotent"
        )]
        for place_id in loop_header_places {
            for live_binding in loop_header.bindings_for_place(*place_id) {
                use_def
                    .reachability_constraints
                    .mark_used(live_binding.reachability_constraint());
                use_def
                    .narrowing_constraints
                    .mark_used(live_binding.narrowing_constraint());
            }
        }
        use_def.set_loop_header(loop_header_id, loop_header);
    }

    fn synthesize_nested_binding_definitions(
        &mut self,
        nested_bindings: NestedGlobalOrNonlocalDeclarations,
    ) {
        let mut nested_bindings = nested_bindings.into_iter().collect::<Vec<_>>();
        nested_bindings.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

        for (name, mut declarations) in nested_bindings {
            // Filter down to only the declarations with `is_bound: true`. If there are none left,
            // skip synthesizing a definition for this symbol. (The reason we track these at all is
            // that we reuse some of the same machinery to report semantic syntax errors for
            // invalid `nonlocal`s, and those don't necessarily need a binding.)
            declarations.retain(|d| d.is_bound);
            declarations.shrink_to_fit();
            if declarations.is_empty() {
                continue;
            }

            let place: ScopedPlaceId = self.add_symbol(name.clone()).into();
            let definition = Definition::new(
                self.db,
                self.current_scope_id(),
                place,
                DefinitionKind::NestedBindings(Box::new(NestedBindingsDefinitionKind {
                    name,
                    execution: NestedBindingExecution::Lazy,
                    nested_declarations: declarations,
                })),
                false,
            );

            // Adding a binding typically invalidates narrowing aliases like
            // `is_int = isinstance(x, int)`. However, for the same reason that we retain both
            // `global` and `nonlocal` nested writes -- we don't necessarily know yet which ones
            // are going to be visible in the current scope -- it's also too early to know whether
            // we should invalidate narrowing aliases. Situations where this matters tend to be
            // *very* contrived, though, for example:
            //
            // ```py
            // x: int | str = 1
            // def _(x: int | str):
            //     is_int = isinstance(x, int)
            //     def _():
            //         global x
            //         x = "hello"
            //     if is_int:
            //         # We should narrow `x` to `int` here, because the global `x` is a different variable.
            //         reveal_type(x)
            // ```
            //
            // TODO: We could be more precise here by delaying invalidation until inference time.
            self.invalidate_narrowing_aliases_for(place);

            self.current_use_def_map_mut().record_binding(
                place,
                definition,
                // Nested bindings definitions are like loop headers in that they don't shadow
                // prior bindings, but they're different in that they *also* don't get shadowed by
                // bindings that come later. The idea is that nested functions can be called at any
                // time, so these bindings are effectively always visible after their function
                // definitions.
                PreviousDefinitions::AreKept,
                FutureDefinitions::DontShadowThisOne,
                ImportedQualifierAction::Preserve,
            );
        }
    }

    /// Records assignment-expression bindings from a comprehension in its containing scope.
    ///
    /// The value expression still belongs to the comprehension scope, so the real definition
    /// stays there. The synthetic definition lets the containing scope observe that binding while
    /// retaining the comprehension's scope for type inference.
    ///
    /// ```python
    /// [(last := item) for item in items]
    /// print(last)  # `last` is owned by this containing scope.
    /// ```
    fn synthesize_comprehension_binding_definitions(
        &mut self,
        nested_bindings: NestedGlobalOrNonlocalDeclarations,
    ) {
        let mut nested_bindings = nested_bindings.into_iter().collect::<Vec<_>>();
        nested_bindings.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

        for (name, mut declarations) in nested_bindings {
            // Ignore declarations used only to validate `nonlocal` syntax.
            declarations.retain(|d| d.is_bound);
            declarations.shrink_to_fit();
            let Some(first_declaration) = declarations.first().copied() else {
                continue;
            };

            let binding_status = self.comprehension_binding_status(&name, &declarations);

            let symbol = self.add_symbol(name.clone());
            debug_assert!(
                declarations
                    .iter()
                    .all(|declaration| declaration.is_global() == first_declaration.is_global())
            );
            self.forward_comprehension_binding(&name, first_declaration, symbol);

            let place: ScopedPlaceId = symbol.into();
            if binding_status == LiveBindingStatus::Unbound {
                self.mark_place_bound(place);
                continue;
            }

            let definition = Definition::new(
                self.db,
                self.current_scope_id(),
                place,
                DefinitionKind::NestedBindings(Box::new(NestedBindingsDefinitionKind {
                    name,
                    execution: NestedBindingExecution::Eager,
                    nested_declarations: declarations,
                })),
                false,
            );
            let previous = if binding_status == LiveBindingStatus::Bound {
                PreviousDefinitions::AreShadowed
            } else {
                PreviousDefinitions::AreKept
            };
            self.record_definition(place, definition, Some(previous));
        }
    }

    /// Summarizes whether the comprehension's live exit paths bind `name`.
    ///
    /// For example, `value` is only possibly bound after this comprehension because the walrus is
    /// skipped when `flag` is false:
    ///
    /// ```python
    /// [(value := item) if flag else None for item in items]
    /// ```
    fn comprehension_binding_status(
        &mut self,
        name: &str,
        declarations: &[NestedDeclaration],
    ) -> LiveBindingStatus {
        let mut status = LiveBindingStatus::Unbound;
        for declaration in declarations {
            let scope_id = declaration.file_scope_id;
            let Some(symbol) = self.place_tables[scope_id].symbol_id(name) else {
                continue;
            };
            match self.use_def_maps[scope_id].symbol_live_binding_status(symbol) {
                LiveBindingStatus::Bound => return LiveBindingStatus::Bound,
                LiveBindingStatus::PossiblyBound => status = LiveBindingStatus::PossiblyBound,
                LiveBindingStatus::Unbound => {}
            }
        }
        status
    }

    /// Passes a walrus binding out through nested comprehensions.
    ///
    /// ```python
    /// [[(last := item) for item in row] for row in rows]
    /// print(last)  # `last` belongs to the scope outside both comprehensions.
    /// ```
    ///
    /// Each comprehension passes the binding out one level. This preserves the order and
    /// conditions under which the assignment is evaluated.
    fn forward_comprehension_binding(
        &mut self,
        name: &Name,
        first_declaration: NestedDeclaration,
        symbol: ScopedSymbolId,
    ) {
        if self.scopes[self.current_scope()].kind() != ScopeKind::Comprehension {
            return;
        }

        self.current_scope_info_mut()
            .nested_global_or_nonlocal_declarations
            .remove(name);

        if first_declaration.is_global() {
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_global();
        } else {
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_nonlocal();
        }
        self.current_scope_info_mut()
            .this_scope_global_or_nonlocal_declarations
            .entry(name.clone())
            .or_insert(first_declaration.range);
    }

    /// Marks a comprehension walrus target as a write to the containing Python scope.
    ///
    /// The iteration variable remains local to the comprehension, while the walrus target does
    /// not:
    ///
    /// ```python
    /// [(result := item) for item in items]
    /// print(result)  # valid
    /// print(item)    # `item` is not defined here
    /// ```
    fn mark_comprehension_named_target(&mut self, place: ScopedPlaceId, range: TextRange) {
        if self.scopes[self.current_scope()].kind() != ScopeKind::Comprehension {
            return;
        }
        if self.semantic_syntax_errors.borrow().iter().any(|error| {
            matches!(
                error.kind,
                SemanticSyntaxErrorKind::ReboundComprehensionVariable
                    | SemanticSyntaxErrorKind::NamedExpressionInComprehensionIterable
            ) && error.range.contains_range(range)
        }) {
            return;
        }

        let Some(symbol) = place.as_symbol() else {
            return;
        };
        let name = self.current_place_table().symbol(symbol).name().clone();
        let Some(containing_scope) = self.scope_stack.iter().rev().find(|scope_info| {
            self.scopes[scope_info.file_scope_id].kind() != ScopeKind::Comprehension
        }) else {
            return;
        };

        let containing_scope_id = containing_scope.file_scope_id;
        let is_global = match self.scopes[containing_scope_id].kind() {
            ScopeKind::Module => true,
            ScopeKind::Function | ScopeKind::Lambda => self.place_tables[containing_scope_id]
                .symbol_id(&name)
                .is_some_and(|symbol| {
                    self.place_tables[containing_scope_id]
                        .symbol(symbol)
                        .is_global()
                }),
            // Assignment expressions are invalid in comprehensions directly contained by these
            // scopes. Leave the recovered target local to the comprehension.
            ScopeKind::Class | ScopeKind::TypeAlias | ScopeKind::TypeParams => return,
            ScopeKind::Comprehension => return,
        };

        if is_global {
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_global();
        } else {
            let (containing_symbol, added) =
                self.place_tables[containing_scope_id].add_symbol(Symbol::new(name.clone()));
            if added {
                self.use_def_maps[containing_scope_id].add_place(containing_symbol.into());
            }

            let containing_symbol =
                self.place_tables[containing_scope_id].symbol_mut(containing_symbol);
            if !containing_symbol.is_nonlocal() && !containing_symbol.is_bound() {
                containing_symbol.mark_bound();
            }

            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_nonlocal();
        }
        self.current_scope_info_mut()
            .this_scope_global_or_nonlocal_declarations
            .insert(name, range);
    }

    fn record_expression_narrowing_constraint(
        &mut self,
        predicate_node: Expr<'ast>,
    ) -> (PredicateOrLiteral<'db>, ScopedPredicateId) {
        let predicate = self.build_predicate(predicate_node, ExpressionContext::Condition, None);
        let predicate_id = self.record_narrowing_constraint(predicate);
        (predicate, predicate_id)
    }

    fn build_predicate(
        &mut self,
        predicate_node: Expr<'ast>,
        context: ExpressionContext,
        registered: Option<Expression<'db>>,
    ) -> PredicateOrLiteral<'db> {
        // Some commonly used test expressions are eagerly evaluated as `true`
        // or `false` here for performance reasons. This list does not need to
        // be exhaustive. More complex expressions will still evaluate to the
        // correct value during type-checking.
        fn resolve_to_literal(node: Expr<'_>) -> Option<bool> {
            match node.fields() {
                ast::ExprFields::BooleanLiteral(ast::ExprBooleanLiteralFields {
                    value, ..
                }) => Some(value),
                node if is_if_type_checking(node.view()) => Some(true),
                ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields {
                    value: Number::Int(n),
                    ..
                }) => Some(*n != 0),
                ast::ExprFields::EllipsisLiteral(_) => Some(true),
                ast::ExprFields::Lambda(_) | ast::ExprFields::Generator(_) => Some(true),
                ast::ExprFields::NoneLiteral(_) => Some(false),
                ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields {
                    op: UnaryOp::Not,
                    operand,
                    ..
                }) => Some(!resolve_to_literal(operand)?),
                _ => None,
            }
        }

        self.register_narrowing_alias_predicates(predicate_node);

        if let Some(literal) = resolve_to_literal(predicate_node) {
            if registered.is_none() {
                self.add_standalone_expression(predicate_node);
            }
            return PredicateOrLiteral::Literal(literal);
        }

        let expression =
            registered.unwrap_or_else(|| self.add_predicate_expression(predicate_node, context));
        PredicateOrLiteral::Predicate(Predicate {
            node: PredicateNode::Expression(expression),
            is_positive: true,
        })
    }

    fn visit_condition(&mut self, test: Expr<'ast>) {
        self.visit_boolean_test(test, ExpressionContext::Condition);
    }

    /// Visit a boolean test.
    ///
    /// A boolean test is an expression that Python tests for truthiness, such as an `if`
    /// condition or the operand of a `not` expression. ty's `redundant-condition` and
    /// `redundant-condition-strict` rules can warn when such a test is always true or
    /// always false. The rules try hard to avoid emitting duplicate diagnostics on the
    /// same boolean test, however: in this case, a naive implementation would emit two
    /// diagnostics on the `if` test, since there is both an `if` condition that is always
    /// falsy and a `not` operand that is always truthy:
    ///
    /// ```py
    /// def func(): ...
    ///
    /// if not func:  # one diagnostic, or two?
    ///     pass
    /// ```
    ///
    /// To determine whether diagnostics on boolean-test subexpressions need to be suppressed,
    /// the rules need to be able to reliably know whether a boolean test is nested within
    /// another outer boolean test. The [`super::UseDefMap`] answers these queries by storing
    /// [`NodeIndex`]es for each outermost boolean test in any given scope.
    ///
    /// [`Self::visit_boolean_test`] is responsible for instructing the [`UseDefMapBuilder`]
    /// to record a new outermost boolean test. The method is called whenever we visit a boolean
    /// test such as an `if` condition, `while` condition, `assert` condition or `not` operand,
    /// and it consults the [`Self::active_boolean_test_scope`] field to inform its decision:
    ///
    /// - If the field is `None`, it knows that the expression currently being visited is not
    ///   nested inside another outermost boolean test, so the expression currently being
    ///   visited should be recorded
    /// - If the field is `Some(scope)`, the scope must be compared against the scope currently
    ///   being visited. If it's a new scope, it should be recorded; if it's the same scope, it
    ///   should not.
    ///
    /// As further examples, in the following snippet, `ready` is a function object, which is
    /// always truthy. Each use of `not ready` should therefore produce a warning suggesting a
    /// call to `ready`:
    ///
    /// ```python
    /// def ready() -> bool:
    ///     return True
    ///
    /// def consume(value: object) -> bool:
    ///     return bool(value)
    ///
    /// if consume(not ready):
    ///     print("Condition passed")
    ///
    /// if consume(lambda: not ready):
    ///     print("Condition passed")
    /// ```
    ///
    /// For the first `if`, we record `consume(not ready)`. Its diagnostic check will also
    /// examine the test of `ready`, so we do not record `ready` separately. While visiting the
    /// call's arguments, this field keeps the call's scope ID even though the arguments are
    /// evaluated with `ExpressionContext::Value`.
    ///
    /// For the second `if`, the `lambda` body is checked independently in its own scope. Storing
    /// the scope ID lets `visit_boolean_test` recognize that the test of `ready` inside the
    /// `lambda` needs its own entry. This field is temporary visitor state; only the recorded
    /// expression node IDs are retained in the semantic index.
    fn visit_boolean_test(&mut self, test: Expr<'ast>, context: ExpressionContext) {
        let scope = self.current_scope();
        let previous = self.active_boolean_test_scope.replace(scope);
        if previous != Some(scope) {
            self.current_use_def_map_mut()
                .record_boolean_test_root(test.node_index());
        }
        self.visit_expr_with_context(test, context);
        self.active_boolean_test_scope = previous;
    }

    /// Adds a new predicate to the list of all predicates, but does not record it. Returns the
    /// predicate ID for later recording using
    /// [`SemanticIndexBuilder::record_narrowing_constraint_id_for_places`].
    fn add_predicate(&mut self, predicate: PredicateOrLiteral<'db>) -> ScopedPredicateId {
        self.current_use_def_map_mut().add_predicate(predicate)
    }

    /// Negates a predicate and adds it to the list of all predicates, does not record it.
    fn add_negated_predicate(&mut self, predicate: PredicateOrLiteral<'db>) -> ScopedPredicateId {
        self.current_use_def_map_mut()
            .add_predicate(predicate.negated())
    }

    /// Records a previously added narrowing constraint by adding it to the live bindings
    /// of the specified places.
    fn record_narrowing_constraint_id_for_places(
        &mut self,
        predicate: ScopedPredicateId,
        places: &PossiblyNarrowedPlaces,
    ) {
        self.current_use_def_map_mut()
            .record_narrowing_constraint_for_places(predicate, places);
    }

    /// Adds and records a narrowing constraint for only the places that could possibly be narrowed.
    ///
    /// Returns the `ScopedPredicateId` for the positive predicate, which can later be passed to
    /// `record_negated_narrowing_constraint` to record the opposite result of the same check.
    fn record_narrowing_constraint(
        &mut self,
        predicate: PredicateOrLiteral<'db>,
    ) -> ScopedPredicateId {
        let possibly_narrowed = self.compute_possibly_narrowed_places(&predicate);
        let use_def = self.current_use_def_map_mut();
        let predicate_id = use_def.add_predicate(predicate);
        use_def.record_narrowing_constraint_for_places(predicate_id, &possibly_narrowed);
        predicate_id
    }

    /// Computes the conservative set of places that could possibly be narrowed by a predicate.
    ///
    /// This uses the closure-based approach to avoid calling Salsa queries that depend on
    /// the semantic index (which is still being built).
    fn compute_possibly_narrowed_places(
        &self,
        predicate: &PredicateOrLiteral<'db>,
    ) -> PossiblyNarrowedPlaces {
        match predicate {
            PredicateOrLiteral::Literal(_) => PossiblyNarrowedPlaces::default(),
            PredicateOrLiteral::Predicate(pred) => {
                let place_table = self.current_place_table();

                match pred.node {
                    PredicateNode::Expression(expression) => {
                        let expression_node = expression.node_ref(self.db).node(self.module);
                        let mut places = PossiblyNarrowedPlacesBuilder::new(self.db, place_table)
                            .expression(expression_node);
                        self.add_alias_narrowed_places(expression_node, &mut places);
                        places
                    }
                    PredicateNode::Pattern(pattern) => {
                        let module = self.module;
                        PossiblyNarrowedPlacesBuilder::new(self.db, place_table)
                            .pattern(pattern, module)
                    }
                    PredicateNode::SubjectElementPattern(_)
                    | PredicateNode::IsNonTerminalCall(_)
                    | PredicateNode::ContextManagerSuppresses { .. }
                    | PredicateNode::FinallyNormalPathImpossible { .. }
                    | PredicateNode::IsNonEmptyIterable(_)
                    | PredicateNode::OrPatternAlternative(_)
                    | PredicateNode::StarImportPlaceholder(_) => {
                        // These predicates don't narrow any places
                        PossiblyNarrowedPlaces::default()
                    }
                }
            }
        }
    }

    /// Negates the given predicate and then adds it as a narrowing constraint to the places
    /// that could possibly be narrowed.
    ///
    /// Takes the `ScopedPredicateId` from the positive recording so that both constraints refer to
    /// the same check. This lets the positive and negative cases cancel after a complete
    /// `if`/`else`.
    fn record_negated_narrowing_constraint(
        &mut self,
        predicate: PredicateOrLiteral<'db>,
        predicate_id: ScopedPredicateId,
    ) {
        let possibly_narrowed = self.compute_possibly_narrowed_places(&predicate);
        self.current_use_def_map_mut()
            .record_negated_narrowing_constraint_for_places(predicate_id, &possibly_narrowed);
    }

    /// Records that all remaining statements in the current block are unreachable.
    fn mark_unreachable(&mut self) {
        self.current_use_def_map_mut().mark_unreachable();
    }

    /// Records that the current state can enter any active `finally` suites before the current
    /// terminal control-flow transfer reaches its destination.
    fn record_terminal_finally_entry(&mut self) {
        let mut exception_context_stack_manager =
            std::mem::take(&mut self.exception_context_stack_manager);
        exception_context_stack_manager.record_terminal_finally_entry(self);
        self.exception_context_stack_manager = exception_context_stack_manager;
    }

    /// Returns whether an exception raised while evaluating `scope` can propagate directly to its
    /// enclosing scope.
    ///
    /// Generator expressions follow the eager comprehension-scope convention used throughout our
    /// flow model. Although generators are lazy at runtime, their bodies are assumed to execute
    /// immediately, since in practice they are almost always eagerly iterated over.
    ///
    /// ```python
    /// try:
    ///     (may_raise() for _ in [0])
    /// except Exception:
    ///     ...
    /// ```
    fn exception_checkpoint_crosses_scope_boundary(&self, scope_id: FileScopeId) -> bool {
        self.scopes[scope_id].is_eager()
    }

    /// Records the current flow state immediately before an operation that may raise an exception.
    ///
    /// This models exceptions from ordinary operations, not every possible interruption. In
    /// particular, we do not add arbitrary exception points for asynchronously raised exceptions
    /// such as those originating in signal handlers.
    ///
    /// Child expressions must already have been visited, so their completed assignments are
    /// visible if the parent operation fails:
    ///
    /// ```python
    /// state = 0
    /// try:
    ///     may_raise(state := 1)
    /// except Exception:
    ///     reveal_type(state)  # Literal[1]
    /// ```
    ///
    /// Skips snapshot construction when no enclosing `try` or `with` context can handle exceptions.
    fn record_exception_checkpoint(&mut self) {
        if !self
            .exception_context_stack_manager
            .has_active_exception_handler(self)
        {
            return;
        }

        let mut exception_context_stack_manager =
            std::mem::take(&mut self.exception_context_stack_manager);
        exception_context_stack_manager.record_exception_checkpoint(self);
        self.exception_context_stack_manager = exception_context_stack_manager;
    }

    fn record_exception_checkpoint_if(&mut self, can_raise: bool) {
        if can_raise {
            self.record_exception_checkpoint();
        }
    }

    /// Returns whether accessing a name, attribute, or subscript can raise.
    ///
    /// Only a definitely bound name in the current flow state is known to be safe. In particular,
    /// a builtin-looking name may be shadowed by a local binding that has not been visited yet.
    fn place_access_can_raise(&mut self, expr: Expr<'_>, is_use: bool) -> bool {
        let ExprKind::Name(name) = expr.classify() else {
            return true;
        };

        is_use
            && self
                .exception_context_stack_manager
                .has_active_exception_handler(self)
            && self
                .current_place_table()
                .symbol_id(name.id().as_str())
                .is_none_or(|symbol| {
                    self.current_use_def_map_mut()
                        .symbol_live_binding_status(symbol)
                        != LiveBindingStatus::Bound
                })
    }

    /// Returns whether evaluating and truth-testing `expr` cannot invoke Python user code.
    ///
    /// Identity comparisons are safe, but testing an arbitrary value may call `__bool__`:
    ///
    /// ```python
    /// if value is None: ...  # safe
    /// if value: ...  # can raise
    /// ```
    fn condition_evaluation_is_known_safe(expr: Expr<'_>) -> bool {
        if expr.is_literal_expr() || matches!(expr.classify(), ExprKind::Lambda(_)) {
            return true;
        }

        match expr.classify() {
            ExprKind::Named(named) if named.target().is_name_expr() => {
                Self::condition_evaluation_is_known_safe(named.value())
            }
            ExprKind::List(_) | ExprKind::Tuple(_) => {
                Self::expression_evaluation_is_known_safe(expr)
            }
            ExprKind::BoolOp(expr_bool_op) => {
                let values = expr_bool_op.values();

                values.iter().all(Self::condition_evaluation_is_known_safe)
            }
            ExprKind::UnaryOp(expr_unary_op) if expr_unary_op.op() == UnaryOp::Not => {
                let operand = expr_unary_op.operand();
                Self::condition_evaluation_is_known_safe(operand)
            }
            ExprKind::Compare(expr_compare) => {
                let ops = expr_compare.ops();
                let operands = expr_compare.operands();

                ops.iter().all(|op| matches!(op, CmpOp::Is | CmpOp::IsNot))
                    && operands
                        .iter()
                        .all(Self::expression_evaluation_is_known_safe)
            }
            _ => false,
        }
    }

    /// Returns whether evaluating `expr` cannot invoke Python user code.
    ///
    /// Unlike [`Self::condition_evaluation_is_known_safe`], this does not truth-test the resulting
    /// value, so loading a name is safe even when that value's `__bool__` method could raise.
    fn expression_evaluation_is_known_safe(expr: Expr<'_>) -> bool {
        if expr.is_literal_expr()
            || matches!(expr.classify(), ExprKind::Name(_) | ExprKind::Lambda(_))
        {
            return true;
        }

        match expr.fields() {
            ast::ExprFields::Named(ast::ExprNamedFields { node: named, .. })
                if named.target().is_name_expr() =>
            {
                Self::expression_evaluation_is_known_safe(named.value())
            }
            ast::ExprFields::List(ast::ExprListFields { elts, .. })
            | ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. }) => {
                elts.iter().all(Self::expression_evaluation_is_known_safe)
            }
            ast::ExprFields::Compare(_) => Self::condition_evaluation_is_known_safe(expr),
            _ => false,
        }
    }

    /// Returns whether iterating `expr` uses an exact builtin iterator that cannot raise anything
    /// other than `StopIteration` (ignoring ambient failures such as `MemoryError`).
    ///
    /// ```python
    /// for value in [1, 2]: ...  # safe
    /// for value in values: ...  # can invoke user-defined iteration
    /// ```
    fn iteration_is_known_safe(expr: Expr<'_>) -> bool {
        matches!(
            expr.classify(),
            ExprKind::StringLiteral(_)
                | ExprKind::BytesLiteral(_)
                | ExprKind::List(_)
                | ExprKind::Tuple(_)
        ) && Self::expression_evaluation_is_known_safe(expr)
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

    fn push_statement(&mut self, statement: CurrentStatement<'ast, 'db>) {
        self.current_statements.push(statement);
    }

    fn pop_statement(&mut self) -> CurrentStatement<'ast, 'db> {
        self.current_statements.pop().unwrap()
    }

    fn current_statement_mut(&mut self) -> Option<&mut CurrentStatement<'ast, 'db>> {
        self.current_statements.last_mut()
    }

    /// Return whether a pattern contains any capture that changes the current flow state.
    fn pattern_has_bindings(pattern: Pattern<'_>) -> bool {
        match pattern.fields() {
            ast::PatternFields::MatchValue(_) | ast::PatternFields::MatchSingleton(_) => false,
            ast::PatternFields::MatchSequence(ast::PatternMatchSequenceFields {
                patterns, ..
            })
            | ast::PatternFields::MatchOr(ast::PatternMatchOrFields { patterns, .. }) => {
                patterns.iter().any(Self::pattern_has_bindings)
            }
            ast::PatternFields::MatchMapping(ast::PatternMatchMappingFields { items, .. }) => {
                items.into_iter().any(|item| match item {
                    MappingPatternItem::Entry { pattern, .. } => {
                        Self::pattern_has_bindings(pattern)
                    }
                    MappingPatternItem::Rest(_) => true,
                })
            }
            ast::PatternFields::MatchClass(ast::PatternMatchClassFields {
                node: pattern, ..
            }) => pattern
                .arguments()
                .patterns()
                .iter()
                .chain(
                    pattern
                        .arguments()
                        .keywords()
                        .iter()
                        .map(PatternKeyword::pattern),
                )
                .any(Self::pattern_has_bindings),
            ast::PatternFields::MatchStar(ast::PatternMatchStarFields {
                node: pattern, ..
            }) => pattern.name().is_some(),
            ast::PatternFields::MatchAs(ast::PatternMatchAsFields { node: pattern, .. }) => {
                pattern.name().is_some()
                    || pattern.pattern().is_some_and(Self::pattern_has_bindings)
            }
        }
    }

    /// Returns whether matching a pattern can invoke Python user code.
    fn pattern_can_raise(pattern: Pattern<'_>) -> bool {
        match pattern.classify() {
            PatternKind::MatchValue(_)
            | PatternKind::MatchSequence(_)
            | PatternKind::MatchMapping(_)
            | PatternKind::MatchClass(_) => true,
            PatternKind::MatchSingleton(_) | PatternKind::MatchStar(_) => false,
            PatternKind::MatchAs(pattern) => pattern.pattern().is_some_and(Self::pattern_can_raise),
            PatternKind::MatchOr(pattern) => pattern.patterns().iter().any(Self::pattern_can_raise),
        }
    }

    fn predicate_kind(&mut self, pattern: Pattern<'_>) -> PatternPredicateKind<'db> {
        match pattern.classify() {
            PatternKind::MatchValue(pattern) => {
                let value = self.add_standalone_expression(pattern.value());
                PatternPredicateKind::Value(value)
            }
            PatternKind::MatchSingleton(singleton) => {
                PatternPredicateKind::Singleton(singleton.value())
            }
            PatternKind::MatchClass(pattern) => {
                let cls = self.add_standalone_expression(pattern.cls());

                PatternPredicateKind::Class(ClassPatternPredicateKind {
                    class: cls,
                    positional: pattern
                        .arguments()
                        .patterns()
                        .iter()
                        .map(|pattern| self.predicate_kind(pattern))
                        .collect(),
                    keywords: pattern
                        .arguments()
                        .keywords()
                        .iter()
                        .map(|keyword| ClassPatternKeywordPredicateKind {
                            attr: keyword.attr().id().clone(),
                            pattern: self.predicate_kind(keyword.pattern()),
                        })
                        .collect(),
                })
            }
            PatternKind::MatchMapping(pattern) => {
                // Retain keyed entries for subject-aware exhaustiveness analysis.
                PatternPredicateKind::Mapping(MappingPatternPredicateKind {
                    entries: pattern
                        .items()
                        .entries()
                        .map(|(key, pattern)| MappingPatternEntryPredicateKind {
                            key: self.add_standalone_expression(key),
                            pattern: self.predicate_kind(pattern),
                        })
                        .collect(),
                    rest: pattern.items().rest().map(|name| name.id().clone()),
                })
            }
            PatternKind::MatchSequence(pattern) => {
                PatternPredicateKind::Sequence(SequencePatternPredicateKind {
                    patterns: pattern
                        .patterns()
                        .iter()
                        .map(|pattern| self.predicate_kind(pattern))
                        .collect(),
                })
            }
            PatternKind::MatchOr(pattern) => {
                let predicates = pattern
                    .patterns()
                    .iter()
                    .map(|pattern| self.predicate_kind(pattern))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                PatternPredicateKind::Or(predicates)
            }
            PatternKind::MatchAs(pattern) => PatternPredicateKind::As(
                pattern.pattern().map(|p| Box::new(self.predicate_kind(p))),
                pattern.name().map(|name| name.id().clone()),
            ),
            PatternKind::MatchStar(pattern) => {
                PatternPredicateKind::Star(pattern.name().map(|name| name.id().clone()))
            }
        }
    }

    fn create_pattern_predicate(
        &mut self,
        subject: Expression<'db>,
        pattern: Pattern<'_>,
        guard: Option<Expr<'_>>,
        previous_pattern: Option<PatternPredicate<'db>>,
    ) -> PatternPredicate<'db> {
        // This is called for the top-level pattern of each match arm. We need to create a
        // standalone expression for each arm of a match statement, since they can introduce
        // constraints on the match subject. (Or more accurately, for the match arm's pattern,
        // since its the pattern that introduces any constraints, not the body.) Ideally, that
        // standalone expression would wrap the match arm's pattern as a whole. But a standalone
        // expression can currently only wrap an Expr, which patterns are not. So, we need to
        // choose an Expr that can "stand in" for the pattern, which we can wrap in a standalone
        // expression.
        //
        // See the comment in TypeInferenceBuilder::infer_match_pattern for more details.

        let kind = self.predicate_kind(pattern);
        let guard =
            guard.map(|guard| self.add_predicate_expression(guard, ExpressionContext::Condition));

        PatternPredicate::new(
            self.db,
            self.file,
            self.current_scope(),
            subject,
            kind,
            guard,
            previous_pattern.map(Box::new),
        )
    }

    fn add_pattern_narrowing_constraint(
        &mut self,
        pattern_predicate: PatternPredicate<'db>,
        subject_targets: &[(ScopedPlaceId, SmallVec<[ScopedDefinitionId; 2]>)],
        sequence_subject_targets: &[(ScopedPlaceId, ScopedUseId, ExpressionNodeKey)],
        is_catchall: bool,
    ) -> (PredicateOrLiteral<'db>, ScopedPredicateId) {
        let predicate = PredicateOrLiteral::Predicate(Predicate {
            node: PredicateNode::Pattern(pattern_predicate),
            is_positive: true,
        });

        // For the last catchall case (irrefutable wildcard without guard), we skip
        // recording the narrowing constraint from the pattern. The accumulated negated
        // constraints from earlier cases (~P1, ~P2, ...) are sufficient. This ensures
        // `P1 OR (~P1 AND P2) OR (~P1 AND ~P2)` simplifies to ALWAYS_TRUE, preserving
        // the original type after an exhaustive match. The reachability and pattern
        // predicates are still created normally for proper control flow tracking.
        let predicate_id = if is_catchall {
            ScopedPredicateId::ALWAYS_TRUE
        } else if subject_targets.is_empty() && sequence_subject_targets.is_empty() {
            self.record_narrowing_constraint(predicate)
        } else {
            let predicate_id = self.add_predicate(predicate);
            for (place, bindings) in subject_targets {
                self.current_use_def_map_mut()
                    .record_narrowing_constraint_for_bindings(predicate_id, *place, bindings);
            }
            for &(place, use_id, target) in sequence_subject_targets {
                let subject_element_id =
                    self.add_predicate(PredicateOrLiteral::Predicate(Predicate {
                        node: PredicateNode::SubjectElementPattern(
                            SubjectElementPatternPredicate {
                                pattern: pattern_predicate,
                                target,
                            },
                        ),
                        is_positive: true,
                    }));
                self.current_use_def_map_mut()
                    .record_narrowing_constraint_for_bindings_at_use(
                        subject_element_id,
                        place,
                        use_id,
                    );
            }
            predicate_id
        };
        (predicate, predicate_id)
    }

    /// Retain captures that bind the entire subject to its original name. These definitions
    /// still refer to the saved subject if a guard fails, so later cases can narrow them.
    fn extend_match_subject_bindings(
        &self,
        pattern: Pattern<'_>,
        symbol: ScopedSymbolId,
        bindings: &mut SmallVec<[ScopedDefinitionId; 2]>,
    ) {
        match pattern.classify() {
            PatternKind::MatchAs(pattern) => {
                if let Some(name) = pattern.name()
                    && let Some(definition) = self
                        .definitions_by_node
                        .get(&name.into())
                        .and_then(|definitions| definitions.first())
                {
                    let use_def = self.current_use_def_map();
                    bindings.extend(use_def.symbol_binding_definition_ids(symbol).filter(
                        |binding| use_def.definition(*binding).definition() == Some(*definition),
                    ));
                }
                if let Some(pattern) = pattern.pattern() {
                    self.extend_match_subject_bindings(pattern, symbol, bindings);
                }
            }
            PatternKind::MatchOr(pattern) => {
                for pattern in pattern.patterns() {
                    self.extend_match_subject_bindings(pattern, symbol, bindings);
                }
            }
            // Structural patterns can capture extracted values. Identifying whole-subject
            // captures in class patterns requires type information.
            _ => {}
        }
    }

    /// Record an expression that needs to be a Salsa ingredient, because we need to infer its type
    /// standalone (type narrowing tests, RHS of an assignment.)
    fn add_standalone_expression(&mut self, expression_node: Expr<'_>) -> Expression<'db> {
        self.add_standalone_expression_impl(expression_node, ExpressionKind::Normal, None)
    }

    /// Record a predicate in the context in which Python tests it.
    fn add_predicate_expression(
        &mut self,
        node: Expr<'_>,
        context: ExpressionContext,
    ) -> Expression<'db> {
        self.add_standalone_expression_impl(
            node,
            ExpressionKind::Predicate {
                context,
                in_loop: self.in_loop_context(),
            },
            None,
        )
    }

    /// Record an expression that is immediately assigned to a target, and that needs to be a Salsa
    /// ingredient, because we need to infer its type standalone (type narrowing tests, RHS of an
    /// assignment.)
    fn add_standalone_assigned_expression(
        &mut self,
        expression_node: Expr<'_>,
        assigned_to: StmtAssign<'_>,
    ) -> Expression<'db> {
        self.add_standalone_expression_impl(
            expression_node,
            ExpressionKind::Normal,
            Some(assigned_to),
        )
    }

    /// Same as [`SemanticIndexBuilder::add_standalone_expression`], but marks the expression as a
    /// *type* expression, which makes sure that it will later be inferred as such.
    fn add_standalone_type_expression(&mut self, expression_node: Expr<'_>) -> Expression<'db> {
        self.add_standalone_expression_impl(expression_node, ExpressionKind::TypeExpression, None)
    }

    fn add_standalone_expression_impl(
        &mut self,
        expression_node: Expr<'_>,
        expression_kind: ExpressionKind,
        assigned_to: Option<StmtAssign<'_>>,
    ) -> Expression<'db> {
        let expression = Expression::new(
            self.db,
            self.current_scope_id(),
            AstNodeRef::from_view(self.module, expression_node),
            assigned_to.map(|assigned_to| AstNodeRef::from_view(self.module, assigned_to)),
            expression_kind,
        );
        self.expressions_by_node
            .insert(expression_node.into(), expression);
        expression
    }

    fn add_standalone_statement(&mut self, statement_node: Stmt<'_>) -> Statement<'db> {
        // Avoid allocating a salsa ingredient if the statement represents an existing
        // definition or standalone expression.
        let statement = match statement_node.classify() {
            StmtKind::FunctionDef(function) => Some(Statement::Definition(
                self.expect_single_definition(function),
            )),
            StmtKind::ClassDef(class) => {
                Some(Statement::Definition(self.expect_single_definition(class)))
            }
            StmtKind::Expr(stmt_expr) => {
                let value = stmt_expr.value();

                Some(Statement::Expression(self.add_standalone_expression(value)))
            }
            StmtKind::Assign(assign) => {
                if let Some([target]) = assign.targets().as_array()
                    && let Some(name) = target.as_name_expr()
                {
                    Some(Statement::Definition(self.expect_single_definition(name)))
                } else {
                    None
                }
            }
            StmtKind::AnnAssign(assign) if assign.target().is_name_expr() => {
                Some(Statement::Definition(self.expect_single_definition(assign)))
            }
            StmtKind::AugAssign(assign) if assign.target().is_name_expr() => {
                Some(Statement::Definition(self.expect_single_definition(assign)))
            }
            StmtKind::TypeAlias(alias) => {
                Some(Statement::Definition(self.expect_single_definition(alias)))
            }
            _ => None,
        };

        let statement = if let Some(statement) = statement {
            statement
        } else {
            Statement::Other(StatementInner::new(
                self.db,
                self.file,
                self.current_scope(),
                AstNodeRef::from_view(self.module, statement_node),
            ))
        };

        self.statements_by_node
            .insert(statement_node.into(), statement);

        statement
    }

    fn with_type_params<T>(
        &mut self,
        with_scope: NodeWithScopeRef,
        type_params: Option<TypeParams<'ast>>,
        nested: impl FnOnce(&mut Self) -> T,
    ) -> T {
        if let Some(type_params) = type_params {
            self.push_scope(with_scope);

            for type_param in type_params.type_params() {
                let (name, bound, default) = match type_param.classify() {
                    TypeParamKind::TypeVar(type_param_type_var) => {
                        let name = type_param_type_var.name();
                        let bound = type_param_type_var.bound();
                        let default = type_param_type_var.default();
                        (name, bound, default)
                    }
                    TypeParamKind::ParamSpec(type_param_param_spec) => {
                        let name = type_param_param_spec.name();
                        let default = type_param_param_spec.default();
                        (name, None, default)
                    }
                    TypeParamKind::TypeVarTuple(type_param_type_var_tuple) => {
                        let name = type_param_type_var_tuple.name();
                        let default = type_param_type_var_tuple.default();
                        (name, None, default)
                    }
                };
                self.scopes_by_expression
                    .record_expression(&name, self.current_scope());
                let symbol = self.add_symbol(name.id().clone());
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
                match type_param.classify() {
                    TypeParamKind::TypeVar(node) => self.add_definition(symbol.into(), node),
                    TypeParamKind::ParamSpec(node) => self.add_definition(symbol.into(), node),
                    TypeParamKind::TypeVarTuple(node) => self.add_definition(symbol.into(), node),
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
    /// [`Comprehension`]: Comprehension
    fn with_generators_scope(
        &mut self,
        scope: NodeWithScopeRef,
        generators: NodeList<'ast, ruff_python_ast::kind::Comprehension>,
        visit_outer_elt: impl FnOnce(&mut Self),
    ) -> FileScopeId {
        let mut generators_iter = generators.iter();

        let Some(generator) = generators_iter.next() else {
            unreachable!("Expression must contain at least one generator");
        };

        // The `iter` of the first generator is evaluated in the outer scope, while all subsequent
        // nodes are evaluated in the inner scope.
        let value = self.add_standalone_expression(generator.iter());
        self.visit_expr(generator.iter());
        let first_iteration_can_raise =
            generator.is_async() || !Self::iteration_is_known_safe(generator.iter());
        self.record_exception_checkpoint_if(first_iteration_can_raise);
        let mut loopback_can_raise =
            first_iteration_can_raise || !generator.target().is_name_expr();

        // Clear the assignment stack before entering the comprehension scope.
        // If the comprehension appears inside an assignment target (e.g., error-recovered
        // `arr[::[x for *b in y for (b: _` is parsed as `StmtAnnAssign`), the outer
        // assignment context must not leak into the inner scope.
        let saved_assignments = std::mem::take(&mut self.current_assignments);

        self.push_scope(scope);
        let comprehension_scope = self.current_scope();

        if generators.iter().any(Comprehension::is_async) {
            self.async_comprehensions.insert(comprehension_scope);
        }

        self.add_unpackable_assignment(
            &Unpackable::Comprehension {
                node: generator,
                first: true,
            },
            generator.target(),
            value,
        );

        let mut filtered_out_paths = Vec::new();
        for if_expr in generator.ifs() {
            filtered_out_paths.push(self.visit_comprehension_filter(if_expr));
        }

        for generator in generators_iter {
            let value = self.add_standalone_expression(generator.iter());
            self.visit_expr(generator.iter());
            let iteration_can_raise =
                generator.is_async() || !Self::iteration_is_known_safe(generator.iter());
            self.record_exception_checkpoint_if(iteration_can_raise);
            loopback_can_raise |= iteration_can_raise || !generator.target().is_name_expr();

            self.add_unpackable_assignment(
                &Unpackable::Comprehension {
                    node: generator,
                    first: false,
                },
                generator.target(),
                value,
            );

            for if_expr in generator.ifs() {
                filtered_out_paths.push(self.visit_comprehension_filter(if_expr));
            }
        }

        visit_outer_elt(self);
        for filtered_out_path in filtered_out_paths {
            self.flow_merge(filtered_out_path);
        }
        self.record_exception_checkpoint_if(loopback_can_raise);
        let nested_bindings = self.pop_scope();
        self.synthesize_comprehension_binding_definitions(nested_bindings);
        self.record_exception_checkpoint();

        self.current_assignments = saved_assignments;

        comprehension_scope
    }

    /// Visits a comprehension filter on its truthy path and returns the filtered-out path.
    ///
    /// A false filter skips the rest of the current iteration, but assignments performed while
    /// evaluating the filter remain observable:
    ///
    /// ```python
    /// [item for item in items if (last := item)]
    /// print(last)
    /// ```
    fn visit_comprehension_filter(&mut self, if_expr: Expr<'ast>) -> FlowSnapshot {
        self.visit_condition(if_expr);
        let condition_flow_snapshot = self.flow_snapshot_for_condition(if_expr);
        let filtered_out = if let Some(snapshots) = condition_flow_snapshot.into_branches() {
            self.flow_restore(snapshots.truthy);
            snapshots.falsy
        } else {
            self.flow_snapshot()
        };

        let (predicate, narrowing_id) = self.record_expression_narrowing_constraint(if_expr);
        let reachability_constraint = self.record_reachability_constraint_id(narrowing_id);
        let included_path = self.flow_snapshot();

        self.flow_restore(filtered_out);
        self.record_negated_narrowing_constraint(predicate, narrowing_id);
        self.record_negated_reachability_constraint(reachability_constraint);
        let filtered_out = self.flow_snapshot();

        self.flow_restore(included_path);
        filtered_out
    }

    fn declare_parameters(&mut self, parameters: Parameters<'ast>) {
        for parameter in parameters.iter_non_variadic_params() {
            self.declare_parameter(parameter);
        }
        if let Some(vararg) = parameters.vararg() {
            let symbol = self.add_symbol(vararg.name().id().clone());
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_parameter();
            self.add_definition(
                symbol.into(),
                ParameterDefinitionNodeRef::VariadicPositionalParameter(vararg),
            );
        }
        if let Some(kwarg) = parameters.kwarg() {
            let symbol = self.add_symbol(kwarg.name().id().clone());
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_parameter();
            self.add_definition(
                symbol.into(),
                ParameterDefinitionNodeRef::VariadicKeywordParameter(kwarg),
            );
        }
    }

    fn declare_parameter(&mut self, parameter: ParameterWithDefault<'ast>) {
        let symbol = self.add_symbol(parameter.name().id().clone());

        self.add_definition(
            symbol.into(),
            ParameterDefinitionNodeRef::Parameter(parameter),
        );

        self.current_place_table_mut()
            .symbol_mut(symbol)
            .mark_parameter();
    }

    fn declare_lambda_parameters(
        &mut self,
        parameters: Parameters<'ast>,
        lambda: ExprLambda<'ast>,
    ) {
        let mut index = 0;
        for parameter in parameters.posonlyargs() {
            self.declare_lambda_parameter(index, parameter, lambda);
            index += 1;
        }
        for parameter in parameters.args() {
            self.declare_lambda_parameter(index, parameter, lambda);
            index += 1;
        }
        if let Some(vararg) = parameters.vararg() {
            let symbol = self.add_symbol(vararg.name().id().clone());
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_parameter();
            self.add_definition(
                symbol.into(),
                LambdaParameterDefinitionNodeRef {
                    index,
                    lambda,
                    parameter: ParameterDefinitionNodeRef::VariadicPositionalParameter(vararg),
                },
            );
            index += 1;
        }
        for parameter in parameters.kwonlyargs() {
            self.declare_lambda_parameter(index, parameter, lambda);
            index += 1;
        }
        if let Some(kwarg) = parameters.kwarg() {
            let symbol = self.add_symbol(kwarg.name().id().clone());
            self.current_place_table_mut()
                .symbol_mut(symbol)
                .mark_parameter();
            self.add_definition(
                symbol.into(),
                LambdaParameterDefinitionNodeRef {
                    index,
                    lambda,
                    parameter: ParameterDefinitionNodeRef::VariadicKeywordParameter(kwarg),
                },
            );
        }
    }

    fn declare_lambda_parameter(
        &mut self,
        index: usize,
        parameter: ParameterWithDefault<'ast>,
        lambda: ExprLambda<'ast>,
    ) {
        let symbol = self.add_symbol(parameter.name().id().clone());

        self.add_definition(
            symbol.into(),
            LambdaParameterDefinitionNodeRef {
                index,
                lambda,
                parameter: ParameterDefinitionNodeRef::Parameter(parameter),
            },
        );

        self.current_place_table_mut()
            .symbol_mut(symbol)
            .mark_parameter();
    }

    /// Add an unpackable assignment for the given [`Unpackable`].
    ///
    /// This method handles assignments that can contain unpacking like assignment statements,
    /// for statements, etc.
    fn add_unpackable_assignment(
        &mut self,
        unpackable: &Unpackable<'ast>,
        target: Expr<'ast>,
        value: Expression<'db>,
    ) {
        self.record_exception_checkpoint_if(matches!(
            target.classify(),
            ExprKind::List(_) | ExprKind::Tuple(_)
        ));

        let current_assignment = match target.classify() {
            ExprKind::List(_) | ExprKind::Tuple(_) => {
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
                let unpack = Unpack::new(
                    self.db,
                    self.file,
                    value_file_scope,
                    self.current_scope(),
                    // Note `target` belongs to the `self.module` tree
                    AstNodeRef::from_view(self.module, target),
                    UnpackValue::new(unpackable.kind(), value),
                );
                self.unpacks_by_target.insert(target.into(), unpack);
                Some(unpackable.as_current_assignment(Some(unpack)))
            }
            ExprKind::Name(_)
            | ExprKind::Starred(_)
            | ExprKind::Attribute(_)
            | ExprKind::Subscript(_) => Some(unpackable.as_current_assignment(None)),
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
        self.visit_body(self.module.root().body());

        // Pop the root scope
        self.pop_scope();
        self.sweep_nonlocal_lazy_snapshots();
        assert!(self.scope_stack.is_empty());

        assert_eq!(&self.current_assignments, &[]);

        let ast_ids = self.ast_ids.finish();

        let mut semantic_syntax_errors = self.semantic_syntax_errors.into_inner();
        semantic_syntax_errors.shrink_to_fit();
        self.annotations
            .sort_unstable_by_key(|&row| self.module.get_by_index(row).start());
        let uses_by_collection = FrozenMap::from_entries(
            self.uses_by_collection
                .into_iter()
                .map(|(definition, uses)| (definition, uses.into_boxed_slice()))
                .collect(),
        );

        let mut use_def_map_interner = UseDefMapInterner::default();
        let mut interned_place_tables: FxHashMap<u64, SmallVec<[Arc<PlaceTable>; 1]>> =
            FxHashMap::default();
        let place_tables = self
            .place_tables
            .into_iter()
            .map(|builder| {
                let table = builder.finish();
                // Empty tables are cheap and do not need a lookup in the interner.
                if table.symbols().next().is_none() && table.members().next().is_none() {
                    return Arc::new(table);
                }

                let hash = FxBuildHasher.hash_one(&table);
                if let Some(existing) = interned_place_tables.get(&hash).and_then(|candidates| {
                    candidates
                        .iter()
                        .find(|candidate| candidate.as_ref() == &table)
                }) {
                    return Arc::clone(existing);
                }

                let table = Arc::new(table);
                interned_place_tables
                    .entry(hash)
                    .or_default()
                    .push(Arc::clone(&table));
                table
            })
            .collect();

        let scopes_by_expression = self.scopes_by_expression.build(&mut self.scopes);
        SemanticIndex {
            place_tables,
            scopes: self.scopes.into(),
            definitions_by_node: DefinitionsByNode::from_map(self.definitions_by_node),
            expressions_by_node: self.expressions_by_node,
            unpacks_by_target: FrozenMap::from(self.unpacks_by_target),
            statements_by_node: self.statements_by_node,
            scope_ids_by_scope: self.scope_ids_by_scope.into(),
            ast_ids,
            scopes_by_expression,
            scopes_by_node: self.scopes_by_node,
            use_def_maps: self
                .use_def_maps
                .into_iter()
                .map(|builder| use_def_map_interner.intern(builder.finish()))
                .collect(),
            enclosing_lambda_statements: FrozenMap::from(self.enclosing_lambda_statements),
            collections_by_use: FrozenMap::from(self.collections_by_use),
            uses_by_collection,
            imported_modules: FrozenSet::from(self.imported_modules),
            has_future_annotations: self.has_future_annotations,
            enclosing_snapshots: FrozenMap::from(self.enclosing_snapshots),
            semantic_syntax_errors,
            generator_functions: FrozenSet::from(self.generator_functions),
            async_comprehensions: FrozenSet::from(self.async_comprehensions),
            annotations: self.annotations.into_boxed_slice(),
            narrowing_alias_predicates: FrozenMap::from(self.alias_predicates),
        }
    }

    fn with_semantic_checker(&mut self, f: impl FnOnce(&mut SemanticSyntaxChecker, &Self)) {
        let mut checker = self.semantic_checker;
        f(&mut checker, self);
        self.semantic_checker = checker;
    }

    fn source_text(&self) -> &SourceText {
        self.source_text
            .get_or_init(|| source_text(self.db, self.file.file(self.db)))
    }

    fn visit_expr_with_context(&mut self, expr: Expr<'ast>, context: ExpressionContext) {
        SemanticSyntaxChecker::check_expr(expr, self);

        self.scopes_by_expression
            .record_expression(&expr, self.current_scope());

        match expr.classify() {
            ExprKind::Name(node) => {
                self.visit_place_expression(expr, node.ctx());
            }
            ExprKind::Attribute(node) => {
                self.visit_place_expression(expr, node.ctx());
            }
            ExprKind::Subscript(node) => {
                self.visit_place_expression(expr, node.ctx());
            }
            ExprKind::Named(node) => {
                self.visit_expr(node.value());

                // See https://peps.python.org/pep-0572/#differences-between-assignment-expressions-and-assignment-statements
                if node.target().is_name_expr() {
                    self.push_assignment(CurrentAssignment::Named(node));
                    self.visit_expr(node.target());
                    self.pop_assignment();
                } else {
                    self.visit_expr(node.target());
                }
            }
            ExprKind::Lambda(lambda) => {
                self.current_statement_mut()
                    .expect("every lambda expression is part of a statement")
                    .lambda_expressions
                    .push(lambda);

                if let Some(parameters) = lambda.parameters() {
                    // The default value of the parameters needs to be evaluated in the
                    // enclosing scope.
                    for default in parameters
                        .iter_non_variadic_params()
                        .filter_map(ParameterWithDefault::default)
                    {
                        self.visit_expr(default);
                    }
                    self.visit_parameters(parameters);
                }
                self.push_scope(NodeWithScopeRef::Lambda(lambda));

                // Add symbols and definitions for the parameters to the lambda scope.
                if let Some(parameters) = lambda.parameters() {
                    self.declare_lambda_parameters(parameters, lambda);
                }

                self.visit_expr(lambda.body());
                self.pop_scope();
            }
            ExprKind::If(node) => self.visit_if_expression(node, context),
            ExprKind::ListComp(list_comprehension) => {
                let elt = list_comprehension.elt();
                let generators = list_comprehension.generators();

                let scope = self.with_generators_scope(
                    NodeWithScopeRef::ListComprehension(list_comprehension),
                    generators,
                    |builder| builder.visit_expr(elt),
                );
                if self.async_comprehensions.contains(&scope) {
                    self.mark_current_comprehension_async();
                }
            }
            ExprKind::SetComp(set_comprehension) => {
                let elt = set_comprehension.elt();
                let generators = set_comprehension.generators();

                let scope = self.with_generators_scope(
                    NodeWithScopeRef::SetComprehension(set_comprehension),
                    generators,
                    |builder| builder.visit_expr(elt),
                );
                if self.async_comprehensions.contains(&scope) {
                    self.mark_current_comprehension_async();
                }
            }
            ExprKind::Generator(generator) => {
                let elt = generator.elt();
                let generators = generator.generators();

                self.with_generators_scope(
                    NodeWithScopeRef::GeneratorExpression(generator),
                    generators,
                    |builder| builder.visit_expr(elt),
                );
            }
            ExprKind::DictComp(dict_comprehension) => {
                let key = dict_comprehension.key();
                let value = dict_comprehension.value();
                let generators = dict_comprehension.generators();

                let scope = self.with_generators_scope(
                    NodeWithScopeRef::DictComprehension(dict_comprehension),
                    generators,
                    |builder| {
                        if let Some(key) = key {
                            builder.visit_expr(key);
                        }
                        builder.visit_expr(value);
                    },
                );
                if self.async_comprehensions.contains(&scope) {
                    self.mark_current_comprehension_async();
                }
            }
            ExprKind::Call(_) | ExprKind::BinOp(_) => {
                walk_expr(self, expr);
                self.record_exception_checkpoint();
            }
            ExprKind::UnaryOp(unary) => {
                if unary.op() == UnaryOp::Not {
                    self.visit_boolean_test(unary.operand(), context);
                } else {
                    self.visit_expr_with_context(unary.operand(), ExpressionContext::Value);
                }
                self.record_exception_checkpoint_if(
                    unary.op() != UnaryOp::Not
                        || !Self::condition_evaluation_is_known_safe(unary.operand()),
                );
            }
            ExprKind::Compare(compare) => {
                self.visit_expr(compare.first_operand());
                for (_, op, right) in compare.iter() {
                    self.visit_expr(right);
                    self.record_exception_checkpoint_if(!matches!(op, CmpOp::Is | CmpOp::IsNot));
                }
            }
            ExprKind::BoolOp(node) => self.visit_bool_expression(node, context),
            ExprKind::StringLiteral(_) => {
                walk_expr(self, expr);
            }
            ExprKind::Yield(_) | ExprKind::YieldFrom(_) => {
                let scope = self.current_scope();
                if self.scopes[scope].kind() == ScopeKind::Function {
                    self.generator_functions.insert(scope);
                }
                walk_expr(self, expr);
                self.record_exception_checkpoint();
            }
            ExprKind::Await(_) => {
                self.mark_current_comprehension_async();
                walk_expr(self, expr);
                self.record_exception_checkpoint();
            }
            _ => {
                walk_expr(self, expr);
            }
        }
    }

    fn visit_place_expression(&mut self, expr: Expr<'ast>, ctx: ExprContext) {
        // Record place effects after walking the expression. For names, this is
        // equivalent because `walk_expr` is a no-op; for attribute/subscript places,
        // child evaluation can introduce bindings (for example via walrus operators),
        // and those bindings need to exist before we register parent/member associations.
        let mut deferred_effects = None;
        if let Some(mut place_expr) = PlaceExpr::try_from_expr(expr) {
            if let Some(method_scope_id) = self.is_method_or_eagerly_executed_in_method()
                && let PlaceExpr::Member(member) = &mut place_expr
                && member.is_instance_attribute_candidate()
                && let Some(attribute) = expr.as_attribute_expr()
            {
                // We specifically mark direct attribute assignments to the first
                // parameter of a method, i.e. typically `self` or `cls`.
                // However, we must check that the symbol hasn't been shadowed by an
                // intermediate scope (e.g., a comprehension variable: `for self in [...]`)
                // and that the AST base is still the original name rather than a
                // rebinding expression such as `(self := other).x`.
                let accessed_object_refers_to_first_parameter =
                    self.current_first_parameter_name.is_some_and(|first| {
                        attribute
                            .value()
                            .as_name_expr()
                            .is_some_and(|name| name.id() == first)
                            && !self.is_symbol_bound_in_intermediate_eager_scopes(
                                first,
                                method_scope_id,
                            )
                    });

                if accessed_object_refers_to_first_parameter {
                    member.mark_instance_attribute();
                }
            }

            let (is_use, is_definition) = match (ctx, self.current_assignment()) {
                (ExprContext::Store, Some(CurrentAssignment::AugAssign(_))) => {
                    // Record the target load now; the definition is recorded separately
                    // after visiting the right-hand side.
                    (true, false)
                }
                (ExprContext::Load, _) => (true, false),
                (ExprContext::Store, _) => (false, true),
                (ExprContext::Del, _) => (true, true),
                (ExprContext::Invalid, _) => (false, false),
            };
            deferred_effects = Some((place_expr, is_use, is_definition));
        }

        walk_expr(self, expr);

        let is_use = deferred_effects
            .as_ref()
            .is_some_and(|(_, is_use, _)| *is_use);
        let can_raise = self.place_access_can_raise(expr, is_use);
        self.record_exception_checkpoint_if(can_raise);

        if let Some((place_expr, is_use, is_definition)) = deferred_effects {
            let place_id = self.add_place(place_expr);

            if is_use {
                self.record_place_use(place_id, expr);

                // Keep track of any uses of unannotated collection initializers.
                if let Some(collection_def) = self.unannotated_collection_initializer_binding(expr)
                    && let Some(current_statement) = self.current_statements.last_mut()
                {
                    current_statement
                        .collection_uses
                        .push((collection_def, expr.into()));
                }
            }

            if is_definition {
                self.record_place_definition(place_id, expr);
            }

            if let Some(unpack_position) = self
                .current_assignment_mut()
                .and_then(CurrentAssignment::unpack_position_mut)
            {
                *unpack_position = UnpackPosition::Other;
            }
        }
    }

    /// Visits a conditional expression without reserving its flow snapshots in every recursive
    /// expression-visitor frame. This matters for deeply nested expressions in unoptimized builds.
    fn visit_if_expression(&mut self, node: ExprIf<'ast>, context: ExpressionContext) {
        let body = node.body();
        let test = node.test();
        let orelse = node.orelse();
        self.visit_condition(test);
        let condition_flow_snapshot = self.flow_snapshot_for_condition(test);
        let falsy = if let Some(snapshots) = condition_flow_snapshot.into_branches() {
            self.flow_restore(snapshots.truthy);
            snapshots.falsy
        } else {
            self.flow_snapshot()
        };
        let (predicate, predicate_id) = self.record_expression_narrowing_constraint(test);
        let reachability_constraint = self.record_reachability_constraint_id(predicate_id);
        let in_type_checking_block = self.in_type_checking_block;
        self.current_use_def_map_mut()
            .record_range_reachability(body.range(), in_type_checking_block);
        self.visit_expr_with_context(body, context);
        let post_body = self.flow_snapshot();
        self.flow_restore(falsy);

        self.record_negated_narrowing_constraint(predicate, predicate_id);
        self.record_negated_reachability_constraint(reachability_constraint);
        let in_type_checking_block = self.in_type_checking_block;
        self.current_use_def_map_mut()
            .record_range_reachability(orelse.range(), in_type_checking_block);
        self.visit_expr_with_context(orelse, context);
        self.flow_merge(post_body);
    }

    /// Keeps short-circuit flow snapshots out of the common recursive expression-visitor frame.
    fn visit_bool_expression(&mut self, node: ExprBoolOp<'ast>, context: ExpressionContext) {
        let values = node.values();
        let op = node.op();
        let mut snapshots = vec![];
        let has_named_expression = values
            .iter()
            .any(|value| any_over_expr(value, ruff_python_ast::Expr::is_named_expr));
        let mut reachability_constraints = SmallVec::<[ScopedReachabilityConstraintId; 1]>::new();
        let mut last_condition_flow_snapshots = None;

        for (index, value) in values.iter().enumerate() {
            for id in &reachability_constraints {
                self.current_use_def_map_mut()
                    .record_reachability_constraint(*id); // TODO: nicer API
            }

            let in_type_checking_block = self.in_type_checking_block;
            self.current_use_def_map_mut()
                .record_range_reachability(value.range(), in_type_checking_block);
            self.visit_expr_with_context(value, context);

            // Only non-final values can short-circuit this boolean operation. The final
            // value can still have its own outcome-specific flow if it is nested.
            if index < values.len() - 1 {
                self.record_exception_checkpoint_if(!Self::condition_evaluation_is_known_safe(
                    value,
                ));
                let condition_flow_snapshots = self.take_condition_flow_snapshots(value);
                let predicate = self.build_predicate(value, context, None);
                let possibly_narrowed = self.compute_possibly_narrowed_places(&predicate);
                let predicate_id = match op {
                    BoolOp::And => self.add_predicate(predicate),
                    BoolOp::Or => self.add_negated_predicate(predicate),
                };
                let reachability_constraint = self
                    .current_reachability_constraints_mut()
                    .add_atom(predicate_id);

                let continuation = if let Some(condition_flow_snapshots) = condition_flow_snapshots
                {
                    let (short_circuit, continuation) =
                        condition_flow_snapshots.into_short_circuit_and_continuation(op);
                    self.flow_restore(short_circuit);
                    continuation
                } else {
                    self.flow_snapshot()
                };

                // This path reaches the current operand and then short-circuits.
                self.record_negated_reachability_constraint(reachability_constraint);
                snapshots.push(self.flow_snapshot());

                self.flow_restore(continuation);
                self.record_narrowing_constraint_id_for_places(predicate_id, &possibly_narrowed);
                // The continuation carries the conditions that led to this operand. Named
                // expressions can introduce new bindings that still need the earlier gates.
                if !has_named_expression {
                    reachability_constraints.clear();
                }
                reachability_constraints.push(reachability_constraint);
            } else {
                last_condition_flow_snapshots = self.take_condition_flow_snapshots(value);
            }
        }

        let has_specialized_last = last_condition_flow_snapshots.is_some();
        let (last_short_circuit, no_short_circuit) =
            if let Some(condition_flow_snapshots) = last_condition_flow_snapshots {
                let (short_circuit, no_short_circuit) =
                    condition_flow_snapshots.into_short_circuit_and_continuation(op);
                (Some(short_circuit), Some(no_short_circuit))
            } else {
                (None, has_named_expression.then(|| self.flow_snapshot()))
            };

        if let Some(last_short_circuit) = last_short_circuit {
            self.flow_restore(last_short_circuit);
        }

        for snapshot in snapshots {
            self.flow_merge(snapshot);
        }

        if let Some(no_short_circuit) = no_short_circuit {
            let bool_op_key = ExpressionNodeKey::from(Expr::from(node));
            let maybe_short_circuit = self.flow_snapshot();

            if has_specialized_last {
                // Restore the merged post-expression flow after constructing the two
                // outcome-specific snapshots.
                self.flow_merge(no_short_circuit.clone());
            }

            let (truthy, falsy) = match op {
                BoolOp::And => (no_short_circuit, maybe_short_circuit),
                BoolOp::Or => (maybe_short_circuit, no_short_circuit),
            };

            self.condition_flow_snapshots_by_node
                .insert(bool_op_key, ConditionFlowSnapshots { truthy, falsy });
        }
    }

    fn visit_stmt_impl(&mut self, stmt: Stmt<'ast>) {
        self.with_semantic_checker(|semantic, context| semantic.visit_stmt(stmt, context));

        let in_type_checking_block = self.in_type_checking_block;
        self.current_use_def_map_mut()
            .record_range_reachability(stmt.range(), in_type_checking_block);

        match stmt.classify() {
            StmtKind::FunctionDef(function_def) => {
                let ast::StmtFunctionDefFields {
                    decorator_list,
                    parameters,
                    type_params,
                    name,
                    returns,
                    body,
                    is_async: _,
                    range: _,
                    node: _,
                } = function_def.fields();
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                // Evaluate default args before we visit the body. If the default expression ends
                // up looking at locally bound variables, `nonlocal` or `global` assignments in the
                // body shouldn't affect their inferred values. For example:
                // ```
                // x = 1
                // def f(y=reveal_type(x)):  # Literal[1]
                //     global x
                //     x = 2
                // reveal_type(x)  # Literal[1, 2]
                // ```
                for default in parameters
                    .iter_non_variadic_params()
                    .filter_map(ParameterWithDefault::default)
                {
                    self.visit_expr(default);
                }

                let nested_bindings = self.with_type_params(
                    NodeWithScopeRef::FunctionTypeParameters(function_def),
                    type_params,
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
                            .map(|first_param| first_param.parameter().name().id().as_str());
                        std::mem::swap(
                            &mut builder.current_first_parameter_name,
                            &mut first_parameter_name,
                        );

                        builder.visit_body(body);

                        builder.current_first_parameter_name = first_parameter_name;
                        builder.pop_scope()
                    },
                );

                // The nested bindings returned by `pop_scope` are exactly the ones that are
                // potentially visible at this point. That is, they include `global` and `nonlocal`
                // declarations in the popped functions body and any nested bodies, but they omit
                // the ones that resolved to the popped body. Synthesize a definition to record
                // them. This definition type has special shadowing behavior, so it doesn't shadow
                // prior bindings, and it remains visible after subsequent bindings. This
                // represents the fact that the nested function could be called at any time.
                //
                // NOTE: This is deliberately somewhat unsound. For example, bindings from parent
                // functions and sibling functions can also be visible at any point, depending on
                // when different functions get invoked. However, we really want examples like this
                // to do what users expect, so we accept the unsoundness here:
                //
                //     def f():
                //         x = 1
                //         def g():
                //             nonlocal x
                //             x = 2
                //         def h():
                //             nonlocal x
                //             x = 3
                //             # Technically `g` could get called at any time, including right
                //             # here. But inferring `Literal[2, 3]` here would be confusing.
                //             reveal_type(x)  # revealed: Literal[3]
                //          x = 4
                //          # On the other hand, users probably want to see 2 and 3 here, because
                //          # they're nested within this scope? Hopefully it's not too confusing.
                //          reveal_type(x)  # revealed: Literal[2, 3, 4]
                //
                // In other cases it can also be unsound that we only consider nested bindings to
                // be visible after their function definition, when in practice they could be
                // visible "before" (because nested functions can escape their lexical scope and
                // get called more than once). For more discussion of all these behaviors, see the
                // mdtest case "Visibility of `nonlocal` bindings from nested and sibling scopes"
                // and its `global` counterpart.
                self.synthesize_nested_binding_definitions(nested_bindings);

                // Decorator application can raise after defaults and annotations are evaluated.
                self.record_exception_checkpoint_if(!decorator_list.is_empty());

                // The symbol for the function name itself has to be evaluated
                // at the end to match the runtime evaluation of parameter defaults
                // and return-type annotations.
                let symbol = self.add_symbol(name.id().clone());

                // Record a use of the function name in the scope that it is defined in, so that it
                // can be used to find previously defined functions with the same name. This is
                // used to collect all the overloaded definitions of a function. This needs to be
                // done on the `Identifier` node as opposed to `ExprName` because that's what the
                // AST uses.
                let use_id = self.ast_ids.record_use(self.current_scope(), name);
                self.current_use_def_map_mut()
                    .record_use(symbol.into(), use_id);

                self.add_definition(symbol.into(), function_def);
                self.mark_symbol_used(symbol);
            }
            StmtKind::ClassDef(class) => {
                for decorator in class.decorator_list() {
                    self.visit_decorator(decorator);
                }

                let nested_bindings = self.with_type_params(
                    NodeWithScopeRef::ClassTypeParameters(class),
                    class.type_params(),
                    |builder| {
                        if let Some(arguments) = class.arguments() {
                            builder.visit_arguments(arguments);
                        }

                        builder.push_scope(NodeWithScopeRef::Class(class));
                        builder.visit_body(class.body());

                        builder.pop_scope()
                    },
                );

                // We currently treat nested `global` and `nonlocal` bindings from class bodies the
                // same way as ones from function bodies above. That's correct in the common case
                // where they actually come from a function within the class. But when they appear
                // directly within a class body, this isn't quite correct, because these synthetic
                // definitions behave lazily, while class bodies are actually evaluated eagerly.
                self.synthesize_nested_binding_definitions(nested_bindings);

                // Class construction and decorator application can raise after the body executes.
                self.record_exception_checkpoint();

                // In Python runtime semantics, a class is registered after its scope is evaluated.
                let symbol = self.add_symbol(class.name().id().clone());
                self.add_definition(symbol.into(), class);
            }
            StmtKind::TypeAlias(type_alias) => {
                let symbol = self.add_symbol(
                    type_alias
                        .name()
                        .as_name_expr()
                        .map(|name| name.id().clone())
                        .unwrap_or("<unknown>".into()),
                );
                self.add_definition(symbol.into(), type_alias);
                self.visit_expr(type_alias.name());

                self.with_type_params(
                    NodeWithScopeRef::TypeAliasTypeParameters(type_alias),
                    type_alias.type_params(),
                    |builder| {
                        builder.push_scope(NodeWithScopeRef::TypeAlias(type_alias));
                        builder.visit_expr(type_alias.value());
                        builder.pop_scope()
                    },
                );
            }
            StmtKind::Import(node) => {
                for (alias_index, alias) in node.names().iter().enumerate() {
                    self.record_exception_checkpoint();

                    // Mark the imported module, and all of its parents, as being imported in this
                    // file.
                    if let Some(module_name) = ModuleName::new(alias.name().as_str()) {
                        self.imported_modules.extend(module_name.ancestors());
                    }

                    let (symbol_name, is_reexported) = if let Some(asname) = alias.asname() {
                        self.scopes_by_expression
                            .record_expression(&asname, self.current_scope());
                        (asname.id().clone(), asname.id() == alias.name().id())
                    } else {
                        (
                            Name::new(alias.name().id().split('.').next().unwrap()),
                            false,
                        )
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
            StmtKind::ImportFrom(node) => {
                self.record_exception_checkpoint();

                // If we see:
                //
                // * `from .x.y import z` (or `from whatever.thispackage.x.y`)
                // * And we are in an `__init__.py(i)` (hereafter `thispackage`)
                // * And this is the first time we've seen `from .x` in this module
                // * And we're in the global scope
                //
                // We introduce a local definition `x = <module 'thispackage.x'>` that occurs
                // before the `z = ...` binding the import introduces. This models the fact
                // that the *first* time that you import 'thispackage.x' the python runtime creates
                // `x` as a variable in the global scope of `thispackage`.
                //
                // This is not a perfect simulation of actual runtime behaviour for *various*
                // reasons but it works well for most practical purposes. In particular it's nice
                // that `x` can be freely overwritten, and that we don't assume that an import
                // in one function is visible in another function.
                let mut is_self_import = false;
                let source_file = self.file.file(self.db);
                let resolver_environment = self.resolver_environment;
                if source_file.is_package(self.db)
                    && let Ok(module_name) = ModuleName::from_identifier_parts(
                        self.db,
                        ImportingFile::File(source_file, resolver_environment),
                        node.module().map(Identifier::as_str),
                        node.level(),
                    )
                    && let Ok(thispackage) = ModuleName::package_for_file(
                        self.db,
                        ImportingFile::File(source_file, resolver_environment),
                    )
                {
                    // Record whether this is equivalent to `from . import ...`
                    is_self_import = module_name == thispackage;

                    if node.module().is_some()
                        && let Some(relative_submodule) = module_name.relative_to(&thispackage)
                        && let Some(direct_submodule) = relative_submodule.components().next()
                        && !self.seen_submodule_imports.contains(direct_submodule)
                        && self.current_scope().is_global()
                    {
                        self.seen_submodule_imports
                            .insert(direct_submodule.to_owned());

                        let is_immediately_shadowed = node.names().iter().any(|alias| {
                            if alias.name() == "*" {
                                return false;
                            }

                            let bound_name = alias.asname().unwrap_or(alias.name());
                            bound_name.id().as_str() == direct_submodule
                        });

                        if !is_immediately_shadowed {
                            let direct_submodule_name = Name::new(direct_submodule);
                            let symbol = self.add_symbol(direct_submodule_name);

                            let module_index = if node.level() == 0 {
                                // "whatever.thispackage.x.y" we want `x`
                                thispackage.components().count()
                            } else {
                                // ".x.y" we want `x` (level 1 => index 0)
                                // "..x.y" we want `y` (level 2 => index 1)
                                // (The Identifier doesn't include the prefix dots)
                                node.level() as usize - 1
                            };
                            self.add_definition(
                                symbol.into(),
                                ImportFromSubmoduleDefinitionNodeRef { node, module_index },
                            );
                        }
                    }
                }

                let mut found_star = false;
                for (alias_index, alias) in node.names().iter().enumerate() {
                    // Loading each imported name can fail after the module import and any earlier
                    // names or package-submodule side effects have completed.
                    self.record_exception_checkpoint();

                    if alias.name() == "*" {
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

                        let Some(module) = resolve_module_for_import_from(
                            self.db,
                            ImportingFile::File(source_file, resolver_environment),
                            node,
                        ) else {
                            continue;
                        };

                        let Some(referenced_file) = module.file(self.db) else {
                            continue;
                        };
                        let referenced_program_file =
                            ProgramFile::new(self.db, referenced_file, self.file.program(self.db));
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
                        for export in exported_names(self.db, referenced_program_file) {
                            let symbol_id = self.add_symbol(export.clone());
                            let node_ref = StarImportDefinitionNodeRef { node, symbol_id };
                            let star_import = StarImportPlaceholderPredicate::new(
                                self.db,
                                self.file,
                                symbol_id,
                                referenced_program_file,
                            );

                            let star_import_predicate = self.add_predicate(star_import.into());

                            let scope = self.current_scope();
                            let associated_member_ids = self.place_tables[scope]
                                .associated_place_ids(ScopedPlaceId::Symbol(symbol_id));
                            let pre_definition = self.use_def_maps[scope]
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

                    let (symbol_name, is_reexported) = if let Some(asname) = alias.asname() {
                        self.scopes_by_expression
                            .record_expression(&asname, self.current_scope());
                        // It's re-exported if it's `from ... import x as x`
                        (asname.id(), asname.id() == alias.name().id())
                    } else {
                        // As a non-standard rule to handle stubs in the wild, we consider
                        // `from . import x` and `from whatever.thispackage import x` in an
                        // `__init__.pyi` to re-export `x` (as long as it wasn't renamed)
                        (alias.name().id(), is_self_import)
                    };

                    // Look for eager imports `from __future__ import annotations`, ignore `as ...`
                    // We intentionally don't enforce the rules about location of `__future__`
                    // imports here, we assume the user's intent was to apply the `__future__`
                    // import, so we still check using it (and will also emit a diagnostic about a
                    // miss-placed `__future__` import.)
                    self.has_future_annotations |= !node.is_lazy()
                        && alias.name().id() == "annotations"
                        && node.module().is_some_and(|name| name == "__future__");

                    let symbol = self.add_symbol(symbol_name.clone().clone());

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

            StmtKind::Assert(stmt_assert) => {
                let test = stmt_assert.test();
                let msg = stmt_assert.msg();

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
                // The other important part is the `<halt>`. This lets us skip merging the
                // `msg` branch back into the following flow, since there is no way of getting out
                // of that branch. Code after the assertion starts from the condition's truthy flow.

                self.visit_condition(test);
                let condition_flow_snapshot = self.flow_snapshot_for_condition(test);
                let predicate = self.build_predicate(test, ExpressionContext::Condition, None);

                if msg.is_some()
                    || self
                        .exception_context_stack_manager
                        .has_active_exception_handler(self)
                {
                    let truthy = if let Some(snapshots) = condition_flow_snapshot.into_branches() {
                        self.flow_restore(snapshots.falsy);
                        snapshots.truthy
                    } else {
                        self.flow_snapshot()
                    };
                    let negated_predicate = predicate.negated();
                    let predicate_id = self.record_narrowing_constraint(negated_predicate);
                    self.record_reachability_constraint_id(predicate_id);
                    if let Some(msg) = msg {
                        self.visit_expr(msg);
                    }
                    self.record_exception_checkpoint();
                    self.flow_restore(truthy);
                } else if let Some(truthy) = condition_flow_snapshot.into_truthy() {
                    self.flow_restore(truthy);
                }

                let predicate_id = self.record_narrowing_constraint(predicate);
                self.record_reachability_constraint_id(predicate_id);
            }

            StmtKind::Assign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);

                self.visit_expr(node.value());

                // Unannotated collection initializers must be standalone expressions to participate
                // in full-scope bidirectional inference.
                if node.targets().len() == 1 && is_collection_initializer(node.value()) {
                    self.add_standalone_assigned_expression(node.value(), node);
                }

                // Optimization for the common case: if there's just one target, and it's not an
                // unpacking, and the target is a simple name, we don't need the RHS to be a
                // standalone expression at all.
                if let Some([target]) = node.targets().as_array()
                    && target.is_name_expr()
                {
                    self.push_assignment(CurrentAssignment::Assign {
                        node,
                        unpack: None,
                        owner: BindingsOwner::Definition,
                    });
                    self.visit_expr(target);
                    self.pop_assignment();

                    self.try_register_narrowing_alias(target, Some(node.value()));
                } else {
                    let value = self.add_standalone_assigned_expression(node.value(), node);

                    for target in node.targets() {
                        self.add_unpackable_assignment(&Unpackable::Assign(node), target, value);
                    }
                }
            }
            StmtKind::AnnAssign(node) => {
                debug_assert_eq!(&self.current_assignments, &[]);
                // For an assignment with a value, an exception from the annotation or RHS must
                // not discard the declared type. The value is still bound only after the RHS
                // completes, so a handler can observe an earlier binding (or an unbound name).
                let pending = self.begin_annotated_assignment(node);
                self.visit_annotation(node.annotation());
                if let Some(value) = node.value() {
                    self.visit_expr(value);
                    if self.is_method_or_eagerly_executed_in_method().is_some() {
                        // Record the right-hand side of the assignment as a standalone expression
                        // if we're inside a method. This allows type inference to infer the type
                        // of the value for annotated assignments like `self.CONSTANT: Final = 1`,
                        // where the type itself is not part of the annotation.
                        self.add_standalone_expression(value);
                    }
                }

                if let ExprKind::Name(name) = node.target().classify() {
                    let symbol_id = self.add_symbol(name.id().clone());
                    let symbol = self.current_place_table().symbol(symbol_id);
                    // Check whether the variable has been declared global.
                    if symbol.is_global() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::AnnotatedGlobal(
                                name.id().as_str().into(),
                            ),
                            range: name.range(),
                            python_version: self.python_version(),
                        });
                    }
                    // Check whether the variable has been declared nonlocal.
                    if symbol.is_nonlocal() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::AnnotatedNonlocal(
                                name.id().as_str().into(),
                            ),
                            range: name.range(),
                            python_version: self.python_version(),
                        });
                    }
                }

                // See https://docs.python.org/3/library/ast.html#ast.AnnAssign
                if matches!(
                    node.target().classify(),
                    ExprKind::Attribute(_) | ExprKind::Subscript(_) | ExprKind::Name(_)
                ) {
                    self.push_assignment(CurrentAssignment::AnnAssign { node, pending });
                    self.visit_expr(node.target());
                    self.pop_assignment();

                    self.try_register_narrowing_alias(node.target(), node.value());
                } else {
                    self.visit_expr(node.target());
                }
            }
            StmtKind::AugAssign(aug_assign) => {
                let target = aug_assign.target();
                let op = aug_assign.op();
                let value = aug_assign.value();

                debug_assert_eq!(&self.current_assignments, &[]);

                // An augmented assignment loads its target before evaluating the right-hand side,
                // but only defines the target after the operation succeeds.
                let is_place_target = matches!(
                    target.classify(),
                    ExprKind::Name(_) | ExprKind::Attribute(_) | ExprKind::Subscript(_)
                );
                if is_place_target {
                    self.push_assignment(CurrentAssignment::AugAssign(aug_assign));
                    self.visit_expr(target);
                    self.pop_assignment();
                } else {
                    self.visit_expr(target);
                }

                self.visit_expr(value);

                if let ast::ExprFields::Name(ast::ExprNameFields { id, .. }) = target.fields()
                    && id == "__all__"
                    && op.is_add()
                    && self.in_module_scope()
                    && let ast::ExprFields::Attribute(ast::ExprAttributeFields {
                        value: module,
                        attr,
                        ..
                    }) = value.fields()
                    && attr == "__all__"
                {
                    self.add_standalone_expression(module);
                }

                self.record_exception_checkpoint();

                if is_place_target && let Some(place_expr) = PlaceExpr::try_from_expr(target) {
                    let place_id = self.add_place(place_expr);
                    self.push_assignment(CurrentAssignment::AugAssign(aug_assign));
                    self.record_place_definition(place_id, target);
                    self.pop_assignment();
                }
            }
            StmtKind::If(node) => {
                let final_elif_test = node
                    .elif_else_clauses()
                    .last()
                    .and_then(ElifElseClause::test);
                let mut chain_start = final_elif_test.map(|_| self.flow_snapshot());
                self.visit_condition(node.test());
                let condition_flow_snapshot = self.flow_snapshot_for_condition(node.test());
                let mut falsy = if let Some(snapshots) = condition_flow_snapshot.into_branches() {
                    self.flow_restore(snapshots.truthy);
                    snapshots.falsy
                } else {
                    self.flow_snapshot()
                };
                let (mut last_predicate, mut last_narrowing_id) =
                    self.record_expression_narrowing_constraint(node.test());
                let mut last_reachability_constraint =
                    self.record_reachability_constraint_id(last_narrowing_id);

                let is_outer_block_in_type_checking = self.in_type_checking_block;

                let if_block_in_type_checking = is_if_type_checking(node.test());

                // Track if we're in a chain that started with "not TYPE_CHECKING"
                let mut is_in_not_type_checking_chain = is_if_not_type_checking(node.test());

                self.in_type_checking_block =
                    if_block_in_type_checking || is_outer_block_in_type_checking;

                self.visit_body(node.body());

                let mut post_clauses: Vec<FlowSnapshot> = vec![];
                let elif_else_clauses = node
                    .elif_else_clauses()
                    .iter()
                    .map(|clause| (clause.test(), clause.body()));
                let has_else = node
                    .elif_else_clauses()
                    .last()
                    .is_some_and(|clause| clause.test().is_none());
                let elif_else_clauses = elif_else_clauses.chain(if has_else {
                    // if there's an `else` clause already, we don't need to add another
                    None
                } else {
                    // if there's no `else` branch, we should add a no-op `else` branch
                    Some((None, node.body().slice(0..0)))
                });

                for (clause_test, clause_body) in elif_else_clauses {
                    // snapshot after every block except the last; the last one will just become
                    // the state that we merge the other snapshots into
                    post_clauses.push(self.flow_snapshot());
                    // we can only take an elif/else branch if none of the previous ones were
                    // taken
                    self.flow_restore(falsy);

                    self.record_negated_narrowing_constraint(last_predicate, last_narrowing_id);
                    self.record_negated_reachability_constraint(last_reachability_constraint);

                    let next_falsy = if let Some(elif_test) = clause_test {
                        if final_elif_test.is_some_and(|test| test.range() == elif_test.range()) {
                            self.current_use_def_map_mut()
                                .set_if_chain_start(chain_start.take());
                        }
                        self.visit_condition(elif_test);
                        self.current_use_def_map_mut().set_if_chain_start(None);
                        // A test expression is evaluated whether the branch is taken or not
                        let condition_flow_snapshot = self.flow_snapshot_for_condition(elif_test);
                        let next_falsy =
                            if let Some(snapshots) = condition_flow_snapshot.into_branches() {
                                self.flow_restore(snapshots.truthy);
                                snapshots.falsy
                            } else {
                                self.flow_snapshot()
                            };

                        (last_predicate, last_narrowing_id) =
                            self.record_expression_narrowing_constraint(elif_test);

                        last_reachability_constraint =
                            self.record_reachability_constraint_id(last_narrowing_id);

                        Some(next_falsy)
                    } else {
                        None
                    };

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

                    // Nested conditional clauses inherit an enclosing TYPE_CHECKING context.
                    self.in_type_checking_block =
                        is_outer_block_in_type_checking || clause_in_type_checking;

                    self.visit_body(clause_body);

                    let Some(next_falsy) = next_falsy else {
                        break;
                    };
                    falsy = next_falsy;
                }

                for post_clause_state in post_clauses {
                    self.flow_merge(post_clause_state);
                }

                self.in_type_checking_block = is_outer_block_in_type_checking;
            }
            StmtKind::While(while_stmt) => {
                let test = while_stmt.test();
                let body = while_stmt.body();
                let orelse = while_stmt.orelse();

                // Pre-walk the loop to collect all the bound places, then create a loop header
                // definition for each bound place. See `struct LoopHeader` for more on this. Loop
                // header definitions store the ID of a reserved `LoopHeader` that we populate
                // after walking the body.
                let bound_places = loop_bindings_visitor::collect_while_loop_bindings(while_stmt);
                let mut maybe_loop_header_info = None;
                // Avoid allocating a `LoopHeader` if there are no bound places in this loop.
                if !bound_places.is_empty() {
                    maybe_loop_header_info = Some(self.synthesize_loop_header_definitions(
                        LoopStmtRef::While(while_stmt),
                        bound_places,
                    ));
                }

                // Visit the test expression after creating loop headers, so that loop-back values
                // are visible.
                let outer_loop = self.push_loop();
                self.visit_condition(test);
                let condition_flow_snapshot = self.flow_snapshot_for_condition(test);

                // Take the pre_loop snapshot from the post-test fallback flow before restoring the
                // condition's truthy flow for the body. This preserves the zero-iteration path for
                // the loop exit merge below.
                let pre_loop = self.flow_snapshot();
                if let Some(truthy) = condition_flow_snapshot.into_truthy() {
                    self.flow_restore(truthy);
                }
                let (predicate, predicate_id) = self.record_expression_narrowing_constraint(test);
                self.record_reachability_constraint_id(predicate_id);

                self.visit_body(body);
                let this_loop = self.pop_loop(outer_loop);

                // Loop-back bindings include everything that's visible if/when control reaches the
                // end of the loop body, and they also include everything that's visible to a
                // `continue` statement. Merge the `continue` states before collecting bindings.
                for continue_state in this_loop.continue_states {
                    self.flow_merge(continue_state);
                }

                // Collect all the loop-back bindings (including the `continue` states we just
                // merged) and populate the `LoopHeader`.
                if let Some((header_id, bound_place_ids, loop_min_definition_id)) =
                    maybe_loop_header_info
                {
                    self.populate_loop_header(&bound_place_ids, header_id, loop_min_definition_id);
                }

                self.record_exception_checkpoint_if(!Self::condition_evaluation_is_known_safe(
                    test,
                ));

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

                self.record_negated_narrowing_constraint(predicate, predicate_id);

                self.visit_body(orelse);

                // Breaking out of a while loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                for break_state in this_loop.break_states {
                    self.flow_merge(break_state);
                }
            }
            StmtKind::With(stmt_with) => {
                let items = stmt_with.items();
                let body = stmt_with.body();
                let is_async = stmt_with.is_async();

                for item in items {
                    let context_expr = item.context_expr();
                    let optional_vars = item.optional_vars();
                    self.visit_expr(context_expr);
                    self.record_exception_checkpoint();

                    self.exception_context_stack_manager
                        .push_context_manager_context();

                    if let Some(optional_vars) = optional_vars {
                        let context_manager = self.add_standalone_expression(context_expr);
                        self.add_unpackable_assignment(
                            &Unpackable::WithItem { item, is_async },
                            optional_vars,
                            context_manager,
                        );
                    }
                }

                self.visit_body(body);

                for item in items.iter().rev() {
                    let mut exceptional_entries = self
                        .exception_context_stack_manager
                        .finish_context_manager_context()
                        .into_iter();

                    if let Some(exceptional_entry) = exceptional_entries.next() {
                        let normal_exit = self.flow_snapshot();
                        if normal_exit.is_always_unreachable() {
                            self.exception_context_stack_manager
                                .record_deferred_terminal_context_manager_exit();
                        }
                        let context_expr = item.context_expr();
                        let expression = self
                            .expressions_by_node
                            .get(&ExpressionNodeKey::from(context_expr))
                            .copied()
                            .unwrap_or_else(|| self.add_standalone_expression(context_expr));
                        let predicate = PredicateOrLiteral::Predicate(Predicate {
                            node: PredicateNode::ContextManagerSuppresses {
                                expression,
                                is_async,
                            },
                            is_positive: true,
                        });
                        let predicate_id = self.add_predicate(predicate);

                        self.flow_restore(exceptional_entry);
                        for exceptional_entry in exceptional_entries {
                            self.flow_merge(exceptional_entry);
                        }

                        self.record_ambiguous_reachability();
                        let reachability_constraint = self
                            .current_reachability_constraints_mut()
                            .add_atom(predicate_id);
                        let narrowing_constraint = self
                            .current_use_def_map_mut()
                            .narrowing_constraints
                            .add_atom(predicate_id);
                        self.current_use_def_map_mut()
                            .record_non_terminal_call_constraints(
                                reachability_constraint,
                                narrowing_constraint,
                            );

                        self.flow_merge(normal_exit);
                    }

                    // A manager cannot suppress an exception raised by its own exit method, but
                    // an earlier manager or enclosing `try` statement can still receive it.
                    self.record_exception_checkpoint();
                }
            }

            StmtKind::For(for_stmt) => {
                let is_async = for_stmt.is_async();
                let target = for_stmt.target();
                let iter = for_stmt.iter();
                let body = for_stmt.body();
                let orelse = for_stmt.orelse();

                debug_assert_eq!(&self.current_assignments, &[]);

                let iter_expr = self.add_standalone_expression(iter);
                self.visit_expr(iter);
                let iteration_can_raise = is_async || !Self::iteration_is_known_safe(iter);
                self.record_exception_checkpoint_if(iteration_can_raise);

                let literal_iterable_is_non_empty = (!is_async)
                    .then(|| literal_iterable_truthiness(iter))
                    .and_then(Truthiness::into_bool);

                let (after_empty_iter, non_empty_range_constraint) =
                    match literal_iterable_is_non_empty {
                        Some(false) => {
                            let after_iter = self.flow_snapshot();
                            self.mark_unreachable();
                            (Some(after_iter), None)
                        }
                        Some(true) => (None, None),
                        None if is_direct_range_call(iter) => {
                            let after_iter = self.flow_snapshot();
                            let constraint = self.record_reachability_constraint(
                                PredicateOrLiteral::Predicate(Predicate {
                                    node: PredicateNode::IsNonEmptyIterable(iter_expr),
                                    is_positive: true,
                                }),
                            );

                            (None, Some((after_iter, constraint)))
                        }
                        None => {
                            self.record_ambiguous_reachability();
                            (None, None)
                        }
                    };

                let pre_loop = self.flow_snapshot();

                // Pre-walk the loop to collect all the bound places, then create a loop header
                // definition for each bound place. See `struct LoopHeader` for more on this. Loop
                // header definitions store the ID of a reserved `LoopHeader` that we populate
                // after walking the body.
                let bound_places = loop_bindings_visitor::collect_for_loop_bindings(for_stmt);
                let mut maybe_loop_header_info = None;
                // Avoid allocating a `LoopHeader` if there are no bound places in this loop.
                if !bound_places.is_empty() {
                    maybe_loop_header_info = Some(self.synthesize_loop_header_definitions(
                        LoopStmtRef::For(for_stmt),
                        bound_places,
                    ));
                }

                self.add_unpackable_assignment(&Unpackable::For(for_stmt), target, iter_expr);

                let outer_loop = self.push_loop();
                self.visit_body(body);
                let this_loop = self.pop_loop(outer_loop);

                // Loop-back bindings include everything that's visible if/when control reaches the
                // end of the loop body, and they also include everything that's visible to a
                // `continue` statement. Merge the `continue` states before collecting bindings.
                for continue_state in this_loop.continue_states {
                    self.flow_merge(continue_state);
                }

                // Collect all the loop-back bindings (including the `continue` states we just
                // merged) and populate the `LoopHeader`.
                if let Some((header_id, bound_place_ids, loop_min_definition_id)) =
                    maybe_loop_header_info
                {
                    self.populate_loop_header(&bound_place_ids, header_id, loop_min_definition_id);
                }

                self.record_exception_checkpoint_if(iteration_can_raise || !target.is_name_expr());

                if let Some(after_iter) = after_empty_iter {
                    self.flow_restore(after_iter);
                } else if literal_iterable_is_non_empty.is_none() {
                    // We may execute the `else` clause without ever executing the body, so merge
                    // in a zero-iteration state before visiting `else`.
                    if let Some((after_iter, non_empty_range_constraint)) =
                        non_empty_range_constraint
                    {
                        let post_loop_body = self.flow_snapshot();
                        self.flow_restore(after_iter);
                        self.record_negated_reachability_constraint(non_empty_range_constraint);
                        let no_iteration = self.flow_snapshot();
                        self.flow_restore(post_loop_body);
                        self.flow_merge(no_iteration);
                    } else {
                        self.flow_merge(pre_loop);
                    }
                }
                self.visit_body(orelse);

                // Breaking out of a `for` loop bypasses the `else` clause, so merge in the break
                // states after visiting `else`.
                for break_state in this_loop.break_states {
                    self.flow_merge(break_state);
                }
            }
            StmtKind::Match(stmt_match) => {
                let subject = stmt_match.subject();
                let cases = stmt_match.cases();

                debug_assert_eq!(self.current_match_case, None);

                let subject_expr = self.add_standalone_expression(subject);
                self.visit_expr(subject);
                if cases.is_empty() {
                    return;
                }

                // A match subject is evaluated once. Retain the bindings read by each place so
                // that case predicates constrain those values rather than later rebindings.
                let subject_places = match_subject_place_expressions(subject)
                    .into_iter()
                    .filter_map(|expression| {
                        let place = PlaceExpr::try_from_expr(expression).and_then(|place| {
                            self.current_place_table().place_id((&place).into())
                        })?;
                        Some((
                            place,
                            self.ast_ids.try_use_id(self.current_scope(), expression),
                        ))
                    })
                    .collect::<SmallVec<[_; 2]>>();
                let mut subject_targets =
                    SmallVec::<[(ScopedPlaceId, SmallVec<[ScopedDefinitionId; 2]>); 2]>::new();
                for &(place, use_id) in &subject_places {
                    let bindings = if let Some(use_id) = use_id {
                        self.current_use_def_map()
                            .bindings_at_use(use_id)
                            .map(LiveBinding::binding)
                            .collect()
                    } else {
                        // A named-expression subject creates its target binding instead of reading
                        // one, so snapshot the binding that was just created.
                        self.current_use_def_map_mut()
                            .current_bindings(place)
                            .map(|binding| LiveBinding::binding(&binding))
                            .collect()
                    };
                    subject_targets.push((place, bindings));
                }
                let subject_place = PlaceExpr::try_from_expr(subject)
                    .and_then(|place| self.current_place_table().place_id((&place).into()));
                let places = self.current_place_table();
                let current_scope = self.current_scope();
                let mut sequence_subject_targets =
                    SmallVec::<[(ScopedPlaceId, ScopedUseId, ExpressionNodeKey); 2]>::new();
                let mut subject_elements: Vec<Expr<'_>> = match subject.classify() {
                    ExprKind::List(list) => list.elts().iter().collect(),
                    ExprKind::Tuple(tuple) => tuple.elts().iter().collect(),
                    _ => Vec::new(),
                };
                while let Some(element) = subject_elements.pop() {
                    match element.classify() {
                        ExprKind::List(list) => subject_elements.extend(list.elts()),
                        ExprKind::Tuple(tuple) => subject_elements.extend(tuple.elts()),
                        _ => {
                            let Some(target) = PlaceExpr::try_from_expr(element)
                                .and_then(|place| places.place_id((&place).into()))
                                .zip(self.ast_ids.try_use_id(current_scope, element))
                            else {
                                continue;
                            };
                            sequence_subject_targets.push((
                                target.0,
                                target.1,
                                ExpressionNodeKey::from(element),
                            ));
                        }
                    }
                }

                let mut no_case_matched = self.flow_snapshot();

                let has_catchall = cases
                    .last()
                    .is_some_and(|case| case.guard().is_none() && case.pattern().is_wildcard());

                let mut post_case_snapshots = vec![];
                let mut previous_pattern: Option<PatternPredicate<'_>> = None;

                for (i, case) in cases.iter().enumerate() {
                    let match_pattern_predicate = self.create_pattern_predicate(
                        subject_expr,
                        case.pattern(),
                        case.guard(),
                        previous_pattern,
                    );
                    self.current_match_case = Some(CurrentMatchCase::new(
                        case.pattern(),
                        match_pattern_predicate,
                    ));
                    self.record_exception_checkpoint_if(Self::pattern_can_raise(case.pattern()));
                    self.visit_pattern(case.pattern());
                    self.current_match_case = None;
                    if let Some((ScopedPlaceId::Symbol(symbol), bindings)) = subject_targets
                        .iter_mut()
                        .find(|(place, _)| Some(*place) == subject_place)
                    {
                        self.extend_match_subject_bindings(case.pattern(), *symbol, bindings);
                    }
                    // unlike in [Stmt::If], we don't reset [no_case_matched]
                    // here because the effects of visiting a pattern is binding
                    // symbols, and this doesn't occur unless the pattern
                    // actually matches
                    let is_catchall = has_catchall && i == cases.len() - 1;
                    let (match_predicate, match_narrowing_id) = self
                        .add_pattern_narrowing_constraint(
                            match_pattern_predicate,
                            &subject_targets,
                            &sequence_subject_targets,
                            is_catchall,
                        );
                    previous_pattern = Some(match_pattern_predicate);
                    let reachability_constraint =
                        self.record_reachability_constraint_id(match_narrowing_id);

                    // For a pattern `P` and guard `G`, the case body is reached through `P && G`,
                    // while the next case is reached through `!P || (P && !G)`. Save `P && !G`
                    // separately so it can be merged with the pattern-failure state after the body.
                    let match_success_guard_failure = case.guard().map(|guard| {
                        self.visit_condition(guard);
                        let condition_flow_snapshot = self.flow_snapshot_for_condition(guard);
                        let falsy = if let Some(snapshots) = condition_flow_snapshot.into_branches()
                        {
                            self.flow_restore(snapshots.truthy);
                            snapshots.falsy
                        } else {
                            self.flow_snapshot()
                        };

                        let guard_predicate = self.build_predicate(
                            guard,
                            ExpressionContext::Condition,
                            match_pattern_predicate.guard(self.db),
                        );
                        let guard_predicate_id = self.record_narrowing_constraint(guard_predicate);
                        let guard_reachability_constraint =
                            self.record_reachability_constraint_id(guard_predicate_id);
                        let guard_success = self.flow_snapshot();

                        self.flow_restore(falsy);
                        self.record_negated_narrowing_constraint(
                            guard_predicate,
                            guard_predicate_id,
                        );
                        self.record_negated_reachability_constraint(guard_reachability_constraint);
                        let match_success_guard_failure = self.flow_snapshot();
                        self.flow_restore(guard_success);
                        match_success_guard_failure
                    });

                    self.visit_body(case.body());

                    post_case_snapshots.push(self.flow_snapshot());

                    if i != cases.len() - 1 || !has_catchall {
                        // We need to restore the state after each case, but not after the last
                        // one. The last one will just become the state that we merge the other
                        // snapshots into.
                        self.flow_restore(no_case_matched.clone());
                        self.record_negated_narrowing_constraint(
                            match_predicate,
                            match_narrowing_id,
                        );
                        self.record_negated_reachability_constraint(reachability_constraint);
                        if let Some(match_success_guard_failure) = match_success_guard_failure {
                            self.flow_merge(match_success_guard_failure);
                        } else {
                            assert!(case.guard().is_none());
                        }
                    } else {
                        debug_assert!(match_success_guard_failure.is_none());
                        debug_assert!(case.guard().is_none());
                    }

                    no_case_matched = self.flow_snapshot();
                }

                for post_clause_state in post_case_snapshots {
                    self.flow_merge(post_clause_state);
                }
            }
            StmtKind::Try(stmt_try) => {
                let body = stmt_try.body();
                let handlers = stmt_try.handlers();
                let orelse = stmt_try.orelse();
                let finalbody = stmt_try.finalbody();
                let is_star = stmt_try.is_star();

                let was_in_try_statement = std::mem::replace(&mut self.in_try_statement, true);
                self.record_ambiguous_reachability();

                let exception_handlers = if handlers.is_empty() {
                    ExceptionHandlers::None
                } else if handlers.iter().any(|handler| {
                    let ExceptHandlerKind::ExceptHandler(handler) = handler.classify();
                    handler.type_().is_none()
                }) {
                    ExceptionHandlers::catch_all()
                } else {
                    ExceptionHandlers::propagating()
                };
                self.exception_context_stack_manager
                    .push_try_context(exception_handlers, !finalbody.is_empty());

                // Visit the `try` block!
                self.visit_body(body);

                let mut post_except_states = vec![];

                // Take all checkpoints recorded immediately before operations in the `try` suite
                // that may raise. Keep the context itself on the stack so that terminal statements
                // in `except` and `else` suites can still be recorded as entries to the associated
                // `finally` suite.
                let try_block_snapshots = self.exception_context_stack_manager.end_try_suite();

                if !handlers.is_empty() {
                    // Save the state immediately *after* visiting the `try` block
                    // but *before* we prepare for visiting the `except` block(s).
                    //
                    // We will revert to this state prior to visiting the `else` block,
                    // as there necessarily must have been 0 `except` blocks executed
                    // if we hit the `else` block.
                    let post_try_block_state = self.flow_snapshot();

                    // Prepare for visiting the `except` block(s). If the `try` suite contained no
                    // exception checkpoints, its handlers are unreachable.
                    let mut try_block_snapshots = try_block_snapshots.into_iter();
                    if let Some(first_snapshot) = try_block_snapshots.next() {
                        self.flow_restore(first_snapshot);
                        for snapshot in try_block_snapshots {
                            self.flow_merge(snapshot);
                        }
                    } else {
                        self.flow_restore(post_try_block_state.clone());
                        self.mark_unreachable();
                    }

                    let pre_except_state = self.flow_snapshot();
                    let num_handlers = handlers.len();

                    for (i, except_handler) in handlers.iter().enumerate() {
                        let ExceptHandlerKind::ExceptHandler(except_handler) =
                            except_handler.classify();
                        let symbol_name = except_handler.name();
                        let handled_exceptions = except_handler.type_();
                        let handler_body = except_handler.body();

                        if let Some(handled_exceptions) = handled_exceptions {
                            self.visit_expr(handled_exceptions);
                        }

                        // If `handled_exceptions` above was `None`, it's something like `except as e:`,
                        // which is invalid syntax. However, it's still pretty obvious here that the user
                        // *wanted* `e` to be bound, so we should still create a definition here nonetheless.
                        let symbol = if let Some(symbol_name) = symbol_name {
                            let symbol = self.add_symbol(symbol_name.id().clone());

                            self.add_definition(
                                symbol.into(),
                                DefinitionNodeRef::ExceptHandler(ExceptHandlerDefinitionNodeRef {
                                    handler: except_handler,
                                    is_star,
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

                let normal_pre_finally_state = self.flow_snapshot();
                let (
                    terminal_finally_entry_snapshots,
                    has_escaping_exception,
                    has_deferred_terminal_context_manager_exit,
                ) = self
                    .exception_context_stack_manager
                    .pop_try_context()
                    .into_finally_entry_state();
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
                if normal_pre_finally_state.is_always_unreachable()
                    && !terminal_finally_entry_snapshots.is_empty()
                {
                    let mut snapshots = terminal_finally_entry_snapshots.into_iter();
                    let first_snapshot = snapshots.next().expect("checked non-empty snapshots");
                    self.flow_restore(first_snapshot);
                    for snapshot in snapshots {
                        self.flow_merge(snapshot);
                    }
                    self.visit_body(finalbody);
                    if !self.flow_snapshot().is_always_unreachable() {
                        if !finalbody.is_empty() && has_escaping_exception {
                            self.record_exception_checkpoint();
                        }
                        self.record_terminal_finally_entry();
                    }
                    self.mark_unreachable();
                } else {
                    let mut post_finally_terminal_predicate = None;
                    let mut terminal_snapshots = terminal_finally_entry_snapshots.into_iter();
                    if has_deferred_terminal_context_manager_exit
                        && let Some(snapshot) = terminal_snapshots.next()
                    {
                        let continuation = self.current_use_def_map().reachability;
                        self.current_reachability_constraints_mut()
                            .mark_used(continuation);
                        let predicate_id =
                            self.add_predicate(PredicateOrLiteral::Predicate(Predicate {
                                node: PredicateNode::FinallyNormalPathImpossible {
                                    scope: self.current_scope_id(),
                                    continuation,
                                },
                                is_positive: true,
                            }));

                        self.flow_restore(snapshot);
                        for snapshot in terminal_snapshots {
                            self.flow_merge(snapshot);
                        }

                        let reachability_constraint = self
                            .current_reachability_constraints_mut()
                            .add_atom(predicate_id);
                        let narrowing_constraint = self
                            .current_use_def_map_mut()
                            .narrowing_constraints
                            .add_atom(predicate_id);
                        self.current_use_def_map_mut()
                            .record_non_terminal_call_constraints(
                                reachability_constraint,
                                narrowing_constraint,
                            );

                        if finalbody.is_empty() {
                            let terminal_snapshot = self.flow_snapshot();
                            self.flow_restore(normal_pre_finally_state);
                            self.exception_context_stack_manager
                                .propagate_deferred_terminal_context_manager_exit(
                                    terminal_snapshot,
                                );
                        } else {
                            self.flow_merge(normal_pre_finally_state);
                            post_finally_terminal_predicate = Some(predicate_id);
                        }
                    }
                    // Mixed normal and terminal entry states are still handled by the normal path
                    // only. See the corresponding TODO tests in `terminal_statements.md`.
                    self.visit_body(finalbody);
                    if !finalbody.is_empty()
                        && has_escaping_exception
                        && self.current_use_def_map().reachability
                            != ScopedReachabilityConstraintId::ALWAYS_FALSE
                    {
                        self.record_exception_checkpoint();
                    }

                    if let Some(predicate_id) = post_finally_terminal_predicate
                        && self.current_use_def_map().reachability
                            != ScopedReachabilityConstraintId::ALWAYS_FALSE
                    {
                        let post_finally_state = self.flow_snapshot();
                        let terminal_reachability = self
                            .current_reachability_constraints_mut()
                            .add_atom(predicate_id);
                        let terminal_narrowing = self
                            .current_use_def_map_mut()
                            .narrowing_constraints
                            .add_atom(predicate_id);
                        self.current_use_def_map_mut()
                            .record_non_terminal_call_constraints(
                                terminal_reachability,
                                terminal_narrowing,
                            );
                        let terminal_snapshot = self.flow_snapshot();
                        self.flow_restore(post_finally_state);
                        self.exception_context_stack_manager
                            .propagate_deferred_terminal_context_manager_exit(terminal_snapshot);

                        let normal_reachability = self
                            .current_reachability_constraints_mut()
                            .add_not_constraint(terminal_reachability);
                        let normal_narrowing = self
                            .current_use_def_map_mut()
                            .narrowing_constraints
                            .add_negated_atom(predicate_id);
                        self.current_use_def_map_mut()
                            .record_non_terminal_call_constraints(
                                normal_reachability,
                                normal_narrowing,
                            );
                    }
                }
                self.in_try_statement = was_in_try_statement;
            }

            StmtKind::Raise(_) => {
                walk_stmt(self, stmt);
                self.record_exception_checkpoint();
                self.record_terminal_finally_entry();
                // Everything in the current block after a terminal statement is unreachable.
                self.mark_unreachable();
            }

            StmtKind::Return(_) => {
                walk_stmt(self, stmt);
                self.record_terminal_finally_entry();
                // Everything in the current block after a terminal statement is unreachable.
                self.mark_unreachable();
            }

            StmtKind::Continue(_) | StmtKind::Break(_) => {
                if self
                    .exception_context_stack_manager
                    .has_context_manager_exception_checkpoint()
                {
                    self.record_ambiguous_reachability();
                }
                let snapshot = self.flow_snapshot();
                if let Some(current_loop) = self.current_loop_mut() {
                    if stmt.is_continue_stmt() {
                        current_loop.continue_states.push(snapshot);
                    } else {
                        current_loop.break_states.push(snapshot);
                    }
                }
                self.record_terminal_finally_entry();
                // Everything in the current block after a terminal statement is unreachable.
                self.mark_unreachable();
            }
            StmtKind::Global(stmt_global) => {
                let range = stmt_global.range();
                let names = stmt_global.names();

                for name in names {
                    self.scopes_by_expression
                        .record_expression(&name, self.current_scope());
                    let symbol_id = self.add_symbol(name.id().clone());
                    let symbol = self.current_place_table().symbol(symbol_id);
                    // Check whether the variable has already been accessed in this scope.
                    if (symbol.is_bound() || symbol.is_declared() || symbol.is_used())
                        && !symbol.is_parameter()
                    {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::LoadBeforeGlobalDeclaration {
                                name: name.to_string(),
                                start: name.range().start(),
                            },
                            range: name.range(),
                            python_version: self.python_version(),
                        });
                    }
                    // Check whether the variable has also been declared nonlocal.
                    if symbol.is_nonlocal() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::NonlocalAndGlobal(name.to_string()),
                            range: name.range(),
                            python_version: self.python_version(),
                        });
                        // Never mark a symbol both global and nonlocal, even in this error case.
                        continue;
                    }
                    // Check whether this is the module scope, where `global` has no effect.
                    let scope_id = self.current_scope();
                    if scope_id.is_global() {
                        // It's important that we don't `mark_global` here, because we error on
                        // type annotations on places that are marked global, but it's actually
                        // legal to write `global x; x: int = 42` at the module level.
                        continue;
                    }
                    // Assuming none of the rules above are violated, repeated `global`
                    // declarations are allowed and ignored.
                    if symbol.is_global() {
                        continue;
                    }
                    self.current_place_table_mut()
                        .symbol_mut(symbol_id)
                        .mark_global();
                    self.current_scope_info_mut()
                        .this_scope_global_or_nonlocal_declarations
                        .insert(name.id().clone(), range);
                }
                walk_stmt(self, stmt);
            }
            StmtKind::Nonlocal(stmt_nonlocal) => {
                let range = stmt_nonlocal.range();
                let names = stmt_nonlocal.names();

                for name in names {
                    self.scopes_by_expression
                        .record_expression(&name, self.current_scope());
                    let symbol_id = self.add_symbol(name.id().clone());
                    let symbol = self.current_place_table().symbol(symbol_id);
                    // Check whether the variable has already been accessed in this scope.
                    if (symbol.is_bound() || symbol.is_declared() || symbol.is_used())
                        && !symbol.is_parameter()
                    {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::LoadBeforeNonlocalDeclaration {
                                name: name.to_string(),
                                start: name.range().start(),
                            },
                            range: name.range(),
                            python_version: self.python_version(),
                        });
                    }
                    // Check whether the variable has also been declared global.
                    if symbol.is_global() {
                        self.report_semantic_error(SemanticSyntaxError {
                            kind: SemanticSyntaxErrorKind::NonlocalAndGlobal(name.to_string()),
                            range: name.range(),
                            python_version: self.python_version(),
                        });
                        // Never mark a symbol both global and nonlocal, even in this error case.
                        continue;
                    }
                    // Check whether this is the module scope, where `nonlocal` isn't allowed.
                    let scope_id = self.current_scope();
                    if scope_id.is_global() {
                        // The SemanticSyntaxChecker will report an error for this.
                        continue;
                    }
                    // Assuming none of the rules above are violated, repeated `nonlocal`
                    // declarations are allowed and ignored.
                    if symbol.is_nonlocal() {
                        continue;
                    }
                    self.current_place_table_mut()
                        .symbol_mut(symbol_id)
                        .mark_nonlocal();
                    self.current_scope_info_mut()
                        .this_scope_global_or_nonlocal_declarations
                        .insert(name.id().clone(), range);
                }
                walk_stmt(self, stmt);
            }
            StmtKind::Delete(stmt_delete) => {
                let targets = stmt_delete.targets();

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
                        self.invalidate_narrowing_aliases_for(place_id);
                        self.delete_binding(place_id);
                    }
                }
            }
            StmtKind::Expr(stmt_expr) => {
                let value = stmt_expr.value();

                if self.in_module_scope() {
                    if let Some(expr) = dunder_all_extend_argument(value) {
                        self.add_standalone_expression(expr);
                    }
                }

                self.visit_expr(value);

                // If the statement is a call (or an `await` wrapping a call), it could
                // possibly be a call to a function marked with `NoReturn` (for example,
                // `sys.exit()` or `await async_exit()`). In this case, we use a special
                // kind of constraint to mark the following code as unreachable.
                //
                // Ideally, these constraints should be added for every call expression, even those in
                // sub-expressions. But doing so makes the number of such constraints so high that
                // it significantly degrades performance. We thus cut scope here and add these
                // constraints only at statement-level function calls, like `sys.exit()`, and not
                // within sub-expressions like `3 + sys.exit()` etc.
                if let Some(StatementCall { call, is_await }) =
                    StatementCall::from_expression(value)
                {
                    let func = call.func();
                    // Avoid creating reachability nodes for calls on unannotated collection
                    // literals. Without this short-circuit, performing reachability analysis
                    // can lead to quadratic blowup of cycle dependencies during full-scope
                    // collection inference, as Salsa flattens the dependencies of all cycle
                    // participants, and the reachability analysis of a given use of the
                    // collection may create dependencies on all previous uses, leading to
                    // significant performance regressions.
                    //
                    // Note that built-in collection types do not have methods that explicitly
                    // return `Never`, so this does not have a meaningful semantic impact, except in
                    // the rare case where a collection is explicitly marked as having elements
                    // of type `Never`.
                    if !self.source_type.is_stub()
                        && func
                            .as_attribute_expr()
                            .and_then(|attribute| {
                                self.unannotated_collection_literal_binding(attribute.value())
                            })
                            .is_none()
                    {
                        let callable =
                            self.add_standalone_expression_impl(func, ExpressionKind::Callee, None);
                        let call_expr = self.add_standalone_expression(value);

                        let predicate = Predicate {
                            node: PredicateNode::IsNonTerminalCall(CallableAndCallExpr::new(
                                self.db, callable, call_expr, is_await,
                            )),
                            is_positive: true,
                        };

                        let predicate_id =
                            self.add_predicate(PredicateOrLiteral::Predicate(predicate));
                        let narrowing_constraint = self
                            .current_use_def_map_mut()
                            .narrowing_constraints
                            .add_atom(predicate_id);

                        let reachability_constraint = self
                            .current_reachability_constraints_mut()
                            .add_atom(predicate_id);
                        self.current_use_def_map_mut()
                            .record_non_terminal_call_constraints(
                                reachability_constraint,
                                narrowing_constraint,
                            );
                    }
                }
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }
}

impl<'ast> Visitor<'ast> for SemanticIndexBuilder<'_, 'ast> {
    fn visit_annotation(&mut self, expression: Expr<'ast>) {
        self.annotations.push(expression.node_index());
        self.visit_expr(expression);
    }

    fn visit_stmt(&mut self, stmt: Stmt<'ast>) {
        self.push_statement(CurrentStatement::default());
        self.visit_stmt_impl(stmt);
        let mut current_statement = self.pop_statement();

        // We currently only consider certain types of statements to introduce constraints
        // on collection initializers. This restriction is mostly for performance reasons, as we
        // want to avoid "reads" of a collection contributing to the complexity of the cycles
        // created by full-scope collection inference.
        current_statement
            .collection_uses
            .retain(|(_, use_expression)| {
                match stmt.classify() {
                    // A return involving the collection object.
                    StmtKind::Return(_) => true,

                    // A subscript assignment on the collection object.
                    StmtKind::Assign(stmt_assign) => {
                        let targets = stmt_assign.targets();

                        targets.as_array().is_some_and(|[target]| {
                            target.as_subscript_expr().is_some_and(|subscript| {
                                ExpressionNodeKey::from(subscript.value()) == *use_expression
                            })
                        })
                    }

                    // An annotated assignment assigning the collection object to a new binding.
                    StmtKind::AnnAssign(_) => true,

                    // A bound-method call on the collection object.
                    StmtKind::Expr(stmt_expr) => {
                        let value = stmt_expr.value();

                        match value.classify() {
                            ExprKind::Call(expr_call) => {
                                let func = expr_call.func();
                                match func.classify() {
                                    ExprKind::Attribute(expr_attribute) => {
                                        let value = expr_attribute.value();
                                        ExpressionNodeKey::from(value) == *use_expression
                                    }
                                    _ => false,
                                }
                            }
                            _ => false,
                        }
                    }

                    _ => false,
                }
            });

        if current_statement.lambda_expressions.is_empty()
            && current_statement.collection_uses.is_empty()
        {
            return;
        }

        let standalone_statement = self.add_standalone_statement(stmt);

        // The body of a lambda expression needs access to the `Callable` type
        // context the lambda is being inferred with, and so any statement
        // containing a lambda must be inferable as a standalone statement
        // to avoid large scope-level cycles.
        self.enclosing_lambda_statements.extend(
            current_statement
                .lambda_expressions
                .into_iter()
                .map(|lambda| (lambda.into(), standalone_statement)),
        );

        // The inferred element type of a collection initializer depends on uses of
        // the collection in its containing scope, and so each use must be part
        // of an standalone inferable statement to avoid large scope-level cycles.
        let mut collection_defs = FxHashSet::default();
        for (collection_def, use_expression) in current_statement.collection_uses {
            // If the same collection is referenced multiple times in this statement,
            // we only consider the first occurrence, as collection use constraints are
            // tracked at the statement level.
            if !collection_defs.insert(collection_def) {
                continue;
            }

            self.uses_by_collection
                .entry(collection_def)
                .or_default()
                .push((standalone_statement, use_expression));

            self.collections_by_use
                .insert(use_expression, collection_def);
        }
    }

    fn visit_keyword(&mut self, keyword: Keyword<'ast>) {
        walk_keyword(self, keyword);

        if keyword.arg().is_some() {
            return;
        }

        // Record a use of all members of `x` for a splatted keyword argument `**x`.
        let current_scope = self.current_scope();
        let member_places = PlaceExpr::try_from_expr(keyword.value())
            .and_then(|value_place_expr| {
                self.current_place_table()
                    .place_id((&value_place_expr).into())
            })
            .map(|value_place_id| {
                let place_table = &self.place_tables[current_scope];
                place_table
                    .associated_place_ids(value_place_id)
                    .iter()
                    .filter(move |key_member_id| {
                        let key_member_expr = place_table.member(**key_member_id).expression();

                        // Only include top-level keys.
                        let Some(key_parent) = key_member_expr.as_ref().parent() else {
                            return true;
                        };
                        match place_table.place(value_place_id) {
                            PlaceExprRef::Symbol(_) => false,
                            PlaceExprRef::Member(value_member) => {
                                key_parent == value_member.expression()
                            }
                        }
                    })
                    .map(|key_member_id| ScopedPlaceId::from(*key_member_id))
            });

        let use_id = self.ast_ids.record_use(current_scope, keyword);
        self.use_def_maps[current_scope]
            .record_multi_use(member_places.into_iter().flatten(), use_id);
    }

    fn visit_expr(&mut self, expr: Expr<'ast>) {
        // Generic AST walking evaluates child expressions as values. Short-circuit syntax
        // propagates condition context explicitly through `visit_expr_with_context`.
        self.visit_expr_with_context(expr, ExpressionContext::Value);
    }

    fn visit_parameters(&mut self, parameters: Parameters<'ast>) {
        // Intentionally avoid walking default expressions, as we handle them in the enclosing
        // scope.
        for parameter in parameters.iter().map(AnyParameter::as_parameter) {
            self.visit_parameter(parameter);
        }
    }

    fn visit_pattern(&mut self, pattern: Pattern<'ast>) {
        if let ast::PatternFields::MatchOr(ast::PatternMatchOrFields { patterns, .. }) =
            pattern.fields()
            && let Some((last, alternatives)) = patterns.split_last()
            && (
                // Capture-free alternatives do not affect bindings and need no flow merge.
                patterns.iter().any(Self::pattern_has_bindings)
            )
        {
            // Start each alternative without earlier captures so repeated names do not shadow one
            // another. Complementary predicates preserve possible missing captures while all
            // alternatives together recover the incoming reachability.
            let mut successful_alternatives = None;
            for alternative in alternatives {
                let remaining_alternatives = self.flow_snapshot();
                let selected_alternative =
                    self.record_reachability_constraint(PredicateOrLiteral::Predicate(Predicate {
                        node: PredicateNode::OrPatternAlternative(self.current_scope_id()),
                        is_positive: true,
                    }));
                self.visit_pattern(alternative);
                if let Some(previous_alternatives) = successful_alternatives.take() {
                    self.flow_merge(previous_alternatives);
                }
                successful_alternatives = Some(self.flow_snapshot());
                self.flow_restore(remaining_alternatives);
                self.record_negated_reachability_constraint(selected_alternative);
            }

            self.visit_pattern(last);
            if let Some(successful_alternative) = successful_alternatives {
                self.flow_merge(successful_alternative);
            }
            return;
        }

        walk_pattern(self, pattern);

        if let Some(name) = pattern.capture_name() {
            let symbol = self.add_symbol(name.id().clone());
            let state = self.current_match_case.as_ref().unwrap();
            self.add_definition(
                symbol.into(),
                MatchPatternDefinitionNodeRef {
                    pattern: state.pattern,
                    identifier: name,
                    predicate: state.predicate,
                },
            );
        }
    }
}

impl SemanticSyntaxContext for SemanticIndexBuilder<'_, '_> {
    fn future_annotations_or_stub(&self) -> bool {
        self.has_future_annotations
    }

    fn lazy_import_context(&self) -> Option<LazyImportContext> {
        match self.scopes[self.current_scope()].kind() {
            // Possible, but invalid positions.
            ScopeKind::Function => return Some(LazyImportContext::Function),
            ScopeKind::Class => return Some(LazyImportContext::Class),
            // Valid position.
            ScopeKind::Module => {}
            // Impossible positions because lambdas and comprehensions can't contain statements.
            ScopeKind::Comprehension
            | ScopeKind::Lambda
            | ScopeKind::TypeAlias
            | ScopeKind::TypeParams => {}
        }

        if self.in_try_statement {
            return Some(LazyImportContext::TryExceptBlocks);
        }

        None
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
                    return scope.node().expect_function().node(self.module).is_async();
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
                NodeWithScopeKind::ListComprehension(node) => node.node(self.module).generators(),
                NodeWithScopeKind::SetComprehension(node) => node.node(self.module).generators(),
                NodeWithScopeKind::DictComprehension(node) => node.node(self.module).generators(),
                _ => continue,
            };
            if generators
                .iter()
                .all(|comprehension| !comprehension.is_async())
            {
                return true;
            }
        }
        false
    }

    fn in_class_body_comprehension(&self) -> bool {
        for scope_info in self.scope_stack.iter().rev() {
            match self.scopes[scope_info.file_scope_id].kind() {
                ScopeKind::Comprehension => {}
                ScopeKind::Class => return true,
                ScopeKind::Module
                | ScopeKind::TypeParams
                | ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::TypeAlias => return false,
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
        // TODO(brent) The long-term fix for this is for `YieldOutsideFunction` not to apply to
        // `await` at all and only to emit the more specific `AwaitOutsideAsyncFunction` instead.
        // However, to preserve backwards compatibility with the corresponding Ruff rules, we
        // temporarily filter out the diagnostic for ty instead.
        if matches!(
            error.kind,
            SemanticSyntaxErrorKind::YieldOutsideFunction(YieldOutsideFunctionKind::Await)
        ) {
            return;
        }

        if self.db.should_check_file(self.file.file(self.db)) {
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
            .is_some_and(|func| func.node(self.module).parameters().includes(name))
    }
}

/// A simple-name annotated assignment with an RHS whose declaration is already recorded.
/// Created only by `begin_annotated_assignment`; finishing it records the value binding.
#[derive(Copy, Clone, Debug, PartialEq)]
struct PendingAnnotatedAssignment<'db> {
    definition: Definition<'db>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum CurrentAssignment<'ast, 'db> {
    Assign {
        node: StmtAssign<'ast>,
        unpack: Option<Unpack<'db>>,
        owner: BindingsOwner,
    },
    AnnAssign {
        node: StmtAnnAssign<'ast>,
        pending: Option<PendingAnnotatedAssignment<'db>>,
    },
    AugAssign(StmtAugAssign<'ast>),
    For {
        node: StmtFor<'ast>,
        unpack: Option<(UnpackPosition, Unpack<'db>)>,
    },
    Named(ExprNamed<'ast>),
    Comprehension {
        node: Comprehension<'ast>,
        first: bool,
        unpack: Option<(UnpackPosition, Unpack<'db>)>,
    },
    WithItem {
        item: WithItem<'ast>,
        is_async: bool,
        unpack: Option<(UnpackPosition, Unpack<'db>)>,
    },
}

impl CurrentAssignment<'_, '_> {
    fn unpack_position_mut(&mut self) -> Option<&mut UnpackPosition> {
        match self {
            Self::For { unpack, .. }
            | Self::WithItem { unpack, .. }
            | Self::Comprehension { unpack, .. } => unpack.as_mut().map(|(position, _)| position),
            Self::Assign { .. } | Self::AnnAssign { .. } | Self::AugAssign(_) | Self::Named(_) => {
                None
            }
        }
    }
}

#[derive(Default)]
struct CurrentStatement<'ast, 'db> {
    /// A list of lambda expressions contained in this statement.
    lambda_expressions: Vec<ExprLambda<'ast>>,
    /// A list of collection definitions whose uses are contained in this statement.
    collection_uses: Vec<(Definition<'db>, ExpressionNodeKey)>,
}

#[derive(Debug, PartialEq)]
struct CurrentMatchCase<'ast, 'db> {
    /// The pattern that's part of the current match case.
    pattern: Pattern<'ast>,

    /// The predicate for the complete match case.
    predicate: PatternPredicate<'db>,
}

impl<'ast, 'db> CurrentMatchCase<'ast, 'db> {
    fn new(pattern: Pattern<'ast>, predicate: PatternPredicate<'db>) -> Self {
        Self { pattern, predicate }
    }
}

enum Unpackable<'ast> {
    Assign(StmtAssign<'ast>),
    For(StmtFor<'ast>),
    WithItem {
        item: WithItem<'ast>,
        is_async: bool,
    },
    Comprehension {
        first: bool,
        node: Comprehension<'ast>,
    },
}

impl<'ast> Unpackable<'ast> {
    fn kind(&self) -> UnpackKind {
        match self {
            Unpackable::Assign(_) => UnpackKind::Assign,
            Unpackable::For(node) => UnpackKind::Iterable {
                mode: EvaluationMode::from_is_async(node.is_async()),
            },
            Unpackable::Comprehension { node, .. } => UnpackKind::Iterable {
                mode: EvaluationMode::from_is_async(node.is_async()),
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
        let positioned = unpack.map(|unpack| (UnpackPosition::First, unpack));
        match *self {
            Unpackable::Assign(stmt) => CurrentAssignment::Assign {
                node: stmt,
                unpack,
                owner: BindingsOwner::Statement,
            },
            Unpackable::For(stmt) => CurrentAssignment::For {
                node: stmt,
                unpack: positioned,
            },
            Unpackable::WithItem { item, is_async } => CurrentAssignment::WithItem {
                item,
                is_async,
                unpack: positioned,
            },
            Unpackable::Comprehension { node, first } => CurrentAssignment::Comprehension {
                node,
                first,
                unpack: positioned,
            },
        }
    }
}

/// Returns the single argument to `__all__.extend()`, if it is a call to `__all__.extend()`
/// where it looks like the argument might be a `submodule.__all__` expression.
/// Else, returns `None`.
fn dunder_all_extend_argument(value: Expr<'_>) -> Option<Expr<'_>> {
    let expr_call_node = value.as_call_expr()?;
    let func = expr_call_node.func();
    let arguments = expr_call_node.arguments();

    let expr_attribute_node = func.as_attribute_expr()?;
    let value = expr_attribute_node.value();
    let attr = expr_attribute_node.attr();

    let expr_name_node = value.as_name_expr()?;
    let id = expr_name_node.id();

    if id != "__all__" {
        return None;
    }

    if attr != "extend" {
        return None;
    }

    if !arguments.keywords().is_empty() {
        return None;
    }

    let [single_argument] = arguments.args().as_array()?;

    let expr_attribute_node = single_argument.as_attribute_expr()?;
    let value = expr_attribute_node.value();
    let attr = expr_attribute_node.attr();

    (attr == "__all__").then_some(value)
}

/// Returns `true` for syntactically direct `range(...)` calls.
///
/// This avoids adding reachability predicates for every `for` loop target to the TDD graph. We only
/// emit the predicate for syntactically direct `range(...)` calls; type checking later verifies that
/// the callee resolves to the built-in `range` and determines whether the range is statically
/// non-empty.
fn is_direct_range_call(expr: Expr<'_>) -> bool {
    expr.expression_value()
        .as_call_expr()
        .and_then(|call| call.func().as_name_expr())
        .is_some_and(|name| name.id() == "range")
}

/// Groups consecutive arena row keys belonging to the same scope. Semantic visitation
/// can visit siblings in a different order from the parser that allocated their rows.
struct ExpressionsScopeMapBuilder {
    module: Vec<Option<FileScopeId>>,
    annotations: Vec<(NodeIndex, FileScopeId)>,
}

impl ExpressionsScopeMapBuilder {
    fn new() -> Self {
        Self {
            module: Vec::new(),
            annotations: Vec::new(),
        }
    }

    fn record_expression(&mut self, expression: &impl HasTrackedScope, scope: FileScopeId) {
        let index = expression.node_index();
        if let Some(row) = index.as_u32().filter(|&index| index <= MAX_REAL_INDEX) {
            let row = row as usize;
            if row >= self.module.len() {
                self.module.resize(row + 1, None);
            }
            self.module[row] = Some(scope);
        } else {
            self.annotations.push((index, scope));
        }
    }

    fn build(mut self, scopes: &mut IndexVec<FileScopeId, Scope>) -> ExpressionsScopeMap {
        // Module entries are already in row order. Annotation keys occupy separate ranges.
        self.annotations.sort_unstable_by_key(|(index, _)| *index);
        let mut iter = (0_u32..)
            .zip(self.module)
            .filter_map(|(row, scope)| scope.map(|scope| (NodeIndex::from(row), scope)))
            .chain(self.annotations);
        let Some(first) = iter.next() else {
            return ExpressionsScopeMap::default();
        };

        let mut interval_map = Vec::new();
        let mut push_range = |range: std::ops::RangeInclusive<NodeIndex>, scope: FileScopeId| {
            scopes[scope].record_first_expression_row(*range.start());
            interval_map.push((range, scope));
        };

        let mut current_scope = first.1;
        let mut range = first.0..=first.0;

        for (index, scope) in iter {
            if scope == current_scope {
                range = *range.start()..=index;
                continue;
            }

            push_range(range, current_scope);

            current_scope = scope;
            range = index..=index;
        }

        push_range(range, current_scope);

        ExpressionsScopeMap(interval_map.into_boxed_slice())
    }
}

/// Returns the static truthiness of a literal iterable.
///
/// Returns [`Truthiness::Unknown`] for other expressions and when starred elements or dictionary
/// unpacking make the literal's emptiness ambiguous.
fn literal_iterable_truthiness(expr: Expr<'_>) -> Truthiness {
    match expr.classify() {
        ExprKind::Tuple(_)
        | ExprKind::List(_)
        | ExprKind::Set(_)
        | ExprKind::Dict(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BytesLiteral(_) => Truthiness::from_expr(expr, |_| false),
        _ => Truthiness::Unknown,
    }
}

/// Returns if the expression is a `TYPE_CHECKING` expression.
fn is_if_type_checking(expr: Expr<'_>) -> bool {
    match expr.classify() {
        ExprKind::Name(expr_name) => {
            let id = expr_name.id();
            id == "TYPE_CHECKING"
        }
        ExprKind::Attribute(expr_attribute) => {
            let value = expr_attribute.value();
            let attr = expr_attribute.attr();

            attr == "TYPE_CHECKING" && is_dotted_name(value)
        }
        _ => false,
    }
}

/// Returns if the expression is a `not TYPE_CHECKING` expression.
fn is_if_not_type_checking(expr: Expr<'_>) -> bool {
    matches!(
        expr.fields(),
        ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields {
            op: UnaryOp::Not,
            operand,
            ..
        }) if is_if_type_checking(operand)
    )
}

fn is_empty_collection_constructor_call(expr: Expr<'_>) -> bool {
    let ExprKind::Call(call_node) = expr.classify() else {
        return false;
    };
    let func = call_node.func();
    let arguments = call_node.arguments();

    arguments.is_empty()
        && func
            .as_name_expr()
            .is_some_and(|name| matches!(name.id().as_str(), "list" | "set" | "dict"))
}

fn is_collection_initializer(expr: Expr<'_>) -> bool {
    is_collection_literal(expr) || is_empty_collection_constructor_call(expr)
}

fn is_collection_literal(expr: Expr<'_>) -> bool {
    expr.is_list_expr() || expr.is_set_expr() || expr.is_dict_expr()
}
