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
//! Another piece of information that the `UseDefMap` needs to provide are reachability constraints.
//! See `reachability_constraints.rs` for more details, in particular how they apply to bindings.
//!
//! The [`UseDefMapBuilder`] itself just exposes methods for taking a snapshot, resetting to a
//! snapshot, and merging a snapshot into the current state. The logic using these methods lives in
//! [`SemanticIndexBuilder`](crate::semantic_index::builder::SemanticIndexBuilder), e.g. where it
//! visits a `StmtIf` node.

use ruff_index::{IndexVec, newtype_index};
use rustc_hash::FxHashMap;

use crate::node_key::NodeKey;
use crate::place::BoundnessAnalysis;
use crate::semantic_index::ast_ids::ScopedUseId;
use crate::semantic_index::definition::{Definition, DefinitionState};
use crate::semantic_index::member::ScopedMemberId;
use crate::semantic_index::narrowing_constraints::{
    ConstraintKey, NarrowingConstraints, NarrowingConstraintsBuilder, NarrowingConstraintsIterator,
    ScopedNarrowingConstraint,
};
use crate::semantic_index::place::{PlaceExprRef, ScopedPlaceId};
use crate::semantic_index::predicate::{
    Predicate, PredicateOrLiteral, Predicates, PredicatesBuilder, ScopedPredicateId,
};
use crate::semantic_index::reachability_constraints::{
    ReachabilityConstraints, ReachabilityConstraintsBuilder, ScopedReachabilityConstraintId,
};
use crate::semantic_index::scope::{FileScopeId, ScopeKind, ScopeLaziness};
use crate::semantic_index::symbol::ScopedSymbolId;
use crate::semantic_index::use_def::place_state::{
    Bindings, Declarations, EnclosingSnapshot, LiveBindingsIterator, LiveDeclaration,
    LiveDeclarationsIterator, PlaceState, PreviousDefinitions, ScopedDefinitionId,
};
use crate::semantic_index::{EnclosingSnapshotResult, SemanticIndex};
use crate::types::{IntersectionBuilder, Truthiness, Type, infer_narrowing_constraint};

mod place_state;

/// Applicable definitions and constraints for every use of a name.
#[derive(Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct UseDefMap<'db> {
    /// Array of [`Definition`] in this scope. Only the first entry should be [`DefinitionState::Undefined`];
    /// this represents the implicit "unbound"/"undeclared" definition of every place.
    all_definitions: IndexVec<ScopedDefinitionId, DefinitionState<'db>>,

    /// Array of predicates in this scope.
    predicates: Predicates<'db>,

    /// Array of narrowing constraints in this scope.
    narrowing_constraints: NarrowingConstraints,

    /// Array of reachability constraints in this scope.
    reachability_constraints: ReachabilityConstraints,

    /// [`Bindings`] reaching a [`ScopedUseId`].
    bindings_by_use: IndexVec<ScopedUseId, Bindings>,

    /// Tracks whether or not a given AST node is reachable from the start of the scope.
    node_reachability: FxHashMap<NodeKey, ScopedReachabilityConstraintId>,

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
    ///
    /// If we see a binding to a `Final`-qualified symbol, we also need this map to find previous
    /// bindings to that symbol. If there are any, the assignment is invalid.
    bindings_by_definition: FxHashMap<Definition<'db>, Bindings>,

    /// [`PlaceState`] visible at end of scope for each symbol.
    end_of_scope_symbols: IndexVec<ScopedSymbolId, PlaceState>,

    /// [`PlaceState`] visible at end of scope for each member.
    end_of_scope_members: IndexVec<ScopedMemberId, PlaceState>,

    /// All potentially reachable bindings and declarations, for each symbol.
    reachable_definitions_by_symbol: IndexVec<ScopedSymbolId, ReachableDefinitions>,

    /// All potentially reachable bindings and declarations, for each member.
    reachable_definitions_by_member: IndexVec<ScopedMemberId, ReachableDefinitions>,

    /// Snapshot of bindings in this scope that can be used to resolve a reference in a nested
    /// scope.
    enclosing_snapshots: EnclosingSnapshots,

    /// Whether or not the end of the scope is reachable.
    ///
    /// This is used to check if the function can implicitly return `None`.
    /// For example:
    /// ```py
    /// def f(cond: bool) -> int | None:
    ///     if cond:
    ///        return 1
    ///
    /// def g() -> int:
    ///     if True:
    ///        return 1
    /// ```
    ///
    /// Function `f` may implicitly return `None`, but `g` cannot.
    ///
    /// This is used by [`UseDefMap::can_implicitly_return_none`].
    end_of_scope_reachability: ScopedReachabilityConstraintId,
}

pub(crate) enum ApplicableConstraints<'map, 'db> {
    UnboundBinding(ConstraintsIterator<'map, 'db>),
    ConstrainedBindings(BindingWithConstraintsIterator<'map, 'db>),
}

impl<'db> UseDefMap<'db> {
    pub(crate) fn bindings_at_use(
        &self,
        use_id: ScopedUseId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(
            &self.bindings_by_use[use_id],
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub(crate) fn applicable_constraints(
        &self,
        constraint_key: ConstraintKey,
        enclosing_scope: FileScopeId,
        expr: PlaceExprRef,
        index: &'db SemanticIndex,
    ) -> ApplicableConstraints<'_, 'db> {
        match constraint_key {
            ConstraintKey::NarrowingConstraint(constraint) => {
                ApplicableConstraints::UnboundBinding(ConstraintsIterator {
                    predicates: &self.predicates,
                    constraint_ids: self.narrowing_constraints.iter_predicates(constraint),
                })
            }
            ConstraintKey::NestedScope(nested_scope) => {
                let EnclosingSnapshotResult::FoundBindings(bindings) =
                    index.enclosing_snapshot(enclosing_scope, expr, nested_scope)
                else {
                    unreachable!(
                        "The result of `SemanticIndex::eager_snapshot` must be `FoundBindings`"
                    )
                };
                ApplicableConstraints::ConstrainedBindings(bindings)
            }
            ConstraintKey::UseId(use_id) => {
                ApplicableConstraints::ConstrainedBindings(self.bindings_at_use(use_id))
            }
        }
    }

    pub(super) fn is_reachable(
        &self,
        db: &dyn crate::Db,
        reachability: ScopedReachabilityConstraintId,
    ) -> bool {
        self.reachability_constraints
            .evaluate(db, &self.predicates, reachability)
            .may_be_true()
    }

    /// Check whether or not a given expression is reachable from the start of the scope. This
    /// is a local analysis which does not capture the possibility that the entire scope might
    /// be unreachable. Use [`super::SemanticIndex::is_node_reachable`] for the global
    /// analysis.
    #[track_caller]
    pub(super) fn is_node_reachable(&self, db: &dyn crate::Db, node_key: NodeKey) -> bool {
        self
            .reachability_constraints
            .evaluate(
                db,
                &self.predicates,
                *self
                    .node_reachability
                    .get(&node_key)
                    .expect("`is_node_reachable` should only be called on AST nodes with recorded reachability"),
            )
            .may_be_true()
    }

    pub(crate) fn end_of_scope_bindings(
        &self,
        place: ScopedPlaceId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.end_of_scope_symbol_bindings(symbol),
            ScopedPlaceId::Member(member) => self.end_of_scope_member_bindings(member),
        }
    }

    pub(crate) fn end_of_scope_symbol_bindings(
        &self,
        symbol: ScopedSymbolId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(
            self.end_of_scope_symbols[symbol].bindings(),
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub(crate) fn end_of_scope_member_bindings(
        &self,
        member: ScopedMemberId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(
            self.end_of_scope_members[member].bindings(),
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub(crate) fn reachable_bindings(
        &self,
        place: ScopedPlaceId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.reachable_symbol_bindings(symbol),
            ScopedPlaceId::Member(member) => self.reachable_member_bindings(member),
        }
    }

    pub(crate) fn reachable_symbol_bindings(
        &self,
        symbol: ScopedSymbolId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        let bindings = &self.reachable_definitions_by_symbol[symbol].bindings;
        self.bindings_iterator(bindings, BoundnessAnalysis::AssumeBound)
    }

    pub(crate) fn reachable_member_bindings(
        &self,
        symbol: ScopedMemberId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        let bindings = &self.reachable_definitions_by_member[symbol].bindings;
        self.bindings_iterator(bindings, BoundnessAnalysis::AssumeBound)
    }

    pub(crate) fn enclosing_snapshot(
        &self,
        snapshot_id: ScopedEnclosingSnapshotId,
        nested_laziness: ScopeLaziness,
    ) -> EnclosingSnapshotResult<'_, 'db> {
        let boundness_analysis = if nested_laziness.is_eager() {
            BoundnessAnalysis::BasedOnUnboundVisibility
        } else {
            // TODO: We haven't implemented proper boundness analysis for nonlocal symbols, so we assume the boundness is bound for now.
            BoundnessAnalysis::AssumeBound
        };
        match self.enclosing_snapshots.get(snapshot_id) {
            Some(EnclosingSnapshot::Constraint(constraint)) => {
                EnclosingSnapshotResult::FoundConstraint(*constraint)
            }
            Some(EnclosingSnapshot::Bindings(bindings)) => EnclosingSnapshotResult::FoundBindings(
                self.bindings_iterator(bindings, boundness_analysis),
            ),
            None => EnclosingSnapshotResult::NotFound,
        }
    }

    pub(crate) fn bindings_at_definition(
        &self,
        definition: Definition<'db>,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        self.bindings_iterator(
            &self.bindings_by_definition[&definition],
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub(crate) fn declarations_at_binding(
        &self,
        binding: Definition<'db>,
    ) -> DeclarationsIterator<'_, 'db> {
        self.declarations_iterator(
            &self.declarations_by_binding[&binding],
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub(crate) fn end_of_scope_declarations<'map>(
        &'map self,
        place: ScopedPlaceId,
    ) -> DeclarationsIterator<'map, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.end_of_scope_symbol_declarations(symbol),
            ScopedPlaceId::Member(member) => self.end_of_scope_member_declarations(member),
        }
    }

    pub(crate) fn end_of_scope_symbol_declarations<'map>(
        &'map self,
        symbol: ScopedSymbolId,
    ) -> DeclarationsIterator<'map, 'db> {
        let declarations = self.end_of_scope_symbols[symbol].declarations();
        self.declarations_iterator(declarations, BoundnessAnalysis::BasedOnUnboundVisibility)
    }

    pub(crate) fn end_of_scope_member_declarations<'map>(
        &'map self,
        member: ScopedMemberId,
    ) -> DeclarationsIterator<'map, 'db> {
        let declarations = self.end_of_scope_members[member].declarations();
        self.declarations_iterator(declarations, BoundnessAnalysis::BasedOnUnboundVisibility)
    }

    pub(crate) fn reachable_symbol_declarations(
        &self,
        symbol: ScopedSymbolId,
    ) -> DeclarationsIterator<'_, 'db> {
        let declarations = &self.reachable_definitions_by_symbol[symbol].declarations;
        self.declarations_iterator(declarations, BoundnessAnalysis::AssumeBound)
    }

    pub(crate) fn reachable_member_declarations(
        &self,
        member: ScopedMemberId,
    ) -> DeclarationsIterator<'_, 'db> {
        let declarations = &self.reachable_definitions_by_member[member].declarations;
        self.declarations_iterator(declarations, BoundnessAnalysis::AssumeBound)
    }

    pub(crate) fn reachable_declarations(
        &self,
        place: ScopedPlaceId,
    ) -> DeclarationsIterator<'_, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.reachable_symbol_declarations(symbol),
            ScopedPlaceId::Member(member) => self.reachable_member_declarations(member),
        }
    }

    pub(crate) fn all_end_of_scope_symbol_declarations<'map>(
        &'map self,
    ) -> impl Iterator<Item = (ScopedSymbolId, DeclarationsIterator<'map, 'db>)> + 'map {
        self.end_of_scope_symbols
            .indices()
            .map(|symbol_id| (symbol_id, self.end_of_scope_symbol_declarations(symbol_id)))
    }

    pub(crate) fn all_end_of_scope_symbol_bindings<'map>(
        &'map self,
    ) -> impl Iterator<Item = (ScopedSymbolId, BindingWithConstraintsIterator<'map, 'db>)> + 'map
    {
        self.end_of_scope_symbols
            .indices()
            .map(|symbol_id| (symbol_id, self.end_of_scope_symbol_bindings(symbol_id)))
    }

    pub(crate) fn all_reachable_symbols<'map>(
        &'map self,
    ) -> impl Iterator<
        Item = (
            ScopedSymbolId,
            DeclarationsIterator<'map, 'db>,
            BindingWithConstraintsIterator<'map, 'db>,
        ),
    > + 'map {
        self.reachable_definitions_by_symbol.iter_enumerated().map(
            |(symbol_id, reachable_definitions)| {
                let declarations = self.declarations_iterator(
                    &reachable_definitions.declarations,
                    BoundnessAnalysis::AssumeBound,
                );
                let bindings = self.bindings_iterator(
                    &reachable_definitions.bindings,
                    BoundnessAnalysis::AssumeBound,
                );
                (symbol_id, declarations, bindings)
            },
        )
    }

    /// This function is intended to be called only once inside `TypeInferenceBuilder::infer_function_body`.
    pub(crate) fn can_implicitly_return_none(&self, db: &dyn crate::Db) -> bool {
        !self
            .reachability_constraints
            .evaluate(db, &self.predicates, self.end_of_scope_reachability)
            .is_always_false()
    }

    pub(crate) fn binding_reachability(
        &self,
        db: &dyn crate::Db,
        binding: &BindingWithConstraints<'_, 'db>,
    ) -> Truthiness {
        self.reachability_constraints.evaluate(
            db,
            &self.predicates,
            binding.reachability_constraint,
        )
    }

    fn bindings_iterator<'map>(
        &'map self,
        bindings: &'map Bindings,
        boundness_analysis: BoundnessAnalysis,
    ) -> BindingWithConstraintsIterator<'map, 'db> {
        BindingWithConstraintsIterator {
            all_definitions: &self.all_definitions,
            predicates: &self.predicates,
            narrowing_constraints: &self.narrowing_constraints,
            reachability_constraints: &self.reachability_constraints,
            boundness_analysis,
            inner: bindings.iter(),
        }
    }

    fn declarations_iterator<'map>(
        &'map self,
        declarations: &'map Declarations,
        boundness_analysis: BoundnessAnalysis,
    ) -> DeclarationsIterator<'map, 'db> {
        DeclarationsIterator {
            all_definitions: &self.all_definitions,
            predicates: &self.predicates,
            reachability_constraints: &self.reachability_constraints,
            boundness_analysis,
            inner: declarations.iter(),
        }
    }
}

/// Uniquely identifies a snapshot of an enclosing scope place state that can be used to resolve a
/// reference in a nested scope.
///
/// An eager scope has its entire body executed immediately at the location where it is defined.
/// For any free references in the nested scope, we use the bindings that are visible at the point
/// where the nested scope is defined, instead of using the public type of the place.
///
/// There is a unique ID for each distinct [`EnclosingSnapshotKey`] in the file.
#[newtype_index]
#[derive(get_size2::GetSize)]
pub(crate) struct ScopedEnclosingSnapshotId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub(crate) struct EnclosingSnapshotKey {
    /// The enclosing scope containing the bindings
    pub(crate) enclosing_scope: FileScopeId,
    /// The referenced place (in the enclosing scope)
    pub(crate) enclosing_place: ScopedPlaceId,
    /// The nested scope containing the reference
    pub(crate) nested_scope: FileScopeId,
    /// Laziness of the nested scope (technically redundant, but convenient to have here)
    pub(crate) nested_laziness: ScopeLaziness,
}

/// Snapshots of enclosing scope place states for resolving a reference in a nested scope.
/// If the nested scope is eager, the snapshot is simply recorded and used as is.
/// If it is lazy, every time the outer symbol is reassigned, the snapshot is updated to add the
/// new binding.
type EnclosingSnapshots = IndexVec<ScopedEnclosingSnapshotId, EnclosingSnapshot>;

#[derive(Clone, Debug)]
pub(crate) struct BindingWithConstraintsIterator<'map, 'db> {
    all_definitions: &'map IndexVec<ScopedDefinitionId, DefinitionState<'db>>,
    pub(crate) predicates: &'map Predicates<'db>,
    pub(crate) narrowing_constraints: &'map NarrowingConstraints,
    pub(crate) reachability_constraints: &'map ReachabilityConstraints,
    pub(crate) boundness_analysis: BoundnessAnalysis,
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
                reachability_constraint: live_binding.reachability_constraint,
            })
    }
}

impl std::iter::FusedIterator for BindingWithConstraintsIterator<'_, '_> {}

pub(crate) struct BindingWithConstraints<'map, 'db> {
    pub(crate) binding: DefinitionState<'db>,
    pub(crate) narrowing_constraint: ConstraintsIterator<'map, 'db>,
    pub(crate) reachability_constraint: ScopedReachabilityConstraintId,
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
    pub(crate) reachability_constraints: &'map ReachabilityConstraints,
    pub(crate) boundness_analysis: BoundnessAnalysis,
    inner: LiveDeclarationsIterator<'map>,
}

#[derive(Debug)]
pub(crate) struct DeclarationWithConstraint<'db> {
    pub(crate) declaration: DefinitionState<'db>,
    pub(crate) reachability_constraint: ScopedReachabilityConstraintId,
}

impl<'db> Iterator for DeclarationsIterator<'_, 'db> {
    type Item = DeclarationWithConstraint<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(
            |LiveDeclaration {
                 declaration,
                 reachability_constraint,
             }| {
                DeclarationWithConstraint {
                    declaration: self.all_definitions[*declaration],
                    reachability_constraint: *reachability_constraint,
                }
            },
        )
    }
}

impl std::iter::FusedIterator for DeclarationsIterator<'_, '_> {}

#[derive(Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
struct ReachableDefinitions {
    bindings: Bindings,
    declarations: Declarations,
}

/// A snapshot of the definitions and constraints state at a particular point in control flow.
#[derive(Clone, Debug)]
pub(super) struct FlowSnapshot {
    symbol_states: IndexVec<ScopedSymbolId, PlaceState>,
    member_states: IndexVec<ScopedMemberId, PlaceState>,
    reachability: ScopedReachabilityConstraintId,
}

/// A snapshot of the state of a single symbol (e.g. `obj`) and all of its associated members
/// (e.g. `obj.attr`, `obj["key"]`).
pub(super) struct SingleSymbolSnapshot {
    symbol_state: PlaceState,
    associated_member_states: FxHashMap<ScopedMemberId, PlaceState>,
}

#[derive(Debug)]
pub(super) struct UseDefMapBuilder<'db> {
    /// Append-only array of [`DefinitionState`].
    all_definitions: IndexVec<ScopedDefinitionId, DefinitionState<'db>>,

    /// Builder of predicates.
    pub(super) predicates: PredicatesBuilder<'db>,

    /// Builder of narrowing constraints.
    pub(super) narrowing_constraints: NarrowingConstraintsBuilder,

    /// Builder of reachability constraints.
    pub(super) reachability_constraints: ReachabilityConstraintsBuilder,

    /// Live bindings at each so-far-recorded use.
    bindings_by_use: IndexVec<ScopedUseId, Bindings>,

    /// Tracks whether or not the current point in control flow is reachable from the
    /// start of the scope.
    pub(super) reachability: ScopedReachabilityConstraintId,

    /// Tracks whether or not a given AST node is reachable from the start of the scope.
    node_reachability: FxHashMap<NodeKey, ScopedReachabilityConstraintId>,

    /// Live declarations for each so-far-recorded binding.
    declarations_by_binding: FxHashMap<Definition<'db>, Declarations>,

    /// Live bindings for each so-far-recorded definition.
    bindings_by_definition: FxHashMap<Definition<'db>, Bindings>,

    /// Currently live bindings and declarations for each place.
    symbol_states: IndexVec<ScopedSymbolId, PlaceState>,

    member_states: IndexVec<ScopedMemberId, PlaceState>,

    /// All potentially reachable bindings and declarations, for each place.
    reachable_symbol_definitions: IndexVec<ScopedSymbolId, ReachableDefinitions>,

    reachable_member_definitions: IndexVec<ScopedMemberId, ReachableDefinitions>,

    /// Snapshots of place states in this scope that can be used to resolve a reference in a
    /// nested scope.
    enclosing_snapshots: EnclosingSnapshots,

    /// Is this a class scope?
    is_class_scope: bool,
}

impl<'db> UseDefMapBuilder<'db> {
    pub(super) fn new(is_class_scope: bool) -> Self {
        Self {
            all_definitions: IndexVec::from_iter([DefinitionState::Undefined]),
            predicates: PredicatesBuilder::default(),
            narrowing_constraints: NarrowingConstraintsBuilder::default(),
            reachability_constraints: ReachabilityConstraintsBuilder::default(),
            bindings_by_use: IndexVec::new(),
            reachability: ScopedReachabilityConstraintId::ALWAYS_TRUE,
            node_reachability: FxHashMap::default(),
            declarations_by_binding: FxHashMap::default(),
            bindings_by_definition: FxHashMap::default(),
            symbol_states: IndexVec::new(),
            member_states: IndexVec::new(),
            reachable_member_definitions: IndexVec::new(),
            reachable_symbol_definitions: IndexVec::new(),
            enclosing_snapshots: EnclosingSnapshots::default(),
            is_class_scope,
        }
    }

    pub(super) fn mark_unreachable(&mut self) {
        self.reachability = ScopedReachabilityConstraintId::ALWAYS_FALSE;

        for state in &mut self.symbol_states {
            state.record_reachability_constraint(
                &mut self.reachability_constraints,
                ScopedReachabilityConstraintId::ALWAYS_FALSE,
            );
        }

        for state in &mut self.member_states {
            state.record_reachability_constraint(
                &mut self.reachability_constraints,
                ScopedReachabilityConstraintId::ALWAYS_FALSE,
            );
        }
    }

    pub(super) fn add_place(&mut self, place: ScopedPlaceId) {
        match place {
            ScopedPlaceId::Symbol(symbol) => {
                let new_place = self
                    .symbol_states
                    .push(PlaceState::undefined(self.reachability));
                debug_assert_eq!(symbol, new_place);
                let new_place = self
                    .reachable_symbol_definitions
                    .push(ReachableDefinitions {
                        bindings: Bindings::unbound(self.reachability),
                        declarations: Declarations::undeclared(self.reachability),
                    });
                debug_assert_eq!(symbol, new_place);
            }
            ScopedPlaceId::Member(member) => {
                let new_place = self
                    .member_states
                    .push(PlaceState::undefined(self.reachability));
                debug_assert_eq!(member, new_place);
                let new_place = self
                    .reachable_member_definitions
                    .push(ReachableDefinitions {
                        bindings: Bindings::unbound(self.reachability),
                        declarations: Declarations::undeclared(self.reachability),
                    });
                debug_assert_eq!(member, new_place);
            }
        }
    }

    pub(super) fn record_binding(&mut self, place: ScopedPlaceId, binding: Definition<'db>) {
        let bindings = match place {
            ScopedPlaceId::Symbol(symbol) => self.symbol_states[symbol].bindings(),
            ScopedPlaceId::Member(member) => self.member_states[member].bindings(),
        };

        self.bindings_by_definition
            .insert(binding, bindings.clone());

        let def_id = self.all_definitions.push(DefinitionState::Defined(binding));
        let place_state = match place {
            ScopedPlaceId::Symbol(symbol) => &mut self.symbol_states[symbol],
            ScopedPlaceId::Member(member) => &mut self.member_states[member],
        };
        self.declarations_by_binding
            .insert(binding, place_state.declarations().clone());
        place_state.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
        );

        let bindings = match place {
            ScopedPlaceId::Symbol(symbol) => {
                &mut self.reachable_symbol_definitions[symbol].bindings
            }
            ScopedPlaceId::Member(member) => {
                &mut self.reachable_member_definitions[member].bindings
            }
        };

        bindings.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
            PreviousDefinitions::AreKept,
        );
    }

    pub(super) fn add_predicate(
        &mut self,
        predicate: PredicateOrLiteral<'db>,
    ) -> ScopedPredicateId {
        match predicate {
            PredicateOrLiteral::Predicate(predicate) => self.predicates.add_predicate(predicate),
            PredicateOrLiteral::Literal(true) => ScopedPredicateId::ALWAYS_TRUE,
            PredicateOrLiteral::Literal(false) => ScopedPredicateId::ALWAYS_FALSE,
        }
    }

    pub(super) fn record_narrowing_constraint(&mut self, predicate: ScopedPredicateId) {
        if predicate == ScopedPredicateId::ALWAYS_TRUE
            || predicate == ScopedPredicateId::ALWAYS_FALSE
        {
            // No need to record a narrowing constraint for `True` or `False`.
            return;
        }

        let narrowing_constraint = predicate.into();
        for state in &mut self.symbol_states {
            state
                .record_narrowing_constraint(&mut self.narrowing_constraints, narrowing_constraint);
        }

        for state in &mut self.member_states {
            state
                .record_narrowing_constraint(&mut self.narrowing_constraints, narrowing_constraint);
        }
    }

    /// Snapshot the state of a single symbol and all of its associated members, at the current
    /// point in control flow.
    ///
    /// This is only used for `*`-import reachability constraints, which are handled differently
    /// to most other reachability constraints. See the doc-comment for
    /// [`Self::record_and_negate_star_import_reachability_constraint`] for more details.
    pub(super) fn single_symbol_snapshot(
        &self,
        symbol: ScopedSymbolId,
        associated_member_ids: &[ScopedMemberId],
    ) -> SingleSymbolSnapshot {
        let symbol_state = self.symbol_states[symbol].clone();
        let mut associated_member_states = FxHashMap::default();
        for &member_id in associated_member_ids {
            associated_member_states.insert(member_id, self.member_states[member_id].clone());
        }
        SingleSymbolSnapshot {
            symbol_state,
            associated_member_states,
        }
    }

    /// This method exists solely for handling `*`-import reachability constraints.
    ///
    /// The reason why we add reachability constraints for [`Definition`]s created by `*` imports
    /// is laid out in the doc-comment for `StarImportPlaceholderPredicate`. But treating these
    /// reachability constraints in the use-def map the same way as all other reachability constraints
    /// was shown to lead to [significant regressions] for small codebases where typeshed
    /// dominates. (Although `*` imports are not common generally, they are used in several
    /// important places by typeshed.)
    ///
    /// To solve these regressions, it was observed that we could do significantly less work for
    /// `*`-import definitions. We do a number of things differently here to our normal handling of
    /// reachability constraints:
    ///
    /// - We only apply and negate the reachability constraints to a single symbol, rather than to
    ///   all symbols. This is possible here because, unlike most definitions, we know in advance that
    ///   exactly one definition occurs inside the "if-true" predicate branch, and we know exactly
    ///   which definition it is.
    ///
    /// - We only snapshot the state for a single place prior to the definition, rather than doing
    ///   expensive calls to [`Self::snapshot`]. Again, this is possible because we know
    ///   that only a single definition occurs inside the "if-predicate-true" predicate branch.
    ///
    /// - Normally we take care to check whether an "if-predicate-true" branch or an
    ///   "if-predicate-false" branch contains a terminal statement: these can affect the reachability
    ///   of symbols defined inside either branch. However, in the case of `*`-import definitions,
    ///   this is unnecessary (and therefore not done in this method), since we know that a `*`-import
    ///   predicate cannot create a terminal statement inside either branch.
    ///
    /// [significant regressions]: https://github.com/astral-sh/ruff/pull/17286#issuecomment-2786755746
    pub(super) fn record_and_negate_star_import_reachability_constraint(
        &mut self,
        reachability_id: ScopedReachabilityConstraintId,
        symbol: ScopedSymbolId,
        pre_definition: SingleSymbolSnapshot,
    ) {
        let negated_reachability_id = self
            .reachability_constraints
            .add_not_constraint(reachability_id);

        let mut post_definition_state =
            std::mem::replace(&mut self.symbol_states[symbol], pre_definition.symbol_state);

        post_definition_state
            .record_reachability_constraint(&mut self.reachability_constraints, reachability_id);

        self.symbol_states[symbol].record_reachability_constraint(
            &mut self.reachability_constraints,
            negated_reachability_id,
        );

        self.symbol_states[symbol].merge(
            post_definition_state,
            &mut self.narrowing_constraints,
            &mut self.reachability_constraints,
        );

        // And similarly for all associated members:
        for (member_id, pre_definition_member_state) in pre_definition.associated_member_states {
            let mut post_definition_state = std::mem::replace(
                &mut self.member_states[member_id],
                pre_definition_member_state,
            );

            post_definition_state.record_reachability_constraint(
                &mut self.reachability_constraints,
                reachability_id,
            );

            self.member_states[member_id].record_reachability_constraint(
                &mut self.reachability_constraints,
                negated_reachability_id,
            );

            self.member_states[member_id].merge(
                post_definition_state,
                &mut self.narrowing_constraints,
                &mut self.reachability_constraints,
            );
        }
    }

    pub(super) fn record_reachability_constraint(
        &mut self,
        constraint: ScopedReachabilityConstraintId,
    ) {
        self.reachability = self
            .reachability_constraints
            .add_and_constraint(self.reachability, constraint);

        for state in &mut self.symbol_states {
            state.record_reachability_constraint(&mut self.reachability_constraints, constraint);
        }

        for state in &mut self.member_states {
            state.record_reachability_constraint(&mut self.reachability_constraints, constraint);
        }
    }

    pub(super) fn record_declaration(
        &mut self,
        place: ScopedPlaceId,
        declaration: Definition<'db>,
    ) {
        let def_id = self
            .all_definitions
            .push(DefinitionState::Defined(declaration));

        let place_state = match place {
            ScopedPlaceId::Symbol(symbol) => &mut self.symbol_states[symbol],
            ScopedPlaceId::Member(member) => &mut self.member_states[member],
        };

        self.bindings_by_definition
            .insert(declaration, place_state.bindings().clone());
        place_state.record_declaration(def_id, self.reachability);

        let definitions = match place {
            ScopedPlaceId::Symbol(symbol) => &mut self.reachable_symbol_definitions[symbol],
            ScopedPlaceId::Member(member) => &mut self.reachable_member_definitions[member],
        };

        definitions.declarations.record_declaration(
            def_id,
            self.reachability,
            PreviousDefinitions::AreKept,
        );
    }

    pub(super) fn record_declaration_and_binding(
        &mut self,
        place: ScopedPlaceId,
        definition: Definition<'db>,
    ) {
        // We don't need to store anything in self.bindings_by_declaration or
        // self.declarations_by_binding.
        let def_id = self
            .all_definitions
            .push(DefinitionState::Defined(definition));
        let place_state = match place {
            ScopedPlaceId::Symbol(symbol) => &mut self.symbol_states[symbol],
            ScopedPlaceId::Member(member) => &mut self.member_states[member],
        };
        place_state.record_declaration(def_id, self.reachability);
        place_state.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
        );

        let reachable_definitions = match place {
            ScopedPlaceId::Symbol(symbol) => &mut self.reachable_symbol_definitions[symbol],
            ScopedPlaceId::Member(member) => &mut self.reachable_member_definitions[member],
        };

        reachable_definitions.declarations.record_declaration(
            def_id,
            self.reachability,
            PreviousDefinitions::AreKept,
        );
        reachable_definitions.bindings.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
            PreviousDefinitions::AreKept,
        );
    }

    pub(super) fn delete_binding(&mut self, place: ScopedPlaceId) {
        let def_id = self.all_definitions.push(DefinitionState::Deleted);
        let place_state = match place {
            ScopedPlaceId::Symbol(symbol) => &mut self.symbol_states[symbol],
            ScopedPlaceId::Member(member) => &mut self.member_states[member],
        };

        place_state.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
        );
    }

    pub(super) fn record_use(
        &mut self,
        place: ScopedPlaceId,
        use_id: ScopedUseId,
        node_key: NodeKey,
    ) {
        let bindings = match place {
            ScopedPlaceId::Symbol(symbol) => &mut self.symbol_states[symbol].bindings(),
            ScopedPlaceId::Member(member) => &mut self.member_states[member].bindings(),
        };
        // We have a use of a place; clone the current bindings for that place, and record them
        // as the live bindings for this use.
        let new_use = self.bindings_by_use.push(bindings.clone());
        debug_assert_eq!(use_id, new_use);

        // Track reachability of all uses of places to silence `unresolved-reference`
        // diagnostics in unreachable code.
        self.record_node_reachability(node_key);
    }

    pub(super) fn record_node_reachability(&mut self, node_key: NodeKey) {
        self.node_reachability.insert(node_key, self.reachability);
    }

    pub(super) fn snapshot_enclosing_state(
        &mut self,
        enclosing_place: ScopedPlaceId,
        enclosing_scope: ScopeKind,
        enclosing_place_expr: PlaceExprRef,
        is_parent_of_annotation_scope: bool,
    ) -> ScopedEnclosingSnapshotId {
        let bindings = match enclosing_place {
            ScopedPlaceId::Symbol(symbol) => self.symbol_states[symbol].bindings(),
            ScopedPlaceId::Member(member) => self.member_states[member].bindings(),
        };

        let is_class_symbol = enclosing_scope.is_class() && enclosing_place.is_symbol();
        // Names bound in class scopes are never visible to nested scopes (but
        // attributes/subscripts are visible), so we never need to save eager scope bindings in a
        // class scope. There is one exception to this rule: annotation scopes can see names
        // defined in an immediately-enclosing class scope.
        if (is_class_symbol && !is_parent_of_annotation_scope) || !enclosing_place_expr.is_bound() {
            self.enclosing_snapshots.push(EnclosingSnapshot::Constraint(
                bindings.unbound_narrowing_constraint(),
            ))
        } else {
            self.enclosing_snapshots
                .push(EnclosingSnapshot::Bindings(bindings.clone()))
        }
    }

    pub(super) fn update_enclosing_snapshot(
        &mut self,
        snapshot_id: ScopedEnclosingSnapshotId,
        enclosing_symbol: ScopedSymbolId,
    ) {
        match self.enclosing_snapshots.get_mut(snapshot_id) {
            Some(EnclosingSnapshot::Bindings(bindings)) => {
                let new_symbol_state = &self.symbol_states[enclosing_symbol];
                bindings.merge(
                    new_symbol_state.bindings().clone(),
                    &mut self.narrowing_constraints,
                    &mut self.reachability_constraints,
                );
            }
            Some(EnclosingSnapshot::Constraint(constraint)) => {
                *constraint = ScopedNarrowingConstraint::empty();
            }
            None => {}
        }
    }

    /// Take a snapshot of the current visible-places state.
    pub(super) fn snapshot(&self) -> FlowSnapshot {
        FlowSnapshot {
            symbol_states: self.symbol_states.clone(),
            member_states: self.member_states.clone(),
            reachability: self.reachability,
        }
    }

    /// Restore the current builder places state to the given snapshot.
    pub(super) fn restore(&mut self, snapshot: FlowSnapshot) {
        // We never remove places from `place_states` (it's an IndexVec, and the place
        // IDs must line up), so the current number of known places must always be equal to or
        // greater than the number of known places in a previously-taken snapshot.
        let num_symbols = self.symbol_states.len();
        let num_members = self.member_states.len();
        debug_assert!(num_symbols >= snapshot.symbol_states.len());

        // Restore the current visible-definitions state to the given snapshot.
        self.symbol_states = snapshot.symbol_states;
        self.member_states = snapshot.member_states;
        self.reachability = snapshot.reachability;

        // If the snapshot we are restoring is missing some places we've recorded since, we need
        // to fill them in so the place IDs continue to line up. Since they don't exist in the
        // snapshot, the correct state to fill them in with is "undefined".
        self.symbol_states
            .resize(num_symbols, PlaceState::undefined(self.reachability));

        self.member_states
            .resize(num_members, PlaceState::undefined(self.reachability));
    }

    /// Merge the given snapshot into the current state, reflecting that we might have taken either
    /// path to get here. The new state for each place should include definitions from both the
    /// prior state and the snapshot.
    pub(super) fn merge(&mut self, snapshot: FlowSnapshot) {
        // As an optimization, if we know statically that either of the snapshots is always
        // unreachable, we can leave it out of the merged result entirely. Note that we cannot
        // perform any type inference at this point, so this is largely limited to unreachability
        // via terminal statements. If a flow's reachability depends on an expression in the code,
        // we will include the flow in the merged result; the reachability constraints of its
        // bindings will include this reachability condition, so that later during type inference,
        // we can determine whether any particular binding is non-visible due to unreachability.
        if snapshot.reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
            return;
        }
        if self.reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
            self.restore(snapshot);
            return;
        }

        // We never remove places from `place_states` (it's an IndexVec, and the place
        // IDs must line up), so the current number of known places must always be equal to or
        // greater than the number of known places in a previously-taken snapshot.
        debug_assert!(self.symbol_states.len() >= snapshot.symbol_states.len());
        debug_assert!(self.member_states.len() >= snapshot.member_states.len());

        let mut snapshot_definitions_iter = snapshot.symbol_states.into_iter();
        for current in &mut self.symbol_states {
            if let Some(snapshot) = snapshot_definitions_iter.next() {
                current.merge(
                    snapshot,
                    &mut self.narrowing_constraints,
                    &mut self.reachability_constraints,
                );
            } else {
                current.merge(
                    PlaceState::undefined(snapshot.reachability),
                    &mut self.narrowing_constraints,
                    &mut self.reachability_constraints,
                );
                // Place not present in snapshot, so it's unbound/undeclared from that path.
            }
        }

        let mut snapshot_definitions_iter = snapshot.member_states.into_iter();
        for current in &mut self.member_states {
            if let Some(snapshot) = snapshot_definitions_iter.next() {
                current.merge(
                    snapshot,
                    &mut self.narrowing_constraints,
                    &mut self.reachability_constraints,
                );
            } else {
                current.merge(
                    PlaceState::undefined(snapshot.reachability),
                    &mut self.narrowing_constraints,
                    &mut self.reachability_constraints,
                );
                // Place not present in snapshot, so it's unbound/undeclared from that path.
            }
        }

        self.reachability = self
            .reachability_constraints
            .add_or_constraint(self.reachability, snapshot.reachability);
    }

    fn mark_reachability_constraints(&mut self) {
        // We only walk the fields that are copied through to the UseDefMap when we finish building
        // it.
        for bindings in &mut self.bindings_by_use {
            bindings.finish(&mut self.reachability_constraints);
        }
        for constraint in self.node_reachability.values() {
            self.reachability_constraints.mark_used(*constraint);
        }
        for symbol_state in &mut self.symbol_states {
            symbol_state.finish(&mut self.reachability_constraints);
        }
        for member_state in &mut self.member_states {
            member_state.finish(&mut self.reachability_constraints);
        }
        for reachable_definition in &mut self.reachable_symbol_definitions {
            reachable_definition
                .bindings
                .finish(&mut self.reachability_constraints);
            reachable_definition
                .declarations
                .finish(&mut self.reachability_constraints);
        }
        for reachable_definition in &mut self.reachable_member_definitions {
            reachable_definition
                .bindings
                .finish(&mut self.reachability_constraints);
            reachable_definition
                .declarations
                .finish(&mut self.reachability_constraints);
        }
        for declarations in self.declarations_by_binding.values_mut() {
            declarations.finish(&mut self.reachability_constraints);
        }
        for bindings in self.bindings_by_definition.values_mut() {
            bindings.finish(&mut self.reachability_constraints);
        }
        for eager_snapshot in &mut self.enclosing_snapshots {
            eager_snapshot.finish(&mut self.reachability_constraints);
        }
        self.reachability_constraints.mark_used(self.reachability);
    }

    pub(super) fn finish(mut self) -> UseDefMap<'db> {
        self.mark_reachability_constraints();

        self.all_definitions.shrink_to_fit();
        self.symbol_states.shrink_to_fit();
        self.member_states.shrink_to_fit();
        self.reachable_symbol_definitions.shrink_to_fit();
        self.reachable_member_definitions.shrink_to_fit();
        self.bindings_by_use.shrink_to_fit();
        self.node_reachability.shrink_to_fit();
        self.declarations_by_binding.shrink_to_fit();
        self.bindings_by_definition.shrink_to_fit();
        self.enclosing_snapshots.shrink_to_fit();

        UseDefMap {
            all_definitions: self.all_definitions,
            predicates: self.predicates.build(),
            narrowing_constraints: self.narrowing_constraints.build(),
            reachability_constraints: self.reachability_constraints.build(),
            bindings_by_use: self.bindings_by_use,
            node_reachability: self.node_reachability,
            end_of_scope_symbols: self.symbol_states,
            end_of_scope_members: self.member_states,
            reachable_definitions_by_symbol: self.reachable_symbol_definitions,
            reachable_definitions_by_member: self.reachable_member_definitions,
            declarations_by_binding: self.declarations_by_binding,
            bindings_by_definition: self.bindings_by_definition,
            enclosing_snapshots: self.enclosing_snapshots,
            end_of_scope_reachability: self.reachability,
        }
    }
}
