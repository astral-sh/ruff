//! Defines a [`PlaceLoad`], which combines the semantics of name resolution
//! with the results of reaching definition analysis to describe the sources
//! that may provide the value read from a place.
//!
//! More specifically, a [`PlaceLoad`] encapsulates:
//!
//! 1. [`PlaceLoad::local_source`], the (optional) value source from the load's
//!    own scope.
//! 2. The lexical ([`PlaceLoad::lexical_fallbacks`]) and post-lexical fallbacks
//!    ([`PlaceLoad::post_lexical_fallbacks`]) considered if the local source
//!    does not supply a value for the load.
//! 3. Any condition that controls whether those fallbacks are applicable (via
//!    [`PlaceLoad::with_conditional_fallbacks`] and
//!    [`PlaceLoad::place_expr_prefix_loads`]).
//! 4. [`PlaceLoad::failure_on_exhaustion`] the resolution failure that occurs
//!    if none of those sources ultimately supply a value.
//! 5. [`PlaceLoad::constraint_keys`], the type-narrowing constraints associated
//!    with each source.
//! 6. [`PlaceLoad::scope_declarations`], the explicit scope declarations
//!    (`global` or `nonlocal`) crossed while resolving the place.
//!
//! We consume this model to different ends:
//!
//! - Type inference uses it to determine the type and definedness of a place
//! - The language server uses it to determine, e.g., what references to an
//!   imported module should be rewritten in response to the module itself being
//!   renamed
//!
//! Here is an example describing how a [`PlaceLoad`] is constructed:
//!
//! ```py
//! from collections.abc import Callable
//!
//! def make_counter(start: int, enabled: bool) -> Callable[[], int | None]:
//!     if enabled:
//!         value = start
//!     else:
//!         value = None
//!
//!     def next_value() -> int | None:
//!         nonlocal value
//!         if value is not None:
//!             current = value  # load U
//!             value += 1
//!             return current
//!         return None
//!
//!     return next_value
//! ```
//!
//! As mentioned previously, constructing the [`PlaceLoad`] at `U` combines
//! reaching definition analysis with a lexical scope walk:
//!
//! 1. Reaching definition analysis from the use-def module supplies the binding
//!    state for `value` at `U` in `next_value`. (In this case, no value-binding
//!    definition reaches U because the `value += 1` assignment occurs after `U`,
//!    so the state records `value` as unbound.)
//! 2. The use-def model also supplies the narrowing constraint `value is not None`
//!    that is associated with the source. That constraint can affect an
//!    inferred type but does not affect name resolution.
//! 3. Name resolution encounters the `nonlocal` declaration and continues
//!    the lexical scope walk into `make_counter`, where `value` is owned.
//! 4. Once name resolution reaches the scope that owns value, it records
//!    `make_counter.value` as an enclosing source of a potential value for `U`.
//!
//! Schematically, the resulting [`PlaceLoad`] looks like this:
//!
//! ```text
//! PlaceLoad {
//!     local_source: Bindings(next_value.value at U),  // unbound
//!     lexical_fallbacks: [
//!         DefinitionsFromOwningScope(make_counter.value),
//!     ],
//!     failure_on_exhaustion: UnboundFree,
//!     constraint_keys: [
//!         value is not None,
//!     ],
//!     scope_declarations: [
//!         Nonlocal(next_value),
//!     ],
//! }
//! ```
//!
//! The enclosing function's binding scope terminates name resolution, even if
//! none of its definitions supply a value at runtime. [`PlaceLoad`] therefore
//! records `UnboundFree` as the failure if both sources are exhausted instead
//! of recording module globals or builtins as later sources. In this example,
//! type inference establishes that the branches in `make_counter` always
//! define `value`, so the failure is unreachable.

use ruff_python_ast::{self as ast, name::Name};
use smallvec::SmallVec;
use ty_python_core::ast_ids::{HasScopedUseId, ScopedUseId};
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::{PlaceExprRef, ScopedPlaceId};
use ty_python_core::scope::{FileScopeId, NodeWithScopeKind, ScopeId, ScopeKind};
use ty_python_core::symbol::Symbol;
use ty_python_core::{
    BindingWithConstraintsIterator, BoundnessAnalysis, EnclosingSnapshotResult, ProgramFile,
    SemanticIndex, Truthiness, global_scope, place_table, use_def_map,
};

use crate::Db;
use crate::reachability::ReachabilityConstraintsExtension;

/// Resolves a place load into its local source and ordered fallbacks.
///
/// This describes name resolution without inferring the type supplied by any source. Eager nested
/// scopes prefer a snapshot taken where the nested scope was created. Otherwise, the search walks
/// outward and records the scope that owns the place for consumers to evaluate.
pub(crate) fn resolve_place_load<'db>(
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    scope: ScopeId<'db>,
    place: PlaceExprRef,
    mode: PlaceLoadMode<'_>,
) -> PlaceLoad<'db> {
    PlaceLoadResolutionContext {
        db,
        index,
        scope,
        file: scope.program_file(db),
        mode,
    }
    .resolve(place)
}

/// A semantic description of reading a place without inferring its type.
pub struct PlaceLoad<'db> {
    /// The source from the load's own scope.
    local_source: Option<PlaceLoadSource<'db>>,
    /// The sources considered after the local source but before implicit globals and builtins.
    lexical_fallbacks: SmallVec<[PlaceLoadSource<'db>; 1]>,
    /// The implicit global and builtin fallbacks considered after lexical resolution.
    post_lexical_fallbacks: Option<PostLexicalFallbacks<'db>>,
    /// The name resolution failure associated with exhausting all sources.
    failure_on_exhaustion: PlaceLoadFailure,
    /// The narrowing constraints collected while resolving the load.
    constraint_keys: Vec<(FileScopeId, ConstraintKey)>,
    /// The explicit `global` and `nonlocal` declarations crossed during name resolution.
    scope_declarations: SmallVec<[ScopedDeclaration; 1]>,
    /// Place-expression prefix loads that must all be undefined before fallbacks are applicable.
    place_expr_prefix_loads: Option<PlaceExprPrefixLoads<'db>>,
}

impl<'db> PlaceLoad<'db> {
    /// Creates a load with no sources.
    fn new() -> Self {
        Self {
            local_source: None,
            lexical_fallbacks: SmallVec::new(),
            post_lexical_fallbacks: None,
            failure_on_exhaustion: PlaceLoadFailure::NotFound,
            constraint_keys: Vec::new(),
            scope_declarations: SmallVec::new(),
            place_expr_prefix_loads: None,
        }
    }

    /// Creates a load whose first source is from its own scope.
    ///
    /// `exit_constraint`, when present, is activated only after inference
    /// visits the local source (that source was already selected from the
    /// binding state identified by the constraint, it is deliberately not
    /// reapplied to the local source).
    fn from_local_source(
        local: PlaceLoadSourceKind<'db>,
        exit_constraint: Option<(FileScopeId, ConstraintKey)>,
    ) -> Self {
        let mut load = Self::new();
        load.local_source =
            Some(load.make_source(local, PlaceLoadSourceRole::Ordinary, exit_constraint));
        load
    }

    /// Records the name resolution failure associated with exhausting all sources.
    fn with_failure_on_exhaustion(mut self, failure_on_exhaustion: PlaceLoadFailure) -> Self {
        self.failure_on_exhaustion = failure_on_exhaustion;
        self
    }

    /// Returns the name resolution failure associated with exhausting all sources.
    pub fn failure_on_exhaustion(&self) -> PlaceLoadFailure {
        self.failure_on_exhaustion
    }

    /// Returns the explicit `global` and `nonlocal` declarations crossed during resolution.
    pub fn scope_declarations(&self) -> &[ScopedDeclaration] {
        &self.scope_declarations
    }

    /// Makes fallbacks conditional on every tracked place-expression prefix being undefined in
    /// the load's scope.
    fn with_conditional_fallbacks(
        mut self,
        place_expr_prefix_loads: PlaceExprPrefixLoads<'db>,
    ) -> Self {
        self.place_expr_prefix_loads = Some(place_expr_prefix_loads);
        self
    }

    /// Returns the constraints used to narrow `source`
    /// (and records which constraints are active after it).
    pub(crate) fn narrowing_constraints_for(
        &self,
        source: &PlaceLoadSource<'db>,
        checkpoint: &mut PlaceLoadConstraintCheckpoint,
    ) -> &[(FileScopeId, ConstraintKey)] {
        checkpoint.0 = checkpoint.0.max(source.exit_checkpoint.0);
        &self.constraint_keys[..source.entry_checkpoint.0]
    }

    /// Returns the constraints used after all lexical sources are exhausted
    /// (and records that inference reached that point).
    pub(crate) fn narrowing_constraints_on_exhaustion(
        &self,
        checkpoint: &mut PlaceLoadConstraintCheckpoint,
    ) -> &[(FileScopeId, ConstraintKey)] {
        checkpoint.0 = self.constraint_keys.len();
        &self.constraint_keys
    }

    /// Consumes the load and returns the constraints active at `checkpoint`.
    pub(crate) fn into_constraints(
        mut self,
        checkpoint: &PlaceLoadConstraintCheckpoint,
    ) -> Vec<(FileScopeId, ConstraintKey)> {
        self.constraint_keys.truncate(checkpoint.0);
        self.constraint_keys
    }

    /// Returns the source from the load's own scope as a zero-or-one slice.
    pub fn local_sources(&self) -> &[PlaceLoadSource<'db>] {
        self.local_source.as_slice()
    }

    /// Returns the lexical and post-lexical fallbacks together with their applicability condition.
    pub fn fallbacks(&self) -> PlaceLoadFallbacks<'_, 'db> {
        self.place_expr_prefix_loads.as_ref().map_or(
            PlaceLoadFallbacks::Unconditional {
                lexical: &self.lexical_fallbacks,
                post_lexical: self.post_lexical_fallbacks.as_ref(),
            },
            |prefix_loads| PlaceLoadFallbacks::IfNoPlaceExprPrefixIsBound {
                prefix_loads,
                lexical: &self.lexical_fallbacks,
                post_lexical: self.post_lexical_fallbacks.as_ref(),
            },
        )
    }

    /// Appends a lexical fallback source.
    fn push_source(&mut self, kind: PlaceLoadSourceKind<'db>, role: PlaceLoadSourceRole) {
        let source = self.make_source(kind, role, None);
        self.lexical_fallbacks.push(source);
    }

    /// Appends a lexical fallback source and a constraint that becomes active
    /// after that source is visited.
    fn push_source_with_exit_constraint(
        &mut self,
        kind: PlaceLoadSourceKind<'db>,
        role: PlaceLoadSourceRole,
        constraint: (FileScopeId, ConstraintKey),
    ) {
        let source = self.make_source(kind, role, Some(constraint));
        self.lexical_fallbacks.push(source);
    }

    /// Appends a lexical fallback source without applying any narrowing constraints to it.
    fn push_unnarrowed_source(
        &mut self,
        kind: PlaceLoadSourceKind<'db>,
        role: PlaceLoadSourceRole,
    ) {
        let exit_checkpoint = self.current_constraint_checkpoint();
        self.lexical_fallbacks.push(PlaceLoadSource {
            kind,
            entry_checkpoint: PlaceLoadConstraintCheckpoint::default(),
            exit_checkpoint,
            role,
        });
    }

    /// Extends the list of constraints used by subsequent sources.
    fn push_constraint(&mut self, scope: FileScopeId, key: ConstraintKey) {
        self.constraint_keys.push((scope, key));
    }

    /// Finishes a load that can fall through to implicit globals and builtins.
    fn finish_at_global_scope(mut self, file: ProgramFile<'db>, name: Option<&Name>) -> Self {
        if let Some(name) = name {
            self.post_lexical_fallbacks = Some(PostLexicalFallbacks {
                file,
                name: name.clone(),
            });
        }

        self.with_failure_on_exhaustion(PlaceLoadFailure::NotFound)
    }

    /// Finishes a load at an enclosing binding scope.
    fn finish_at_enclosing_scope(
        self,
        enclosing_scope_kind: ScopeKind,
        file: ProgramFile<'db>,
        name: Option<&Name>,
    ) -> Self {
        if enclosing_scope_kind.is_class() {
            self.finish_at_global_scope(file, name)
        } else {
            self.with_failure_on_exhaustion(PlaceLoadFailure::UnboundFree)
        }
    }

    fn make_source(
        &mut self,
        kind: PlaceLoadSourceKind<'db>,
        role: PlaceLoadSourceRole,
        exit_constraint: Option<(FileScopeId, ConstraintKey)>,
    ) -> PlaceLoadSource<'db> {
        let entry_checkpoint = self.current_constraint_checkpoint();
        self.constraint_keys.extend(exit_constraint);
        PlaceLoadSource {
            kind,
            entry_checkpoint,
            exit_checkpoint: self.current_constraint_checkpoint(),
            role,
        }
    }

    fn current_constraint_checkpoint(&self) -> PlaceLoadConstraintCheckpoint {
        PlaceLoadConstraintCheckpoint(self.constraint_keys.len())
    }
}

/// Tracks how far inference has progressed through a [`PlaceLoad`]'s constraints.
///
/// Only [`PlaceLoad`] can advance or interpret this checkpoint.
#[derive(Clone, Default)]
pub(crate) struct PlaceLoadConstraintCheckpoint(usize);

/// The implicit module-global and builtin fallbacks considered after lexical resolution.
///
/// The implicit global is narrowed by all constraints collected during resolution.
/// The builtin fallback deliberately remains unnarrowed.
pub struct PostLexicalFallbacks<'db> {
    file: ProgramFile<'db>,
    name: Name,
}

impl<'db> PostLexicalFallbacks<'db> {
    /// Returns the file containing the load.
    pub fn file(&self) -> ProgramFile<'db> {
        self.file
    }

    /// Returns the loaded name.
    pub fn name(&self) -> &Name {
        &self.name
    }
}

/// The sources considered after a [`PlaceLoad`]'s local source is exhausted.
#[derive(Clone, Copy)]
pub enum PlaceLoadFallbacks<'a, 'db> {
    /// The fallback sources are always applicable.
    Unconditional {
        /// Sources considered during lexical resolution after the local source.
        lexical: &'a [PlaceLoadSource<'db>],
        /// The implicit module-global and builtin fallbacks.
        post_lexical: Option<&'a PostLexicalFallbacks<'db>>,
    },
    /// The fallback sources apply only if every tracked prefix is locally undefined.
    ///
    /// For example, the enclosing binding of `obj.value` cannot supply the value read in `inner`:
    ///
    /// ```python
    /// class Outer:
    ///     value: int
    ///
    /// class Inner:
    ///     value: str
    ///
    /// def outer():
    ///     obj = Outer()
    ///     obj.value = 1
    ///
    ///     def inner():
    ///         obj = Inner()
    ///         reveal_type(obj.value)  # revealed: str
    /// ```
    ///
    /// The nested scope binds `obj` to a different object, so normal member lookup on the local
    /// `obj` must handle the load instead.
    IfNoPlaceExprPrefixIsBound {
        /// The place-expression prefix loads that control whether fallback is applicable.
        prefix_loads: &'a PlaceExprPrefixLoads<'db>,
        /// Sources considered during lexical resolution after the local source.
        lexical: &'a [PlaceLoadSource<'db>],
        /// The implicit module-global and builtin fallbacks.
        post_lexical: Option<&'a PostLexicalFallbacks<'db>>,
    },
}

/// Compact descriptions of loads for the tracked prefixes of a place expression.
pub struct PlaceExprPrefixLoads<'db> {
    scope: ScopeId<'db>,
    loads: SmallVec<[PlaceExprPrefixLoad; 2]>,
}

impl<'db> PlaceExprPrefixLoads<'db> {
    /// Creates prefix loads, returning `None` when the iterator is empty.
    fn from_iter(
        scope: ScopeId<'db>,
        loads: impl IntoIterator<Item = PlaceExprPrefixLoad>,
    ) -> Option<Self> {
        let loads = loads.into_iter().collect::<SmallVec<_>>();
        (!loads.is_empty()).then_some(Self { scope, loads })
    }

    /// Returns the scope containing the prefix loads.
    pub fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Iterates over the prefix loads.
    pub fn iter(&self) -> impl Iterator<Item = PlaceExprPrefixLoad> + '_ {
        self.loads.iter().copied()
    }
}

/// Describes how a consumer can evaluate one prefix of a place expression.
#[derive(Clone, Copy)]
pub enum PlaceExprPrefixLoad {
    /// Use the bindings that reach this expression occurrence.
    AtUse(ScopedUseId),
    /// Use every binding reachable for this place at the end of its scope.
    AllReachable(ScopedPlaceId),
    /// The syntax itself guarantees that the prefix is bound.
    DefinitelyBound,
}

/// The evaluation context used to resolve a place load.
#[derive(Clone, Copy)]
pub(crate) enum PlaceLoadMode<'ast> {
    /// Resolve bindings live at an expression occurrence.
    AtExpression(ast::ExprRef<'ast>),
    /// Resolve all bindings reachable at the end of the scope.
    Deferred,
    /// Resolve reachable bindings in a parsed string annotation.
    StringAnnotation,
}

impl PlaceLoadMode<'_> {
    fn is_deferred(self) -> bool {
        matches!(self, Self::Deferred | Self::StringAnnotation)
    }
}

/// One source that can supply the value of a place load.
///
/// [`PlaceLoad`] stores one shared list of constraint keys. Each source maintains two checkpoints
/// into that list:
///
/// - `entry_checkpoint` identifies the constraints used to narrow the source.
/// - `exit_checkpoint` identifies the constraints active after inference visits the source.
///
/// Those two checkpoints differ when a key identifies the binding state used to construct a source.
/// That key becomes active after inference visits the source, but applying it to the same source
/// again would duplicate work.
///
/// For `U` in the preceding example, the constraint representation is schematically:
///
/// ```text
/// PlaceLoad {
///     constraint_keys: [
///         (next_value, UseId(U)),
///     ],
///     sources: [
///         PlaceLoadSource {
///             kind: Bindings(next_value at U),
///             entry_checkpoint: 0,
///             exit_checkpoint: 1,
///         },
///         PlaceLoadSource {
///             kind: DefinitionsFromOwningScope(make_counter.value),
///             entry_checkpoint: 1,
///             exit_checkpoint: 1,
///         },
///     ],
///     failure_on_exhaustion: UnboundFree,
///     scope_declarations: [Nonlocal(next_value)],
/// }
/// ```
///
/// The first source already comes from `bindings_at_use(U)`, so its `UseId` key becomes active on
/// exit but is not applied on entry. If that source is undefined, the `UseId` key narrows the
/// enclosing `int | None` place to `int`. If both sources are exhausted, the key remains active
/// for expression-level narrowing.
#[derive(Clone)]
pub struct PlaceLoadSource<'db> {
    /// How this source supplies the loaded value.
    pub(crate) kind: PlaceLoadSourceKind<'db>,
    /// Selects the constraints used to narrow this source.
    entry_checkpoint: PlaceLoadConstraintCheckpoint,
    /// Selects the constraints active after inference visits this source.
    exit_checkpoint: PlaceLoadConstraintCheckpoint,
    /// The role this source plays in the load.
    role: PlaceLoadSourceRole,
}

impl<'db> PlaceLoadSource<'db> {
    /// Returns the specialized source, when this load is not binding-backed.
    pub fn implicit(&self) -> Option<&ImplicitPlaceLoad<'db>> {
        match &self.kind {
            PlaceLoadSourceKind::Implicit(implicit) => Some(implicit),
            PlaceLoadSourceKind::Bindings(_)
            | PlaceLoadSourceKind::DefinitionsFromOwningScope { .. } => None,
        }
    }

    /// Returns the reachable binding states represented by this source, when it is binding-backed.
    pub fn reachable_bindings(&self, db: &'db dyn Db) -> Option<ReachableBindings<'db>> {
        let bindings = match &self.kind {
            PlaceLoadSourceKind::Bindings(bindings) => bindings.clone(),
            PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                use_def_map(db, *scope).reachable_bindings(*id)
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                file,
                name,
            }) => {
                let scope = global_scope(db, *file);
                let symbol = place_table(db, scope).symbol_id(name)?;
                use_def_map(db, scope).reachable_symbol_bindings(symbol)
            }
            PlaceLoadSourceKind::Implicit(
                ImplicitPlaceLoad::DunderClass(_) | ImplicitPlaceLoad::ClassBodySymbol(_),
            ) => return None,
        };

        Some(reachable_bindings(db, bindings))
    }

    /// Returns whether this source is the module fallback for a class-local name.
    pub fn is_class_body_global_fallback(&self) -> bool {
        self.role == PlaceLoadSourceRole::ClassBodyGlobalFallback
    }
}

/// Evaluates the statically reachable states in a binding iterator.
pub fn reachable_bindings<'db>(
    db: &'db dyn Db,
    bindings: BindingWithConstraintsIterator<'db, 'db>,
) -> ReachableBindings<'db> {
    ReachableBindings { db, bindings }
}

/// An iterator over the statically reachable binding states for a load source.
pub struct ReachableBindings<'db> {
    db: &'db dyn Db,
    bindings: BindingWithConstraintsIterator<'db, 'db>,
}

impl ReachableBindings<'_> {
    /// Returns how unbound states affect the source's boundness.
    pub fn boundness_analysis(&self) -> BoundnessAnalysis {
        self.bindings.boundness_analysis()
    }
}

impl<'db> Iterator for ReachableBindings<'db> {
    type Item = ReachableBinding<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let binding = self.bindings.next()?;
            let reachability = self.bindings.reachability_constraints().evaluate(
                self.db,
                self.bindings.predicates(),
                binding.reachability_constraint,
            );
            if !reachability.is_always_false() {
                return Some(ReachableBinding {
                    state: binding.binding,
                    reachability,
                });
            }
        }
    }
}

/// A binding state together with its statically known reachability.
#[derive(Clone, Copy)]
pub struct ReachableBinding<'db> {
    state: DefinitionState<'db>,
    reachability: Truthiness,
}

impl<'db> ReachableBinding<'db> {
    /// Returns the binding state.
    pub fn state(self) -> DefinitionState<'db> {
        self.state
    }

    /// Returns whether the binding is reachable.
    pub fn reachability(self) -> Truthiness {
        self.reachability
    }
}

/// Describes how a source can supply a place's value.
#[derive(Clone)]
pub(crate) enum PlaceLoadSourceKind<'db> {
    /// Bindings selected at a use, from an enclosing scope snapshot, or from
    /// all bindings reachable in a scope.
    Bindings(BindingWithConstraintsIterator<'db, 'db>),
    /// Definitions for the loaded place from the scope that binds or declares it.
    DefinitionsFromOwningScope {
        /// The scope containing the place.
        scope: ScopeId<'db>,
        /// The place within `scope`.
        id: ScopedPlaceId,
    },
    /// A source represented by a specialized query or rule.
    Implicit(ImplicitPlaceLoad<'db>),
}

/// A source that consumers evaluate using a specialized query or rule.
#[derive(Clone)]
pub enum ImplicitPlaceLoad<'db> {
    /// The implicit `__class__` cell for a method, lambda, or generator expression defined directly
    /// in a class body, e.g.:
    ///
    /// ```py
    /// class C:
    ///     def method(self):
    ///         return __class__
    /// ```
    DunderClass(Definition<'db>),
    /// An implicit symbol supplied directly in a class body, e.g.:
    ///
    /// ```py
    /// class C:
    ///     defining_module = __module__
    /// ```
    ClassBodySymbol(Name),
    /// A symbol in the module's explicit global namespace, e.g.:
    ///
    /// ```py
    /// answer = 42
    ///
    /// def get_answer():
    ///     return answer
    /// ```
    ExplicitGlobalSymbol { file: ProgramFile<'db>, name: Name },
}

/// The role a source plays in a place load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceLoadSourceRole {
    /// The source follows ordinary Python name resolution rules.
    Ordinary,
    /// The source follows Python’s class-local-to-module fallback rules.
    ClassBodyGlobalFallback,
}

/// The name resolution failure associated with exhausting a place load's sources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaceLoadFailure {
    /// No namespace supplies the name, which raises `NameError`.
    NotFound,
    /// The current function-like binding scope owns the name but supplies no value, which raises
    /// `UnboundLocalError`.
    UnboundLocal,
    /// An enclosing function-like binding scope owns the name but its closure cell is empty, which
    /// raises `NameError`.
    UnboundFree,
}

/// An explicit declaration crossed while resolving a load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopedDeclaration {
    /// A `global` declaration in this scope.
    Global(FileScopeId),
    /// A `nonlocal` declaration in this scope.
    Nonlocal(FileScopeId),
}

#[derive(Clone, Copy)]
struct PlaceLoadResolutionContext<'db, 'ast> {
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    scope: ScopeId<'db>,
    file: ProgramFile<'db>,
    mode: PlaceLoadMode<'ast>,
}

impl<'db> PlaceLoadResolutionContext<'db, '_> {
    fn resolve(self, place: PlaceExprRef) -> PlaceLoad<'db> {
        let current_scope = self.scope.file_scope_id(self.db);
        let place_table = self.index.place_table(current_scope);
        let mut load = match self.local_source(place) {
            Some((local_source, exit_constraint)) => {
                PlaceLoad::from_local_source(local_source, exit_constraint)
            }
            None => PlaceLoad::new(),
        };

        if let Some(prefix_loads) = self.place_expr_prefix_loads(place) {
            load = load.with_conditional_fallbacks(prefix_loads);
        }

        let mut symbol_is_local = false;
        if let Some(symbol) = place.as_symbol()
            && let Some(symbol_id) = place_table.symbol_id(symbol.name())
        {
            // Footgun: `place` and `symbol` were probably constructed with all-zero
            // flags. We need to read the place table to get correct flags.
            let indexed_symbol = place_table.symbol(symbol_id);
            symbol_is_local = indexed_symbol.is_local();
            if indexed_symbol.is_global() {
                load.scope_declarations
                    .push(ScopedDeclaration::Global(current_scope));
            }
            if indexed_symbol.is_nonlocal() {
                load.scope_declarations
                    .push(ScopedDeclaration::Nonlocal(current_scope));
            }

            // If we try to access a variable in a class before it has been defined, the
            // name resolution will fall back to global. See the comment on `Symbol::is_local`.
            let class_body_global_fallback =
                self.scope.node(self.db).scope_kind().is_class() && symbol_is_local;
            if self.skips_non_global_scopes(symbol_id) || class_body_global_fallback {
                let role = if class_body_global_fallback {
                    PlaceLoadSourceRole::ClassBodyGlobalFallback
                } else {
                    PlaceLoadSourceRole::Ordinary
                };
                return self.resolve_global(load, place, Some(symbol.name()), role);
            }
        }

        // Symbols that are bound or declared in the local scope, and not marked `nonlocal` or
        // `global`, never refer to an enclosing scope. (If you reference such a symbol before
        // it's bound, you get an `UnboundLocalError`.) Short-circuit instead of walking
        // enclosing scopes in this case. The one exception to this rule is the global fallback
        // in class bodies, which we already handled above.
        if symbol_is_local {
            if self.scope.node(self.db).scope_kind().is_module() {
                return load.finish_at_global_scope(self.file, place.as_symbol().map(Symbol::name));
            }
            return load.with_failure_on_exhaustion(PlaceLoadFailure::UnboundLocal);
        }

        if let PlaceExprRef::Symbol(symbol) = place
            && symbol.name() == "__class__"
            && let Some(definition) = self.dunder_class_cell_definition()
        {
            load.push_unnarrowed_source(
                PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::DunderClass(definition)),
                PlaceLoadSourceRole::Ordinary,
            );
        }

        // Walk enclosing scopes to resolve a free-variable load (`LOAD_DEREF` at runtime).
        // There are two main ways we try to model these loads:
        //
        // 1. "Snapshots" record the bindings/constraints in the enclosing scope at the point
        //    just before a nested scope begins. For variables that aren't modified after that
        //    point, that's the only value that the nested scope can see. If a variable is
        //    reassigned later, lazy snapshots for that variable can be updated or swept.
        //
        // 2. Otherwise, we keep walking until we get to the variable's original defining
        //    scope and record that scope as a source. Each consumer decides which of the
        //    place's definitions to evaluate there.
        //
        // This walk only resolves free variables and explicit `nonlocal`s. A symbol that is
        // local to the current scope never falls back to an enclosing scope, even if it's only
        // possibly bound at the current use: Python would raise `UnboundLocalError` instead.
        for (enclosing_scope, _) in self.index.ancestor_scopes(current_scope).skip(1) {
            match self.search_scope(&mut load, place, enclosing_scope) {
                ScopeContinuation::Stop(enclosing_scope_kind) => {
                    return load.finish_at_enclosing_scope(
                        enclosing_scope_kind,
                        self.file,
                        place.as_symbol().map(Symbol::name),
                    );
                }
                ScopeContinuation::Enclosing => {}
                ScopeContinuation::Module => break,
            }
        }

        // If we're in a class body, check for implicit class body symbols first.
        // These take precedence over globals.
        if self.scope.node(self.db).scope_kind().is_class()
            && let Some(symbol) = place.as_symbol()
        {
            load.push_source(
                PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ClassBodySymbol(
                    symbol.name().clone(),
                )),
                PlaceLoadSourceRole::Ordinary,
            );
        }

        // Continue resolution in the module's explicit global scope.
        self.resolve_global(
            load,
            place,
            place.as_symbol().map(Symbol::name),
            PlaceLoadSourceRole::Ordinary,
        )
    }

    fn local_source(
        self,
        place: PlaceExprRef,
    ) -> Option<(
        PlaceLoadSourceKind<'db>,
        Option<(FileScopeId, ConstraintKey)>,
    )> {
        let scope = self.scope.file_scope_id(self.db);
        let table = self.index.place_table(scope);
        let use_def = self.index.use_def_map(scope);

        match self.mode {
            PlaceLoadMode::AtExpression(expr_ref) => {
                if expr_ref
                    .as_name_expr()
                    .is_some_and(|name| name.is_invalid())
                    || matches!(expr_ref, ast::ExprRef::Named(_))
                {
                    return None;
                }

                let use_id = expr_ref.scoped_use_id(self.db, self.file);
                Some((
                    PlaceLoadSourceKind::Bindings(use_def.bindings_at_use(use_id)),
                    Some((scope, ConstraintKey::UseId(use_id))),
                ))
            }
            PlaceLoadMode::Deferred | PlaceLoadMode::StringAnnotation => {
                let source = table
                    .place_id(place)
                    .map(|id| PlaceLoadSourceKind::Bindings(use_def.reachable_bindings(id)));
                assert!(
                    source.is_some() || matches!(self.mode, PlaceLoadMode::StringAnnotation),
                    "Expected the place table to create a place for every valid PlaceExpr node"
                );
                source.map(|source| (source, None))
            }
        }
    }

    /// Describes how to evaluate the tracked place-expression prefixes of `place` in this scope.
    fn place_expr_prefix_loads(self, place: PlaceExprRef) -> Option<PlaceExprPrefixLoads<'db>> {
        let table = self.index.place_table(self.scope.file_scope_id(self.db));

        PlaceExprPrefixLoads::from_iter(
            self.scope,
            table
                .parents(place)
                .filter_map(|prefix_id| match self.mode {
                    PlaceLoadMode::Deferred | PlaceLoadMode::StringAnnotation => {
                        Some(PlaceExprPrefixLoad::AllReachable(prefix_id))
                    }
                    PlaceLoadMode::AtExpression(mut prefix_expr_ref) => {
                        let prefix = table.place(prefix_id);
                        for _ in 0..(place.num_member_segments() - prefix.num_member_segments()) {
                            prefix_expr_ref = match prefix_expr_ref {
                                ast::ExprRef::Attribute(attribute) => {
                                    ast::ExprRef::from(&attribute.value)
                                }
                                ast::ExprRef::Subscript(subscript) => {
                                    ast::ExprRef::from(&subscript.value)
                                }
                                _ => return None,
                            };
                        }

                        if prefix_expr_ref
                            .as_name_expr()
                            .is_some_and(|name| name.is_invalid())
                        {
                            return None;
                        }

                        if let ast::ExprRef::Named(named) = prefix_expr_ref {
                            return named
                                .target
                                .is_name_expr()
                                .then_some(PlaceExprPrefixLoad::DefinitelyBound);
                        }

                        Some(PlaceExprPrefixLoad::AtUse(
                            prefix_expr_ref.scoped_use_id(self.db, self.file),
                        ))
                    }
                }),
        )
    }

    fn search_scope(
        self,
        load: &mut PlaceLoad<'db>,
        place: PlaceExprRef,
        enclosing_scope: FileScopeId,
    ) -> ScopeContinuation {
        // If the current enclosing scope is global, no place resolution is performed here,
        // instead falling back to the module's explicit global resolution below.
        if enclosing_scope.is_global() {
            return ScopeContinuation::Module;
        }

        let enclosing = self.index.scope(enclosing_scope);
        let mut eagerly_undefined = false;
        if !self.mode.is_deferred() {
            // If the reference is in a nested eager scope, we need to look for the place at
            // the point where the previous enclosing scope was defined, instead of at the end
            // of the scope. (Note that the semantic index builder takes care of only
            // registering eager bindings for nested scopes that are actually eager, and for
            // enclosing scopes that actually contain bindings that we should use when
            // resolving the reference.)
            match self.index.enclosing_snapshot(
                enclosing_scope,
                place,
                self.scope.file_scope_id(self.db),
            ) {
                EnclosingSnapshotResult::FoundConstraint(constraint) => {
                    load.push_constraint(
                        enclosing_scope,
                        ConstraintKey::NarrowingConstraint(constraint),
                    );
                    // If the current scope is eager, it is certain that the place is undefined in
                    // the current scope. Do not add the place below as a fallback.
                    if self.scope.scope(self.db).is_eager() {
                        eagerly_undefined = true;
                    }
                }
                EnclosingSnapshotResult::FoundBindings(bindings) => {
                    load.push_source_with_exit_constraint(
                        PlaceLoadSourceKind::Bindings(bindings),
                        PlaceLoadSourceRole::Ordinary,
                        (
                            enclosing_scope,
                            ConstraintKey::NestedScope(self.scope.file_scope_id(self.db)),
                        ),
                    );
                    return ScopeContinuation::Stop(enclosing.kind());
                }
                EnclosingSnapshotResult::NotFound => {
                    // There are no visible bindings or constraints here. Don't fall back to
                    // non-eager place resolution if the root place has been reassigned.
                    if self.root_place_was_reassigned(place, enclosing_scope) {
                        return ScopeContinuation::Stop(enclosing.kind());
                    }
                    return ScopeContinuation::Enclosing;
                }
                EnclosingSnapshotResult::NoLongerInEagerContext => {
                    if self.root_place_was_reassigned(place, enclosing_scope) {
                        return ScopeContinuation::Stop(enclosing.kind());
                    }
                }
            }
        }

        // Class scopes are not visible to nested scopes, and we need to handle global
        // scope differently (because an unbound name there falls back to builtins), so
        // check only function-like scopes.
        // There is one exception to this rule: annotation scopes can see
        // names defined in an immediately-enclosing class scope.
        let is_immediately_enclosing_class = self.scope.is_annotation(self.db)
            && self
                .scope
                .scope(self.db)
                .parent()
                .is_some_and(|parent| parent == enclosing_scope);
        if !enclosing.kind().is_function_like() && !is_immediately_enclosing_class {
            return ScopeContinuation::Enclosing;
        }

        let table = self.index.place_table(enclosing_scope);
        let Some(id) = table.place_id(place) else {
            return ScopeContinuation::Enclosing;
        };
        let enclosing_place = table.place(id);
        // Reads of "free" or `nonlocal` variables terminate at any enclosing scope that
        // marks the variable `global`, whether or not that scope actually binds the
        // variable. If we see a `global` declaration, stop walking scopes and proceed to
        // the global handling below. (If we're walking from a prior/inner scope where this
        // variable is `nonlocal`, then this is a semantic syntax error, but we don't
        // enforce that here. See `SemanticIndexBuilder::pop_scope`.)
        if enclosing_place.as_symbol().is_some_and(Symbol::is_global) {
            load.scope_declarations
                .push(ScopedDeclaration::Global(enclosing_scope));
            return ScopeContinuation::Module;
        }
        // Keep walking until we reach the defining scope of the variable. The synthetic
        // nested bindings definitions installed there will see everything below it.
        if enclosing_place.as_symbol().is_some_and(Symbol::is_nonlocal) {
            load.scope_declarations
                .push(ScopedDeclaration::Nonlocal(enclosing_scope));
            return ScopeContinuation::Enclosing;
        }
        if !(enclosing_place.is_bound() || enclosing_place.is_declared()) {
            // Note that this check includes members like `x.y` and `x[0]`, which aren't
            // symbols and can't be explicitly `nonlocal`.
            return ScopeContinuation::Enclosing;
        }

        // We've reached the scope that owns the place. Record it so each consumer can evaluate
        // the considered definitions it needs.
        if !eagerly_undefined {
            load.push_source(
                PlaceLoadSourceKind::DefinitionsFromOwningScope {
                    scope: enclosing_scope.to_scope_id(self.db, self.file),
                    id,
                },
                PlaceLoadSourceRole::Ordinary,
            );
        }
        ScopeContinuation::Stop(enclosing.kind())
    }

    /// Resolve a load that has fallen through to the module's explicit global scope.
    ///
    /// For eager nested scopes, this uses the global enclosing snapshot instead of the completed
    /// module scope, so a class body cannot see a class name that is bound only after the body
    /// finishes:
    ///
    /// ```python
    /// class A:
    ///     A = A
    /// ```
    ///
    /// `symbol_name` is only needed when no snapshot is available: snapshots can resolve complex
    /// places like `a.x`, but the fallback global query only works for bare symbols. `role`
    /// preserves the class-body fallback behavior for names that are also local to the class body.
    fn resolve_global(
        self,
        mut load: PlaceLoad<'db>,
        place: PlaceExprRef,
        symbol_name: Option<&Name>,
        role: PlaceLoadSourceRole,
    ) -> PlaceLoad<'db> {
        let current_scope = self.scope.file_scope_id(self.db);
        let post_lexical_name = place.as_symbol().map(Symbol::name);
        if current_scope.is_global() {
            return load.finish_at_global_scope(self.file, post_lexical_name);
        }

        if !self.mode.is_deferred() {
            match self
                .index
                .enclosing_snapshot(FileScopeId::global(), place, current_scope)
            {
                EnclosingSnapshotResult::FoundConstraint(constraint) => {
                    load.push_constraint(
                        FileScopeId::global(),
                        ConstraintKey::NarrowingConstraint(constraint),
                    );
                    // Reaching here means that no bindings are found in any scope.
                    // Since `explicit_global_symbol` may return a cycle initial value,
                    // don't add it as a fallback.
                    return load.finish_at_global_scope(self.file, post_lexical_name);
                }
                EnclosingSnapshotResult::FoundBindings(bindings) => {
                    load.push_source_with_exit_constraint(
                        PlaceLoadSourceKind::Bindings(bindings),
                        role,
                        (
                            FileScopeId::global(),
                            ConstraintKey::NestedScope(current_scope),
                        ),
                    );
                    return load.finish_at_global_scope(self.file, post_lexical_name);
                }
                // There are no visible bindings / constraint here.
                EnclosingSnapshotResult::NotFound => {
                    return load.finish_at_global_scope(self.file, post_lexical_name);
                }
                EnclosingSnapshotResult::NoLongerInEagerContext => {}
            }
        }

        if let Some(name) = symbol_name {
            load.push_source(
                PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                    file: self.file,
                    name: name.clone(),
                }),
                role,
            );
        }
        load.finish_at_global_scope(self.file, post_lexical_name)
    }

    fn root_place_was_reassigned(self, place: PlaceExprRef, scope: FileScopeId) -> bool {
        let table = self.index.place_table(scope);
        table
            .parents(place)
            .any(|root| table.place(root).is_bound())
    }

    fn skips_non_global_scopes(self, symbol: ty_python_core::symbol::ScopedSymbolId) -> bool {
        let scope = self.scope.file_scope_id(self.db);
        !scope.is_global() && self.index.symbol_is_global_in_scope(symbol, scope)
    }

    fn dunder_class_cell_definition(self) -> Option<Definition<'db>> {
        let current_scope = self.scope.file_scope_id(self.db);
        if let Some(definition) = self.index.class_definition_of_method(current_scope) {
            return Some(definition);
        }

        let scope = self.index.scope(current_scope);
        if !matches!(
            scope.node(),
            NodeWithScopeKind::Lambda(_) | NodeWithScopeKind::GeneratorExpression(_)
        ) {
            return None;
        }
        let class = self.index.parent_scope(current_scope)?.node().as_class()?;
        Some(self.index.expect_single_definition(class))
    }
}

enum ScopeContinuation {
    Stop(ty_python_core::scope::ScopeKind),
    Enclosing,
    Module,
}
