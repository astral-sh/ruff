//! First, some terminology:
//!
//! * A "place" is semantically a location where a value can be read or written, and syntactically,
//!   an expression that can be the target of an assignment, e.g. `x`, `x[0]`, `x.y`. (The term is
//!   borrowed from Rust). In Python syntax, an expression like `f().x` is also allowed as the
//!   target so it can be called a place, but we do not record declarations / bindings like `f().x:
//!   int`, `f().x = ...`. Type checking itself can be done by recording only assignments to names,
//!   but in order to perform type narrowing by attribute/subscript assignments, they must also be
//!   recorded.
//!
//! * A "binding" gives a new value to a place. This includes many different Python statements
//!   (assignment statements of course, but also imports, `def` and `class` statements, `as`
//!   clauses in `with` and `except` statements, match patterns, and others) and even one
//!   expression kind (named expressions). It notably does not include annotated assignment
//!   statements without a right-hand side value; these do not assign any new value to the place.
//!   We consider function parameters to be bindings as well, since (from the perspective of the
//!   function's internal scope), a function parameter begins the scope bound to a value.
//!
//! * A "declaration" establishes an upper bound type for the values that a variable may be
//!   permitted to take on. Annotated assignment statements (with or without an RHS value) are
//!   declarations; annotated function parameters are also declarations. We consider `def` and
//!   `class` statements to also be declarations, so as to prohibit accidentally shadowing them.
//!
//! Annotated assignments with a right-hand side, and annotated function parameters, are both
//! bindings and declarations.
//!
//! We use [`Definition`] as the universal term (and Salsa tracked struct) encompassing both
//! bindings and declarations. (This sacrifices a bit of type safety in exchange for improved
//! performance via fewer Salsa tracked structs and queries, since most declarations -- typed
//! parameters and annotated assignments with RHS -- are both bindings and declarations.)
//!
//! At any given use of a variable, we can ask about both its "declared type" and its "inferred
//! type". These may be different, but the inferred type must always be assignable to the declared
//! type; that is, the declared type is always wider, and the inferred type may be more precise. If
//! we see an invalid assignment, we emit a diagnostic and abandon our inferred type, deferring to
//! the declared type (this allows an explicit annotation to override bad inference, without a
//! cast), maintaining the invariant.
//!
//! The **inferred type** represents the most precise type we believe encompasses all possible
//! values for the variable at a given use. It is based on a union of the bindings which can reach
//! that use through some control flow path, and the narrowing constraints that control flow must
//! have passed through between the binding and the use. For example, in this code:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     use(x)
//! ```
//!
//! For the use of `x` on the third line, the inferred type should be `Literal[1]`. This is based
//! on the binding on the first line, which assigns the type `Literal[1] | None`, and the narrowing
//! constraint on the second line, which rules out the type `None`, since control flow must pass
//! through this constraint to reach the use in question.
//!
//! The **declared type** represents the code author's declaration (usually through a type
//! annotation) that a given variable should not be assigned any type outside the declared type. In
//! our model, declared types are also control-flow-sensitive; we allow the code author to
//! explicitly redeclare the same variable with a different type. So for a given binding of a
//! variable, we will want to ask which declarations of that variable can reach that binding, in
//! order to determine whether the binding is permitted, or should be a type error. For example:
//!
//! ```python
//! from pathlib import Path
//! def f(path: str):
//!     path: Path = Path(path)
//! ```
//!
//! In this function, the initial declared type of `path` is `str`, meaning that the assignment
//! `path = Path(path)` would be a type error, since it assigns to `path` a value whose type is not
//! assignable to `str`. This is the purpose of declared types: they prevent accidental assignment
//! of the wrong type to a variable.
//!
//! But in some cases it is useful to "shadow" or "redeclare" a variable with a new type, and we
//! permit this, as long as it is done with an explicit re-annotation. So `path: Path =
//! Path(path)`, with the explicit `: Path` annotation, is permitted.
//!
//! The general rule is that whatever declaration(s) can reach a given binding determine the
//! validity of that binding. If there is a path in which the place is not declared, that is a
//! declaration of `Unknown`. If multiple declarations can reach a binding, we union them, but by
//! default we also issue a type error, since this implicit union of declared types may hide an
//! error.
//!
//! To support type inference, we build a map from each use of a place to the bindings live at
//! that use, and the type narrowing constraints that apply to each binding.
//!
//! Let's take this code sample:
//!
//! ```python
//! x = 1
//! x = 2
//! y = x
//! if flag:
//!     x = 3
//! else:
//!     x = 4
//! z = x
//! ```
//!
//! In this snippet, we have four bindings of `x` (the statements assigning `1`, `2`, `3`, and `4`
//! to it), and two uses of `x` (the `y = x` and `z = x` assignments). The first binding of `x`
//! does not reach any use, because it's immediately replaced by the second binding, before any use
//! happens. (A linter could thus flag the statement `x = 1` as likely superfluous.)
//!
//! The first use of `x` has one live binding: the assignment `x = 2`.
//!
//! Things get a bit more complex when we have branches. We will definitely take either the `if` or
//! the `else` branch. Thus, the second use of `x` has two live bindings: `x = 3` and `x = 4`. The
//! `x = 2` assignment is no longer visible, because it must be replaced by either `x = 3` or `x =
//! 4`, no matter which branch was taken. We don't know which branch was taken, so we must consider
//! both bindings as live, which means eventually we would (in type inference) look at these two
//! bindings and infer a type of `Literal[3, 4]` -- the union of `Literal[3]` and `Literal[4]` --
//! for the second use of `x`.
//!
//! So that's one question our use-def map needs to answer: given a specific use of a place, which
//! binding(s) can reach that use. In [`AstIds`](crate::semantic_index::ast_ids::AstIds) we number
//! all uses (that means a `Name`/`ExprAttribute`/`ExprSubscript` node with `Load` context)
//! so we have a `ScopedUseId` to efficiently represent each use.
//!
//! We also need to know, for a given definition of a place, what type narrowing constraints apply
//! to it. For instance, in this code sample:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     use(x)
//! ```
//!
//! At the use of `x`, the live binding of `x` is `1 if flag else None`, which would infer as the
//! type `Literal[1] | None`. But the constraint `x is not None` dominates this use, which means we
//! can rule out the possibility that `x` is `None` here, which should give us the type
//! `Literal[1]` for this use.
//!
//! For declared types, we need to be able to answer the question "given a binding to a place,
//! which declarations of that place can reach the binding?" This allows us to emit a diagnostic
//! if the binding is attempting to bind a value of a type that is not assignable to the declared
//! type for that place, at that point in control flow.
//!
//! We also need to know, given a declaration of a place, what the inferred type of that place is
//! at that point. This allows us to emit a diagnostic in a case like `x = "foo"; x: int`. The
//! binding `x = "foo"` occurs before the declaration `x: int`, so according to our
//! control-flow-sensitive interpretation of declarations, the assignment is not an error. But the
//! declaration is an error, since it would violate the "inferred type must be assignable to
//! declared type" rule.
//!
//! Another case we need to handle is when a place is referenced from a different scope (for
//! example, an import or a nonlocal reference). We call this "public" use of a place. For public
//! use of a place, we prefer the declared type, if there are any declarations of that place; if
//! not, we fall back to the inferred type. So we also need to know which declarations and bindings
//! can reach the end of the scope.
//!
//! Technically, public use of a place could occur from any point in control flow of the scope
//! where the place is defined (via inline imports and import cycles, in the case of an import, or
//! via a function call partway through the local scope that ends up using a place from the scope
//! via a global or nonlocal reference.) But modeling this fully accurately requires whole-program
//! analysis that isn't tractable for an efficient analysis, since it means a given place could
//! have a different type every place it's referenced throughout the program, depending on the
//! shape of arbitrarily-sized call/import graphs. So we follow other Python type checkers in
//! making the simplifying assumption that usually the scope will finish execution before its
//! places are made visible to other scopes; for instance, most imports will import from a
//! complete module, not a partially-executed module. (We may want to get a little smarter than
//! this in the future for some closures, but for now this is where we start.)
//!
//! The data structure we build to answer these questions is the `UseDefMap`. It has a
//! `bindings_by_use` vector of [`Bindings`] indexed by [`ScopedUseId`], a
//! `declarations_by_binding` vector of [`Declarations`] indexed by [`ScopedDefinitionId`], a
//! `bindings_by_declaration` vector of [`Bindings`] indexed by [`ScopedDefinitionId`], and
//! `public_bindings` and `public_definitions` vectors indexed by [`ScopedPlaceId`]. The values in
//! each of these vectors are (in principle) a list of live bindings at that use/definition, or at
//! the end of the scope for that place, with a list of the dominating constraints for each
//! binding.
//!
//! In order to avoid vectors-of-vectors-of-vectors and all the allocations that would entail, we
//! don't actually store these "list of visible definitions" as a vector of [`Definition`].
//! Instead, [`Bindings`] and [`Declarations`] are structs which use bit-sets to track
//! definitions (and constraints, in the case of bindings) in terms of [`ScopedDefinitionId`] and
//! [`ScopedPredicateId`], which are indices into the `all_definitions` and `predicates`
//! indexvecs in the [`UseDefMap`].
//!
//! There is another special kind of possible "definition" for a place: there might be a path from
//! the scope entry to a given use in which the place is never bound. We model this with a special
//! "unbound/undeclared" definition (a [`DefinitionState::Undefined`] entry at the start of the
//! `all_definitions` vector). If that sentinel definition is present in the live bindings at a
//! given use, it means that there is a possible path through control flow in which that place is
//! unbound. Similarly, if that sentinel is present in the live declarations, it means that the
//! place is (possibly) undeclared.
//!
//! To build a [`UseDefMap`], the [`UseDefMapBuilder`] is notified of each new use, definition, and
//! constraint as they are encountered by the
//! [`SemanticIndexBuilder`](crate::semantic_index::builder::SemanticIndexBuilder) AST visit. For
//! each place, the builder tracks the `PlaceState` (`Bindings` and `Declarations`) for that place.
//! When we hit a use or definition of a place, we record the necessary parts of the current state
//! for that place that we need for that use or definition. When we reach the end of the scope, it
//! records the state for each place as the public definitions of that place.
//!
//! Let's walk through the above example. Initially we do not have any record of `x`. When we add
//! the new place (before we process the first binding), we create a new undefined `PlaceState`
//! which has a single live binding (the "unbound" definition) and a single live declaration (the
//! "undeclared" definition). When we see `x = 1`, we record that as the sole live binding of `x`.
//! The "unbound" binding is no longer visible. Then we see `x = 2`, and we replace `x = 1` as the
//! sole live binding of `x`. When we get to `y = x`, we record that the live bindings for that use
//! of `x` are just the `x = 2` definition.
//!
//! Then we hit the `if` branch. We visit the `test` node (`flag` in this case), since that will
//! happen regardless. Then we take a pre-branch snapshot of the current state for all places,
//! which we'll need later. Then we record `flag` as a possible constraint on the current binding
//! (`x = 2`), and go ahead and visit the `if` body. When we see `x = 3`, it replaces `x = 2`
//! (constrained by `flag`) as the sole live binding of `x`. At the end of the `if` body, we take
//! another snapshot of the current place state; we'll call this the post-if-body snapshot.
//!
//! Now we need to visit the `else` clause. The conditions when entering the `else` clause should
//! be the pre-if conditions; if we are entering the `else` clause, we know that the `if` test
//! failed and we didn't execute the `if` body. So we first reset the builder to the pre-if state,
//! using the snapshot we took previously (meaning we now have `x = 2` as the sole binding for `x`
//! again), and record a *negative* `flag` constraint for all live bindings (`x = 2`). We then
//! visit the `else` clause, where `x = 4` replaces `x = 2` as the sole live binding of `x`.
//!
//! Now we reach the end of the if/else, and want to visit the following code. The state here needs
//! to reflect that we might have gone through the `if` branch, or we might have gone through the
//! `else` branch, and we don't know which. So we need to "merge" our current builder state
//! (reflecting the end-of-else state, with `x = 4` as the only live binding) with our post-if-body
//! snapshot (which has `x = 3` as the only live binding). The result of this merge is that we now
//! have two live bindings of `x`: `x = 3` and `x = 4`.
//!
//! Another piece of information that the `UseDefMap` needs to provide are visibility constraints.
//! These are similar to the narrowing constraints, but apply to bindings and declarations within a
//! control flow path. Consider the following example:
//! ```py
//! x = 1
//! if test:
//!     x = 2
//!     y = "y"
//! ```
//! In principle, there are two possible control flow paths here. However, if we can statically
//! infer `test` to be always truthy or always falsy (that is, `__bool__` of `test` is of type
//! `Literal[True]` or `Literal[False]`), we can rule out one of the possible paths. To support
//! this feature, we record a visibility constraint of `test` to all live bindings and declarations
//! *after* visiting the body of the `if` statement. And we record a negative visibility constraint
//! `~test` to all live bindings/declarations in the (implicit) `else` branch. For the example
//! above, we would record the following visibility constraints (adding the implicit "unbound"
//! definitions for clarity):
//! ```py
//! x = <unbound>  # not live, shadowed by `x = 1`
//! y = <unbound>  # visibility constraint: ~test
//!
//! x = 1  # visibility constraint: ~test
//! if test:
//!     x = 2  # visibility constraint: test
//!     y = "y"  # visibility constraint: test
//! ```
//! When we encounter a use of `x` after this `if` statement, we would record two live bindings: `x
//! = 1` with a constraint of `~test`, and `x = 2` with a constraint of `test`. In type inference,
//! when we iterate over all live bindings, we can evaluate these constraints to determine if a
//! particular binding is actually visible. For example, if `test` is always truthy, we only see
//! the `x = 2` binding. If `test` is always falsy, we only see the `x = 1` binding. And if the
//! `__bool__` method of `test` returns type `bool`, we can see both bindings.
//!
//! Note that we also record visibility constraints for the start of the scope. This is important
//! to determine if a place is definitely bound, possibly unbound, or definitely unbound. In the
//! example above, The `y = <unbound>` binding is constrained by `~test`, so `y` would only be
//! definitely-bound if `test` is always truthy.
//!
//! The [`UseDefMapBuilder`] itself just exposes methods for taking a snapshot, resetting to a
//! snapshot, and merging a snapshot into the current state. The logic using these methods lives in
//! [`SemanticIndexBuilder`](crate::semantic_index::builder::SemanticIndexBuilder), e.g. where it
//! visits a `StmtIf` node.

use ruff_index::{IndexVec, newtype_index};
use rustc_hash::FxHashMap;

use self::place_state::{
    Bindings, Declarations, EagerSnapshot, LiveBindingsIterator, LiveDeclaration,
    LiveDeclarationsIterator, PlaceState, ScopedDefinitionId,
};
use crate::node_key::NodeKey;
use crate::semantic_index::EagerSnapshotResult;
use crate::semantic_index::ast_ids::ScopedUseId;
use crate::semantic_index::definition::{Definition, DefinitionState};
use crate::semantic_index::narrowing_constraints::{
    ConstraintKey, NarrowingConstraints, NarrowingConstraintsBuilder, NarrowingConstraintsIterator,
};
use crate::semantic_index::place::{FileScopeId, PlaceExpr, ScopeKind, ScopedPlaceId};
use crate::semantic_index::predicate::{
    Predicate, Predicates, PredicatesBuilder, ScopedPredicateId, StarImportPlaceholderPredicate,
};
use crate::semantic_index::visibility_constraints::{
    ScopedVisibilityConstraintId, VisibilityConstraints, VisibilityConstraintsBuilder,
};
use crate::types::{IntersectionBuilder, Truthiness, Type, infer_narrowing_constraint};

mod place_state;

/// Applicable definitions and constraints for every use of a name.
#[derive(Debug, PartialEq, Eq, salsa::Update)]
pub(crate) struct UseDefMap<'db> {
    /// Array of [`Definition`] in this scope. Only the first entry should be [`DefinitionState::Undefined`];
    /// this represents the implicit "unbound"/"undeclared" definition of every place.
    all_definitions: IndexVec<ScopedDefinitionId, DefinitionState<'db>>,

    /// Array of predicates in this scope.
    predicates: Predicates<'db>,

    /// Array of narrowing constraints in this scope.
    narrowing_constraints: NarrowingConstraints,

    /// Array of visibility constraints in this scope.
    visibility_constraints: VisibilityConstraints,

    /// [`Bindings`] reaching a [`ScopedUseId`].
    bindings_by_use: IndexVec<ScopedUseId, Bindings>,

    /// Tracks whether or not a given AST node is reachable from the start of the scope.
    node_reachability: FxHashMap<NodeKey, ScopedVisibilityConstraintId>,

    /// If the definition is a binding (only) -- `x = 1` for example -- then we need
    /// [`Declarations`] to know whether this binding is permitted by the live declarations.
    ///
    /// If the definition is both a declaration and a binding -- `x: int = 1` for example -- then
    /// we don't actually need anything here, all we'll need to validate is that our own RHS is a
    /// valid assignment to our own annotation.
    declarations_by_binding: FxHashMap<Definition<'db>, Declarations>,

    /// If the definition is a declaration (only) -- `x: int` for example -- then we need
    /// [`Bindings`] to know whether this declaration is consistent with the previously
    /// inferred type.
    ///
    /// If the definition is both a declaration and a binding -- `x: int = 1` for example -- then
    /// we don't actually need anything here, all we'll need to validate is that our own RHS is a
    /// valid assignment to our own annotation.
    bindings_by_declaration: FxHashMap<Definition<'db>, Bindings>,

    /// [`PlaceState`] visible at end of scope for each place.
    public_places: IndexVec<ScopedPlaceId, PlaceState>,

    /// Snapshot of bindings in this scope that can be used to resolve a reference in a nested
    /// eager scope.
    eager_snapshots: EagerSnapshots,

    /// Whether or not the start of the scope is visible.
    /// This is used to check if the function can implicitly return `None`.
    /// For example:
    ///
    /// ```python
    /// def f(cond: bool) -> int:
    ///     if cond:
    ///        return 1
    /// ```
    ///
    /// In this case, the function may implicitly return `None`.
    ///
    /// This is used by `UseDefMap::can_implicit_return`.
    scope_start_visibility: ScopedVisibilityConstraintId,
}

impl<'db> UseDefMap<'db> {
    pub(crate) fn bindings_at_use(
        &self,
        use_id: ScopedUseId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(&self.bindings_by_use[use_id])
    }

    pub(crate) fn narrowing_constraints_at_use(
        &self,
        constraint_key: ConstraintKey,
    ) -> ConstraintsIterator<'_, 'db> {
        let constraint = match constraint_key {
            ConstraintKey::NarrowingConstraint(constraint) => constraint,
            ConstraintKey::UseId(use_id) => {
                self.bindings_by_use[use_id].unbound_narrowing_constraint()
            }
        };
        ConstraintsIterator {
            predicates: &self.predicates,
            constraint_ids: self.narrowing_constraints.iter_predicates(constraint),
        }
    }

    pub(super) fn is_reachable(
        &self,
        db: &dyn crate::Db,
        reachability: ScopedVisibilityConstraintId,
    ) -> bool {
        !self
            .visibility_constraints
            .evaluate(db, &self.predicates, reachability)
            .is_always_false()
    }

    /// Check whether or not a given expression is reachable from the start of the scope. This
    /// is a local analysis which does not capture the possibility that the entire scope might
    /// be unreachable. Use [`super::SemanticIndex::is_node_reachable`] for the global
    /// analysis.
    #[track_caller]
    pub(super) fn is_node_reachable(&self, db: &dyn crate::Db, node_key: NodeKey) -> bool {
        !self
            .visibility_constraints
            .evaluate(
                db,
                &self.predicates,
                *self
                    .node_reachability
                    .get(&node_key)
                    .expect("`is_node_reachable` should only be called on AST nodes with recorded reachability"),
            )
            .is_always_false()
    }

    pub(crate) fn public_bindings(
        &self,
        place: ScopedPlaceId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(self.public_places[place].bindings())
    }

    pub(crate) fn eager_snapshot(
        &self,
        eager_bindings: ScopedEagerSnapshotId,
    ) -> EagerSnapshotResult<'_, 'db> {
        match self.eager_snapshots.get(eager_bindings) {
            Some(EagerSnapshot::Constraint(constraint)) => {
                EagerSnapshotResult::FoundConstraint(*constraint)
            }
            Some(EagerSnapshot::Bindings(bindings)) => {
                EagerSnapshotResult::FoundBindings(self.bindings_iterator(bindings))
            }
            None => EagerSnapshotResult::NotFound,
        }
    }

    pub(crate) fn bindings_at_declaration(
        &self,
        declaration: Definition<'db>,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(&self.bindings_by_declaration[&declaration])
    }

    pub(crate) fn declarations_at_binding(
        &self,
        binding: Definition<'db>,
    ) -> DeclarationsIterator<'_, 'db> {
        self.declarations_iterator(&self.declarations_by_binding[&binding])
    }

    pub(crate) fn public_declarations<'map>(
        &'map self,
        place: ScopedPlaceId,
    ) -> DeclarationsIterator<'map, 'db> {
        let declarations = self.public_places[place].declarations();
        self.declarations_iterator(declarations)
    }

    pub(crate) fn all_public_declarations<'map>(
        &'map self,
    ) -> impl Iterator<Item = (ScopedPlaceId, DeclarationsIterator<'map, 'db>)> + 'map {
        (0..self.public_places.len())
            .map(ScopedPlaceId::from_usize)
            .map(|place_id| (place_id, self.public_declarations(place_id)))
    }

    pub(crate) fn all_public_bindings<'map>(
        &'map self,
    ) -> impl Iterator<Item = (ScopedPlaceId, BindingWithConstraintsIterator<'map, 'db>)> + 'map
    {
        (0..self.public_places.len())
            .map(ScopedPlaceId::from_usize)
            .map(|place_id| (place_id, self.public_bindings(place_id)))
    }

    /// This function is intended to be called only once inside `TypeInferenceBuilder::infer_function_body`.
    pub(crate) fn can_implicit_return(&self, db: &dyn crate::Db) -> bool {
        !self
            .visibility_constraints
            .evaluate(db, &self.predicates, self.scope_start_visibility)
            .is_always_false()
    }

    pub(crate) fn is_binding_visible(
        &self,
        db: &dyn crate::Db,
        binding: &BindingWithConstraints<'_, 'db>,
    ) -> Truthiness {
        self.visibility_constraints
            .evaluate(db, &self.predicates, binding.visibility_constraint)
    }

    fn bindings_iterator<'map>(
        &'map self,
        bindings: &'map Bindings,
    ) -> BindingWithConstraintsIterator<'map, 'db> {
        BindingWithConstraintsIterator {
            all_definitions: &self.all_definitions,
            predicates: &self.predicates,
            narrowing_constraints: &self.narrowing_constraints,
            visibility_constraints: &self.visibility_constraints,
            inner: bindings.iter(),
        }
    }

    fn declarations_iterator<'map>(
        &'map self,
        declarations: &'map Declarations,
    ) -> DeclarationsIterator<'map, 'db> {
        DeclarationsIterator {
            all_definitions: &self.all_definitions,
            predicates: &self.predicates,
            visibility_constraints: &self.visibility_constraints,
            inner: declarations.iter(),
        }
    }
}

/// Uniquely identifies a snapshot of a place state that can be used to resolve a reference in a
/// nested eager scope.
///
/// An eager scope has its entire body executed immediately at the location where it is defined.
/// For any free references in the nested scope, we use the bindings that are visible at the point
/// where the nested scope is defined, instead of using the public type of the place.
///
/// There is a unique ID for each distinct [`EagerSnapshotKey`] in the file.
#[newtype_index]
pub(crate) struct ScopedEagerSnapshotId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct EagerSnapshotKey {
    /// The enclosing scope containing the bindings
    pub(crate) enclosing_scope: FileScopeId,
    /// The referenced place (in the enclosing scope)
    pub(crate) enclosing_place: ScopedPlaceId,
    /// The nested eager scope containing the reference
    pub(crate) nested_scope: FileScopeId,
}

/// A snapshot of place states that can be used to resolve a reference in a nested eager scope.
type EagerSnapshots = IndexVec<ScopedEagerSnapshotId, EagerSnapshot>;

#[derive(Debug)]
pub(crate) struct BindingWithConstraintsIterator<'map, 'db> {
    all_definitions: &'map IndexVec<ScopedDefinitionId, DefinitionState<'db>>,
    pub(crate) predicates: &'map Predicates<'db>,
    pub(crate) narrowing_constraints: &'map NarrowingConstraints,
    pub(crate) visibility_constraints: &'map VisibilityConstraints,
    inner: LiveBindingsIterator<'map>,
}

impl<'map, 'db> Iterator for BindingWithConstraintsIterator<'map, 'db> {
    type Item = BindingWithConstraints<'map, 'db>;

    fn next(&mut self) -> Option<Self::Item> {
        let predicates = self.predicates;
        let narrowing_constraints = self.narrowing_constraints;

        self.inner
            .next()
            .map(|live_binding| BindingWithConstraints {
                binding: self.all_definitions[live_binding.binding],
                narrowing_constraint: ConstraintsIterator {
                    predicates,
                    constraint_ids: narrowing_constraints
                        .iter_predicates(live_binding.narrowing_constraint),
                },
                visibility_constraint: live_binding.visibility_constraint,
            })
    }
}

impl std::iter::FusedIterator for BindingWithConstraintsIterator<'_, '_> {}

pub(crate) struct BindingWithConstraints<'map, 'db> {
    pub(crate) binding: DefinitionState<'db>,
    pub(crate) narrowing_constraint: ConstraintsIterator<'map, 'db>,
    pub(crate) visibility_constraint: ScopedVisibilityConstraintId,
}

pub(crate) struct ConstraintsIterator<'map, 'db> {
    predicates: &'map Predicates<'db>,
    constraint_ids: NarrowingConstraintsIterator<'map>,
}

impl<'db> Iterator for ConstraintsIterator<'_, 'db> {
    type Item = Predicate<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.constraint_ids
            .next()
            .map(|narrowing_constraint| self.predicates[narrowing_constraint.predicate()])
    }
}

impl std::iter::FusedIterator for ConstraintsIterator<'_, '_> {}

impl<'db> ConstraintsIterator<'_, 'db> {
    pub(crate) fn narrow(
        self,
        db: &'db dyn crate::Db,
        base_ty: Type<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        let constraint_tys: Vec<_> = self
            .filter_map(|constraint| infer_narrowing_constraint(db, constraint, place))
            .collect();

        if constraint_tys.is_empty() {
            base_ty
        } else {
            constraint_tys
                .into_iter()
                .rev()
                .fold(
                    IntersectionBuilder::new(db).add_positive(base_ty),
                    IntersectionBuilder::add_positive,
                )
                .build()
        }
    }
}

#[derive(Clone)]
pub(crate) struct DeclarationsIterator<'map, 'db> {
    all_definitions: &'map IndexVec<ScopedDefinitionId, DefinitionState<'db>>,
    pub(crate) predicates: &'map Predicates<'db>,
    pub(crate) visibility_constraints: &'map VisibilityConstraints,
    inner: LiveDeclarationsIterator<'map>,
}

pub(crate) struct DeclarationWithConstraint<'db> {
    pub(crate) declaration: DefinitionState<'db>,
    pub(crate) visibility_constraint: ScopedVisibilityConstraintId,
}

impl<'db> Iterator for DeclarationsIterator<'_, 'db> {
    type Item = DeclarationWithConstraint<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(
            |LiveDeclaration {
                 declaration,
                 visibility_constraint,
             }| {
                DeclarationWithConstraint {
                    declaration: self.all_definitions[*declaration],
                    visibility_constraint: *visibility_constraint,
                }
            },
        )
    }
}

impl std::iter::FusedIterator for DeclarationsIterator<'_, '_> {}

/// A snapshot of the definitions and constraints state at a particular point in control flow.
#[derive(Clone, Debug)]
pub(super) struct FlowSnapshot {
    place_states: IndexVec<ScopedPlaceId, PlaceState>,
    scope_start_visibility: ScopedVisibilityConstraintId,
    reachability: ScopedVisibilityConstraintId,
}

#[derive(Debug)]
pub(super) struct UseDefMapBuilder<'db> {
    /// Append-only array of [`Definition`].
    all_definitions: IndexVec<ScopedDefinitionId, DefinitionState<'db>>,

    /// Builder of predicates.
    pub(super) predicates: PredicatesBuilder<'db>,

    /// Builder of narrowing constraints.
    pub(super) narrowing_constraints: NarrowingConstraintsBuilder,

    /// Builder of visibility constraints.
    pub(super) visibility_constraints: VisibilityConstraintsBuilder,

    /// A constraint which describes the visibility of the unbound/undeclared state, i.e.
    /// whether or not a use of a place at the current point in control flow would see
    /// the fake `x = <unbound>` binding at the start of the scope. This is important for
    /// cases like the following, where we need to hide the implicit unbound binding in
    /// the "else" branch:
    /// ```py
    /// # x = <unbound>
    ///
    /// if True:
    ///     x = 1
    ///
    /// use(x)  # the `x = <unbound>` binding is not visible here
    /// ```
    pub(super) scope_start_visibility: ScopedVisibilityConstraintId,

    /// Live bindings at each so-far-recorded use.
    bindings_by_use: IndexVec<ScopedUseId, Bindings>,

    /// Tracks whether or not the scope start is visible at the current point in control flow.
    /// This is subtly different from `scope_start_visibility`, as we apply these constraints
    /// at the beginning of a branch. Visibility constraints, on the other hand, need to be
    /// applied at the end of a branch, as we apply them retroactively to all live bindings:
    /// ```py
    /// y = 1
    ///
    /// if test:
    ///    # we record a reachability constraint of [test] here,
    ///    # so that it can affect the use of `x`:
    ///
    ///    x  # we store a reachability constraint of [test] for this use of `x`
    ///
    ///    y = 2
    ///
    ///    # we record a visibility constraint of [test] here, which retroactively affects
    ///    # the `y = 1` and the `y = 2` binding.
    /// else:
    ///    # we record a reachability constraint of [~test] here.
    ///
    ///    pass
    ///
    ///    # we record a visibility constraint of [~test] here, which retroactively affects
    ///    # the `y = 1` binding.
    ///
    /// use(y)
    /// ```
    /// Depending on the value of `test`, the `y = 1`, `y = 2`, or both bindings may be visible.
    /// The use of `x` is recorded with a reachability constraint of `[test]`.
    pub(super) reachability: ScopedVisibilityConstraintId,

    /// Tracks whether or not a given AST node is reachable from the start of the scope.
    node_reachability: FxHashMap<NodeKey, ScopedVisibilityConstraintId>,

    /// Live declarations for each so-far-recorded binding.
    declarations_by_binding: FxHashMap<Definition<'db>, Declarations>,

    /// Live bindings for each so-far-recorded declaration.
    bindings_by_declaration: FxHashMap<Definition<'db>, Bindings>,

    /// Currently live bindings and declarations for each place.
    place_states: IndexVec<ScopedPlaceId, PlaceState>,

    /// Snapshots of place states in this scope that can be used to resolve a reference in a
    /// nested eager scope.
    eager_snapshots: EagerSnapshots,

    /// Is this a class scope?
    is_class_scope: bool,
}

impl<'db> UseDefMapBuilder<'db> {
    pub(super) fn new(is_class_scope: bool) -> Self {
        Self {
            all_definitions: IndexVec::from_iter([DefinitionState::Undefined]),
            predicates: PredicatesBuilder::default(),
            narrowing_constraints: NarrowingConstraintsBuilder::default(),
            visibility_constraints: VisibilityConstraintsBuilder::default(),
            scope_start_visibility: ScopedVisibilityConstraintId::ALWAYS_TRUE,
            bindings_by_use: IndexVec::new(),
            reachability: ScopedVisibilityConstraintId::ALWAYS_TRUE,
            node_reachability: FxHashMap::default(),
            declarations_by_binding: FxHashMap::default(),
            bindings_by_declaration: FxHashMap::default(),
            place_states: IndexVec::new(),
            eager_snapshots: EagerSnapshots::default(),
            is_class_scope,
        }
    }
    pub(super) fn mark_unreachable(&mut self) {
        self.record_visibility_constraint(ScopedVisibilityConstraintId::ALWAYS_FALSE);
        self.reachability = ScopedVisibilityConstraintId::ALWAYS_FALSE;
    }

    pub(super) fn add_place(&mut self, place: ScopedPlaceId) {
        let new_place = self
            .place_states
            .push(PlaceState::undefined(self.scope_start_visibility));
        debug_assert_eq!(place, new_place);
    }

    pub(super) fn record_binding(
        &mut self,
        place: ScopedPlaceId,
        binding: Definition<'db>,
        is_place_name: bool,
    ) {
        let def_id = self.all_definitions.push(DefinitionState::Defined(binding));
        let place_state = &mut self.place_states[place];
        self.declarations_by_binding
            .insert(binding, place_state.declarations().clone());
        place_state.record_binding(
            def_id,
            self.scope_start_visibility,
            self.is_class_scope,
            is_place_name,
        );
    }

    pub(super) fn add_predicate(&mut self, predicate: Predicate<'db>) -> ScopedPredicateId {
        self.predicates.add_predicate(predicate)
    }

    pub(super) fn record_narrowing_constraint(&mut self, predicate: ScopedPredicateId) {
        let narrowing_constraint = predicate.into();
        for state in &mut self.place_states {
            state
                .record_narrowing_constraint(&mut self.narrowing_constraints, narrowing_constraint);
        }
    }

    pub(super) fn record_visibility_constraint(
        &mut self,
        constraint: ScopedVisibilityConstraintId,
    ) {
        for state in &mut self.place_states {
            state.record_visibility_constraint(&mut self.visibility_constraints, constraint);
        }
        self.scope_start_visibility = self
            .visibility_constraints
            .add_and_constraint(self.scope_start_visibility, constraint);
    }

    /// Snapshot the state of a single place at the current point in control flow.
    ///
    /// This is only used for `*`-import visibility constraints, which are handled differently
    /// to most other visibility constraints. See the doc-comment for
    /// [`Self::record_and_negate_star_import_visibility_constraint`] for more details.
    pub(super) fn single_place_snapshot(&self, place: ScopedPlaceId) -> PlaceState {
        self.place_states[place].clone()
    }

    /// This method exists solely for handling `*`-import visibility constraints.
    ///
    /// The reason why we add visibility constraints for [`Definition`]s created by `*` imports
    /// is laid out in the doc-comment for [`StarImportPlaceholderPredicate`]. But treating these
    /// visibility constraints in the use-def map the same way as all other visibility constraints
    /// was shown to lead to [significant regressions] for small codebases where typeshed
    /// dominates. (Although `*` imports are not common generally, they are used in several
    /// important places by typeshed.)
    ///
    /// To solve these regressions, it was observed that we could do significantly less work for
    /// `*`-import definitions. We do a number of things differently here to our normal handling of
    /// visibility constraints:
    ///
    /// - We only apply and negate the visibility constraints to a single symbol, rather than to
    ///   all symbols. This is possible here because, unlike most definitions, we know in advance that
    ///   exactly one definition occurs inside the "if-true" predicate branch, and we know exactly
    ///   which definition it is.
    ///
    ///   Doing things this way is cheaper in and of itself. However, it also allows us to avoid
    ///   calling [`Self::simplify_visibility_constraints`] after the constraint has been applied to
    ///   the "if-predicate-true" branch and negated for the "if-predicate-false" branch. Simplifying
    ///   the visibility constraints is only important for places that did not have any new
    ///   definitions inside either the "if-predicate-true" branch or the "if-predicate-false" branch.
    ///
    /// - We only snapshot the state for a single place prior to the definition, rather than doing
    ///   expensive calls to [`Self::snapshot`]. Again, this is possible because we know
    ///   that only a single definition occurs inside the "if-predicate-true" predicate branch.
    ///
    /// - Normally we take care to check whether an "if-predicate-true" branch or an
    ///   "if-predicate-false" branch contains a terminal statement: these can affect the visibility
    ///   of symbols defined inside either branch. However, in the case of `*`-import definitions,
    ///   this is unnecessary (and therefore not done in this method), since we know that a `*`-import
    ///   predicate cannot create a terminal statement inside either branch.
    ///
    /// [significant regressions]: https://github.com/astral-sh/ruff/pull/17286#issuecomment-2786755746
    pub(super) fn record_and_negate_star_import_visibility_constraint(
        &mut self,
        star_import: StarImportPlaceholderPredicate<'db>,
        symbol: ScopedPlaceId,
        pre_definition_state: PlaceState,
    ) {
        let predicate_id = self.add_predicate(star_import.into());
        let visibility_id = self.visibility_constraints.add_atom(predicate_id);
        let negated_visibility_id = self
            .visibility_constraints
            .add_not_constraint(visibility_id);

        let mut post_definition_state =
            std::mem::replace(&mut self.place_states[symbol], pre_definition_state);

        post_definition_state
            .record_visibility_constraint(&mut self.visibility_constraints, visibility_id);

        self.place_states[symbol]
            .record_visibility_constraint(&mut self.visibility_constraints, negated_visibility_id);

        self.place_states[symbol].merge(
            post_definition_state,
            &mut self.narrowing_constraints,
            &mut self.visibility_constraints,
        );
    }

    /// This method resets the visibility constraints for all places to a previous state
    /// *if* there have been no new declarations or bindings since then. Consider the
    /// following example:
    /// ```py
    /// x = 0
    /// y = 0
    /// if test_a:
    ///     y = 1
    /// elif test_b:
    ///     y = 2
    /// elif test_c:
    ///    y = 3
    ///
    /// # RESET
    /// ```
    /// We build a complex visibility constraint for the `y = 0` binding. We build the same
    /// constraint for the `x = 0` binding as well, but at the `RESET` point, we can get rid
    /// of it, as the `if`-`elif`-`elif` chain doesn't include any new bindings of `x`.
    pub(super) fn simplify_visibility_constraints(&mut self, snapshot: FlowSnapshot) {
        debug_assert!(self.place_states.len() >= snapshot.place_states.len());

        // If there are any control flow paths that have become unreachable between `snapshot` and
        // now, then it's not valid to simplify any visibility constraints to `snapshot`.
        if self.scope_start_visibility != snapshot.scope_start_visibility {
            return;
        }

        // Note that this loop terminates when we reach a place not present in the snapshot.
        // This means we keep visibility constraints for all new places, which is intended,
        // since these places have been introduced in the corresponding branch, which might
        // be subject to visibility constraints. We only simplify/reset visibility constraints
        // for places that have the same bindings and declarations present compared to the
        // snapshot.
        for (current, snapshot) in self.place_states.iter_mut().zip(snapshot.place_states) {
            current.simplify_visibility_constraints(snapshot);
        }
    }

    pub(super) fn record_reachability_constraint(
        &mut self,
        constraint: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        self.reachability = self
            .visibility_constraints
            .add_and_constraint(self.reachability, constraint);
        self.reachability
    }

    pub(super) fn record_declaration(
        &mut self,
        place: ScopedPlaceId,
        declaration: Definition<'db>,
    ) {
        let def_id = self
            .all_definitions
            .push(DefinitionState::Defined(declaration));
        let place_state = &mut self.place_states[place];
        self.bindings_by_declaration
            .insert(declaration, place_state.bindings().clone());
        place_state.record_declaration(def_id);
    }

    pub(super) fn record_declaration_and_binding(
        &mut self,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        is_place_name: bool,
    ) {
        // We don't need to store anything in self.bindings_by_declaration or
        // self.declarations_by_binding.
        let def_id = self
            .all_definitions
            .push(DefinitionState::Defined(definition));
        let place_state = &mut self.place_states[place];
        place_state.record_declaration(def_id);
        place_state.record_binding(
            def_id,
            self.scope_start_visibility,
            self.is_class_scope,
            is_place_name,
        );
    }

    pub(super) fn delete_binding(&mut self, place: ScopedPlaceId, is_place_name: bool) {
        let def_id = self.all_definitions.push(DefinitionState::Deleted);
        let place_state = &mut self.place_states[place];
        place_state.record_binding(
            def_id,
            self.scope_start_visibility,
            self.is_class_scope,
            is_place_name,
        );
    }

    pub(super) fn record_use(
        &mut self,
        place: ScopedPlaceId,
        use_id: ScopedUseId,
        node_key: NodeKey,
    ) {
        // We have a use of a place; clone the current bindings for that place, and record them
        // as the live bindings for this use.
        let new_use = self
            .bindings_by_use
            .push(self.place_states[place].bindings().clone());
        debug_assert_eq!(use_id, new_use);

        // Track reachability of all uses of places to silence `unresolved-reference`
        // diagnostics in unreachable code.
        self.record_node_reachability(node_key);
    }

    pub(super) fn record_node_reachability(&mut self, node_key: NodeKey) {
        self.node_reachability.insert(node_key, self.reachability);
    }

    pub(super) fn snapshot_eager_state(
        &mut self,
        enclosing_place: ScopedPlaceId,
        scope: ScopeKind,
        enclosing_place_expr: &PlaceExpr,
    ) -> ScopedEagerSnapshotId {
        // Names bound in class scopes are never visible to nested scopes (but attributes/subscripts are visible),
        // so we never need to save eager scope bindings in a class scope.
        if (scope.is_class() && enclosing_place_expr.is_name()) || !enclosing_place_expr.is_bound()
        {
            self.eager_snapshots.push(EagerSnapshot::Constraint(
                self.place_states[enclosing_place]
                    .bindings()
                    .unbound_narrowing_constraint(),
            ))
        } else {
            self.eager_snapshots.push(EagerSnapshot::Bindings(
                self.place_states[enclosing_place].bindings().clone(),
            ))
        }
    }

    /// Take a snapshot of the current visible-places state.
    pub(super) fn snapshot(&self) -> FlowSnapshot {
        FlowSnapshot {
            place_states: self.place_states.clone(),
            scope_start_visibility: self.scope_start_visibility,
            reachability: self.reachability,
        }
    }

    /// Restore the current builder places state to the given snapshot.
    pub(super) fn restore(&mut self, snapshot: FlowSnapshot) {
        // We never remove places from `place_states` (it's an IndexVec, and the place
        // IDs must line up), so the current number of known places must always be equal to or
        // greater than the number of known places in a previously-taken snapshot.
        let num_places = self.place_states.len();
        debug_assert!(num_places >= snapshot.place_states.len());

        // Restore the current visible-definitions state to the given snapshot.
        self.place_states = snapshot.place_states;
        self.scope_start_visibility = snapshot.scope_start_visibility;
        self.reachability = snapshot.reachability;

        // If the snapshot we are restoring is missing some places we've recorded since, we need
        // to fill them in so the place IDs continue to line up. Since they don't exist in the
        // snapshot, the correct state to fill them in with is "undefined".
        self.place_states.resize(
            num_places,
            PlaceState::undefined(self.scope_start_visibility),
        );
    }

    /// Merge the given snapshot into the current state, reflecting that we might have taken either
    /// path to get here. The new state for each place should include definitions from both the
    /// prior state and the snapshot.
    pub(super) fn merge(&mut self, snapshot: FlowSnapshot) {
        // As an optimization, if we know statically that either of the snapshots is always
        // unreachable, we can leave it out of the merged result entirely. Note that we cannot
        // perform any type inference at this point, so this is largely limited to unreachability
        // via terminal statements. If a flow's reachability depends on an expression in the code,
        // we will include the flow in the merged result; the visibility constraints of its
        // bindings will include this reachability condition, so that later during type inference,
        // we can determine whether any particular binding is non-visible due to unreachability.
        if snapshot.scope_start_visibility == ScopedVisibilityConstraintId::ALWAYS_FALSE {
            return;
        }
        if self.scope_start_visibility == ScopedVisibilityConstraintId::ALWAYS_FALSE {
            self.restore(snapshot);
            return;
        }

        // We never remove places from `place_states` (it's an IndexVec, and the place
        // IDs must line up), so the current number of known places must always be equal to or
        // greater than the number of known places in a previously-taken snapshot.
        debug_assert!(self.place_states.len() >= snapshot.place_states.len());

        let mut snapshot_definitions_iter = snapshot.place_states.into_iter();
        for current in &mut self.place_states {
            if let Some(snapshot) = snapshot_definitions_iter.next() {
                current.merge(
                    snapshot,
                    &mut self.narrowing_constraints,
                    &mut self.visibility_constraints,
                );
            } else {
                current.merge(
                    PlaceState::undefined(snapshot.scope_start_visibility),
                    &mut self.narrowing_constraints,
                    &mut self.visibility_constraints,
                );
                // Place not present in snapshot, so it's unbound/undeclared from that path.
            }
        }

        self.scope_start_visibility = self
            .visibility_constraints
            .add_or_constraint(self.scope_start_visibility, snapshot.scope_start_visibility);

        self.reachability = self
            .visibility_constraints
            .add_or_constraint(self.reachability, snapshot.reachability);
    }

    pub(super) fn finish(mut self) -> UseDefMap<'db> {
        self.all_definitions.shrink_to_fit();
        self.place_states.shrink_to_fit();
        self.bindings_by_use.shrink_to_fit();
        self.node_reachability.shrink_to_fit();
        self.declarations_by_binding.shrink_to_fit();
        self.bindings_by_declaration.shrink_to_fit();
        self.eager_snapshots.shrink_to_fit();

        UseDefMap {
            all_definitions: self.all_definitions,
            predicates: self.predicates.build(),
            narrowing_constraints: self.narrowing_constraints.build(),
            visibility_constraints: self.visibility_constraints.build(),
            bindings_by_use: self.bindings_by_use,
            node_reachability: self.node_reachability,
            public_places: self.place_states,
            declarations_by_binding: self.declarations_by_binding,
            bindings_by_declaration: self.bindings_by_declaration,
            eager_snapshots: self.eager_snapshots,
            scope_start_visibility: self.scope_start_visibility,
        }
    }
}
