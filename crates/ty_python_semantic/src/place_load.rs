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

use ruff_python_ast::name::Name;
use smallvec::SmallVec;
use ty_python_core::ast_ids::ScopedUseId;
use ty_python_core::definition::Definition;
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::{ScopeId, ScopeKind};
use ty_python_core::{BindingWithConstraintsIterator, FileScopeId, ProgramFile};

/// A semantic description of reading a place without inferring its type.
pub(crate) struct PlaceLoad<'db> {
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
    pub(crate) scope_declarations: SmallVec<[ScopedDeclaration; 1]>,
    /// Place-expression prefix loads that must all be undefined before fallbacks are applicable.
    place_expr_prefix_loads: Option<PlaceExprPrefixLoads<'db>>,
}

impl<'db> PlaceLoad<'db> {
    /// Creates a new load with no sources.
    pub(crate) fn new() -> Self {
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
    pub(crate) fn from_local_source(
        local: PlaceLoadSourceKind<'db>,
        exit_constraint: Option<(FileScopeId, ConstraintKey)>,
    ) -> Self {
        let mut load = Self::new();
        load.local_source =
            Some(load.make_source(local, PlaceLoadSourceRole::Ordinary, exit_constraint));
        load
    }

    /// Records the name resolution failure associated with exhausting all sources.
    pub(crate) fn with_failure_on_exhaustion(
        mut self,
        failure_on_exhaustion: PlaceLoadFailure,
    ) -> Self {
        self.failure_on_exhaustion = failure_on_exhaustion;
        self
    }

    /// Returns the name resolution failure associated with exhausting all sources.
    pub(crate) fn failure_on_exhaustion(&self) -> PlaceLoadFailure {
        self.failure_on_exhaustion
    }

    /// Makes fallbacks conditional on every tracked place-expression prefix being undefined in
    /// the load's scope.
    pub(crate) fn with_conditional_fallbacks(
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
    pub(crate) fn local_sources(&self) -> &[PlaceLoadSource<'db>] {
        self.local_source.as_slice()
    }

    /// Returns the lexical and post-lexical fallbacks together with their applicability condition.
    pub(crate) fn fallbacks(&self) -> PlaceLoadFallbacks<'_, 'db> {
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
    pub(crate) fn push_source(
        &mut self,
        kind: PlaceLoadSourceKind<'db>,
        role: PlaceLoadSourceRole,
    ) {
        let source = self.make_source(kind, role, None);
        self.lexical_fallbacks.push(source);
    }

    /// Appends a lexical fallback source and a constraint that becomes active
    /// after that source is visited.
    pub(crate) fn push_source_with_exit_constraint(
        &mut self,
        kind: PlaceLoadSourceKind<'db>,
        role: PlaceLoadSourceRole,
        constraint: (FileScopeId, ConstraintKey),
    ) {
        let source = self.make_source(kind, role, Some(constraint));
        self.lexical_fallbacks.push(source);
    }

    /// Appends a lexical fallback source without applying any narrowing constraints to it.
    pub(crate) fn push_unnarrowed_source(
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
    pub(crate) fn push_constraint(&mut self, scope: FileScopeId, key: ConstraintKey) {
        self.constraint_keys.push((scope, key));
    }

    /// Finishes a load that can fall through to implicit globals and builtins.
    pub(crate) fn finish_at_global_scope(
        mut self,
        file: ProgramFile<'db>,
        name: Option<&Name>,
    ) -> Self {
        if let Some(name) = name {
            self.post_lexical_fallbacks = Some(PostLexicalFallbacks {
                file,
                name: name.clone(),
            });
        }

        self.with_failure_on_exhaustion(PlaceLoadFailure::NotFound)
    }

    /// Finishes a load at an enclosing binding scope.
    pub(crate) fn finish_at_enclosing_scope(
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
pub(crate) struct PostLexicalFallbacks<'db> {
    file: ProgramFile<'db>,
    name: Name,
}

impl<'db> PostLexicalFallbacks<'db> {
    /// Returns the file containing the load.
    pub(crate) fn file(&self) -> ProgramFile<'db> {
        self.file
    }

    /// Returns the loaded name.
    pub(crate) fn name(&self) -> &Name {
        &self.name
    }
}

/// The sources considered after a [`PlaceLoad`]'s local source is exhausted.
#[derive(Clone, Copy)]
pub(crate) enum PlaceLoadFallbacks<'a, 'db> {
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
pub(crate) struct PlaceExprPrefixLoads<'db> {
    scope: ScopeId<'db>,
    loads: SmallVec<[PlaceExprPrefixLoad; 2]>,
}

impl<'db> PlaceExprPrefixLoads<'db> {
    /// Creates prefix loads, returning `None` when the iterator is empty.
    pub(crate) fn from_iter(
        scope: ScopeId<'db>,
        loads: impl IntoIterator<Item = PlaceExprPrefixLoad>,
    ) -> Option<Self> {
        let loads = loads.into_iter().collect::<SmallVec<_>>();
        (!loads.is_empty()).then_some(Self { scope, loads })
    }

    /// Returns the scope containing the prefix loads.
    pub(crate) fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Iterates over the prefix loads.
    pub(crate) fn iter(&self) -> impl Iterator<Item = PlaceExprPrefixLoad> + '_ {
        self.loads.iter().copied()
    }
}

/// Describes how a consumer can evaluate one prefix of a place expression.
#[derive(Clone, Copy)]
pub(crate) enum PlaceExprPrefixLoad {
    /// Use the bindings that reach this expression occurrence.
    AtUse(ScopedUseId),
    /// Use every binding reachable for this place at the end of its scope.
    AllReachable(ScopedPlaceId),
    /// The syntax itself guarantees that the prefix is bound.
    DefinitelyBound,
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
pub(crate) struct PlaceLoadSource<'db> {
    /// How this source supplies the loaded value.
    pub(crate) kind: PlaceLoadSourceKind<'db>,
    /// Selects the constraints used to narrow this source.
    entry_checkpoint: PlaceLoadConstraintCheckpoint,
    /// Selects the constraints active after inference visits this source.
    exit_checkpoint: PlaceLoadConstraintCheckpoint,
    /// The role this source plays in the load.
    role: PlaceLoadSourceRole,
}

impl PlaceLoadSource<'_> {
    /// Returns whether this source is the module fallback for a class-local name.
    pub(crate) fn is_class_body_global_fallback(&self) -> bool {
        self.role == PlaceLoadSourceRole::ClassBodyGlobalFallback
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
pub(crate) enum ImplicitPlaceLoad<'db> {
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
pub(crate) enum PlaceLoadFailure {
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
pub(crate) enum ScopedDeclaration {
    /// A `global` declaration in this scope.
    Global(FileScopeId),
    /// A `nonlocal` declaration in this scope.
    Nonlocal(FileScopeId),
}
