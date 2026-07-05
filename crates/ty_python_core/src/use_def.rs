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
//! binding(s) can reach that use. In [`crate::ast_ids::AstIds`] we number
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
//! `bindings_by_use` vector of [`InternedBindingsId`] indexed by [`ScopedUseId`]
//! (plus an interned bindings table), a
//! `definitions_by_definition` map of [`DefinitionsAtDefinition`], and `symbol_states` and
//! `member_states` vectors indexed by [`ScopedSymbolId`]/[`ScopedMemberId`]. The values are (in
//! principle) a list of live bindings at that use/definition, or at the end of the scope for that
//! place, with a list of the dominating constraints for each binding.
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
//! "unbound/undeclared" definition at logical index zero. If that sentinel definition is present
//! in the live bindings at a given use, it means that there is a possible path through control
//! flow in which that place is unbound. Similarly, if that sentinel is present in the live
//! declarations, it means that the place is (possibly) undeclared.
//!
//! To build a [`UseDefMap`], the [`UseDefMapBuilder`] is notified of each new use, definition, and
//! constraint as they are encountered by the
//! [`crate::builder::SemanticIndexBuilder`] AST visit. For
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
//! [`SemanticIndexBuilder`](crate::builder::SemanticIndexBuilder), e.g. where it
//! visits a `StmtIf` node.

use std::collections::hash_map::Entry;
use std::hash::{Hash as _, Hasher as _};
use std::ops::Index;
use std::rc::Rc;
use std::sync::LazyLock;

use ruff_index::{FrozenIndexVec, Idx, IndexVec, newtype_index};
use ruff_text_size::TextRange;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use smallvec::SmallVec;
use thin_vec::ThinVec;

use crate::ast_ids::ScopedUseId;
use crate::definition::{Definition, DefinitionState};
use crate::frozen::FrozenMap;
use crate::member::ScopedMemberId;
use crate::narrowing_constraints::{
    ConstraintKey, NarrowingConstraints, NarrowingConstraintsBuilder, ScopedNarrowingConstraint,
};
use crate::place::{PlaceExprRef, ScopedPlaceId};
use crate::predicate::{PredicateOrLiteral, Predicates, PredicatesBuilder, ScopedPredicateId};
use crate::reachability_constraints::{
    ReachabilityConstraints, ReachabilityConstraintsBuilder, ScopedReachabilityConstraintId,
};
use crate::scope::{FileScopeId, ScopeKind, ScopeLaziness};
use crate::symbol::ScopedSymbolId;
use crate::use_def::place_state::{
    Bindings, Declarations, EnclosingSnapshot, LiveBindingsIterator, LiveDeclaration,
    LiveDeclarationsIterator, PlaceState,
};
use crate::{
    BoundnessAnalysis, EnclosingSnapshotResult, LoopHeader, PossiblyNarrowedPlaces, SemanticIndex,
};

mod place_state;

pub use place_state::LiveBinding;
pub use place_state::ScopedDefinitionId;
pub(super) use place_state::{FutureDefinitions, PreviousDefinitions};

/// Identifies a [`LoopHeader`] within a single scope's [`UseDefMap`].
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct LoopHeaderId;

/// Uniquely identifies an interned [`Bindings`] entry in [`UseDefMap::interned_bindings`].
#[newtype_index]
#[derive(get_size2::GetSize, salsa::SalsaValue)]
struct InternedBindingsId;

/// Uniquely identifies an interned [`Declarations`] entry in [`UseDefMap::interned_declarations`].
#[newtype_index]
#[derive(get_size2::GetSize, salsa::SalsaValue)]
struct InternedDeclarationsId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
struct InternedPlaceStateId(InternedBindingsId, InternedDeclarationsId);

impl InternedPlaceStateId {
    fn bindings_id(self) -> InternedBindingsId {
        self.0
    }

    fn declarations_id(self) -> InternedDeclarationsId {
        self.1
    }
}

struct PlaceStateInterner {
    interned_bindings: RetainedBindingsBuilder,
    interned_ids_by_bindings: hashbrown::HashTable<InternedBindingsId>,
    interned_declarations: RetainedDeclarationsBuilder,
    interned_ids_by_declarations: FxHashMap<Declarations, InternedDeclarationsId>,
    // Undeclared states are common and can be interned by their dense constraint IDs.
    undeclared_declarations_by_constraint:
        IndexVec<ScopedReachabilityConstraintId, Option<InternedDeclarationsId>>,
    // These values are extremely common, so avoid repeatedly hashing their small vectors.
    always_unbound_bindings: Option<InternedBindingsId>,
    always_undeclared_declarations: Option<InternedDeclarationsId>,
}

impl PlaceStateInterner {
    fn with_capacity(bindings: usize, declaration_map: usize, declarations: usize) -> Self {
        Self {
            interned_bindings: RetainedBindingsBuilder::with_capacity(bindings),
            interned_ids_by_bindings: hashbrown::HashTable::with_capacity(bindings),
            interned_declarations: RetainedDeclarationsBuilder::with_capacity(declarations),
            interned_ids_by_declarations: FxHashMap::with_capacity_and_hasher(
                declaration_map,
                FxBuildHasher,
            ),
            undeclared_declarations_by_constraint: IndexVec::new(),
            always_unbound_bindings: None,
            always_undeclared_declarations: None,
        }
    }

    fn intern_bindings(&mut self, bindings: &Bindings) -> InternedBindingsId {
        if bindings.is_always_unbound() {
            if let Some(interned_id) = self.always_unbound_bindings {
                return interned_id;
            }

            let interned_id = self.interned_bindings.push(bindings);
            self.always_unbound_bindings = Some(interned_id);
            return interned_id;
        }

        // The retained representation discards the unbound narrowing constraint, so it isn't
        // part of the interned identity.
        let hash = Self::hash_bindings(bindings.as_slice());
        let interned_bindings = &mut self.interned_bindings;
        let entry = self.interned_ids_by_bindings.entry(
            hash,
            |id| interned_bindings.get(*id) == bindings.as_slice(),
            |id| Self::hash_bindings(interned_bindings.get(*id)),
        );
        match entry {
            hashbrown::hash_table::Entry::Occupied(entry) => *entry.get(),
            hashbrown::hash_table::Entry::Vacant(entry) => {
                let interned_id = interned_bindings.push(bindings);
                entry.insert(interned_id);
                interned_id
            }
        }
    }

    fn hash_bindings(live_bindings: &[LiveBinding]) -> u64 {
        let mut hasher = FxHasher::default();
        live_bindings.hash(&mut hasher);
        hasher.finish()
    }

    fn intern_declarations(&mut self, declarations: Declarations) -> InternedDeclarationsId {
        if declarations.is_always_undeclared() {
            if let Some(interned_id) = self.always_undeclared_declarations {
                return interned_id;
            }

            let interned_id = self.interned_declarations.push(&declarations);
            self.always_undeclared_declarations = Some(interned_id);
            return interned_id;
        }

        if let Some(reachability_constraint) = declarations.undeclared_reachability_constraint()
            && !reachability_constraint.is_terminal()
        {
            let index = reachability_constraint.index();
            let len = self.undeclared_declarations_by_constraint.len();
            if index >= len {
                self.undeclared_declarations_by_constraint
                    .resize(index + 1, None);
            } else if let Some(interned_id) =
                self.undeclared_declarations_by_constraint[reachability_constraint]
            {
                return interned_id;
            }

            let interned_id = self.interned_declarations.push(&declarations);
            self.undeclared_declarations_by_constraint[reachability_constraint] = Some(interned_id);
            return interned_id;
        }

        match self.interned_ids_by_declarations.entry(declarations) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let interned_id = self.interned_declarations.push(entry.key());
                entry.insert(interned_id);
                interned_id
            }
        }
    }

    fn intern_place_state(
        &mut self,
        bindings: &Bindings,
        declarations: Declarations,
    ) -> InternedPlaceStateId {
        InternedPlaceStateId(
            self.intern_bindings(bindings),
            self.intern_declarations(declarations),
        )
    }

    fn retain_place_state(
        &mut self,
        bindings: &Bindings,
        declarations: Declarations,
    ) -> InternedPlaceStateId {
        // Other retained declarations rarely repeat. Keep the compact IDs without hashing every
        // declaration vector to find the occasional duplicate.
        let declarations_id = if declarations.undeclared_reachability_constraint().is_some() {
            self.intern_declarations(declarations)
        } else {
            self.interned_declarations.push(&declarations)
        };
        InternedPlaceStateId(self.intern_bindings(bindings), declarations_id)
    }
}

/// Compact, retained representation of the interned binding vectors for a scope.
///
/// The builder needs a `SmallVec` and an optional unbound constraint while constructing each
/// binding state. Neither is needed after the semantic index is built, so the retained map stores
/// cumulative end offsets into one contiguous array instead.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct RetainedBindings {
    ends: FrozenIndexVec<InternedBindingsId, u32>,
    live_bindings: Box<[LiveBinding]>,
}

struct RetainedBindingsBuilder {
    ends: IndexVec<InternedBindingsId, u32>,
    live_bindings: Vec<LiveBinding>,
}

impl RetainedBindingsBuilder {
    fn with_capacity(bindings: usize) -> Self {
        Self {
            ends: IndexVec::with_capacity(bindings),
            live_bindings: Vec::with_capacity(bindings),
        }
    }

    fn push(&mut self, bindings: &Bindings) -> InternedBindingsId {
        // Definition IDs are also 32-bit and a single scope cannot practically approach this
        // limit. Keeping one cumulative end offset per state halves the retained range metadata.
        self.live_bindings.extend_from_slice(bindings.as_slice());
        let end = u32::try_from(self.live_bindings.len())
            .expect("Expected live-bindings length to fit into a u32");
        self.ends.push(end)
    }

    fn get(&self, index: InternedBindingsId) -> &[LiveBinding] {
        let end = self.ends[index];
        let start = if index.index() == 0 {
            0
        } else {
            self.ends[InternedBindingsId::new(index.index() - 1)]
        };
        &self.live_bindings[start as usize..end as usize]
    }

    fn finish(
        self,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) -> RetainedBindings {
        for binding in &self.live_bindings {
            reachability_constraints.mark_used(binding.reachability_constraint());
            narrowing_constraints.mark_used(binding.narrowing_constraint());
        }
        RetainedBindings {
            ends: self.ends.into(),
            live_bindings: self.live_bindings.into_boxed_slice(),
        }
    }
}

impl Index<InternedBindingsId> for RetainedBindings {
    type Output = [LiveBinding];

    fn index(&self, index: InternedBindingsId) -> &Self::Output {
        let end = self.ends[index];
        let start = if index.index() == 0 {
            0
        } else {
            self.ends[InternedBindingsId::new(index.index() - 1)]
        };
        &self.live_bindings[start as usize..end as usize]
    }
}

/// Compact, retained representation of the interned declaration vectors for a scope.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct RetainedDeclarations {
    /// The exclusive end of each state in `live_declarations`; its start is the previous end.
    ends: FrozenIndexVec<InternedDeclarationsId, u32>,
    live_declarations: Box<[LiveDeclaration]>,
}

struct RetainedDeclarationsBuilder {
    ends: IndexVec<InternedDeclarationsId, u32>,
    live_declarations: Vec<LiveDeclaration>,
}

impl RetainedDeclarationsBuilder {
    fn with_capacity(declarations: usize) -> Self {
        Self {
            ends: IndexVec::with_capacity(declarations),
            live_declarations: Vec::with_capacity(declarations),
        }
    }

    fn push(&mut self, declarations: &Declarations) -> InternedDeclarationsId {
        self.live_declarations.extend(declarations.iter().cloned());
        let end = u32::try_from(self.live_declarations.len())
            .expect("Expected live-declarations length to fit into a u32");
        self.ends.push(end)
    }

    fn finish(
        self,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) -> RetainedDeclarations {
        for declaration in &self.live_declarations {
            reachability_constraints.mark_used(declaration.reachability_constraint);
        }
        RetainedDeclarations {
            ends: self.ends.into(),
            live_declarations: self.live_declarations.into_boxed_slice(),
        }
    }
}

impl Index<InternedDeclarationsId> for RetainedDeclarations {
    type Output = [LiveDeclaration];

    fn index(&self, index: InternedDeclarationsId) -> &Self::Output {
        let end = self.ends[index];
        let start = if index.index() == 0 {
            0
        } else {
            self.ends[InternedDeclarationsId::new(index.index() - 1)]
        };
        &self.live_declarations[start as usize..end as usize]
    }
}

#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
struct RetainedPlaceStates<T> {
    end_of_scope: T,
    reachable: T,
}

#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct DefinitionsAtDefinition<B, D> {
    bindings: B,
    declarations: Option<D>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
enum InternedEnclosingSnapshotId {
    Constraint(ScopedNarrowingConstraint),
    Bindings(InternedBindingsId),
}

/// Lookup tables needed to evaluate reachability and narrowing constraints.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct ConstraintTables<'db> {
    predicates: Predicates<'db>,
    reachability_constraints: ReachabilityConstraints,
    narrowing_constraints: NarrowingConstraints,
}

/// Fields that are empty in most use-def maps.
///
/// These fields share an allocation to avoid storing five collection headers in every
/// [`UseDefMap`]. They are not otherwise semantically related.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct UseDefMapExtra {
    /// [`Bindings`] reaching a [`ScopedUseId`].
    bindings_by_use: FrozenIndexVec<ScopedUseId, InternedBindingsId>,

    /// [`Bindings`] for each member reaching a [`ScopedUseId`].
    ///
    /// This is only used for kwargs expressions, whose corresponding `bindings_by_use` entry
    /// is empty.
    multi_bindings_by_use: MultiBindingsByUse,

    /// Retained [`PlaceState`] values for each member.
    member_states: FrozenIndexVec<ScopedMemberId, RetainedPlaceStates<InternedPlaceStateId>>,

    /// Snapshots of bindings used to resolve references from nested scopes.
    enclosing_snapshots: FrozenIndexVec<ScopedEnclosingSnapshotId, InternedEnclosingSnapshotId>,

    /// Completed loop headers in this scope.
    loop_headers: FrozenIndexVec<LoopHeaderId, LoopHeader>,
}

static EMPTY_CONSTRAINT_TABLES: LazyLock<ConstraintTables<'static>> =
    LazyLock::new(|| ConstraintTables {
        predicates: IndexVec::new().into(),
        reachability_constraints: ReachabilityConstraintsBuilder::default().build(),
        narrowing_constraints: NarrowingConstraintsBuilder::default().build(),
    });

static ALWAYS_UNBOUND_BINDINGS: LazyLock<Bindings> =
    LazyLock::new(|| Bindings::unbound(ScopedReachabilityConstraintId::ALWAYS_TRUE));

static ALWAYS_UNDECLARED_DECLARATIONS: LazyLock<Declarations> =
    LazyLock::new(|| Declarations::undeclared(ScopedReachabilityConstraintId::ALWAYS_TRUE));

#[derive(Clone, Copy, Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct DefinitionUsage {
    is_used: bool,
    is_multipart_import_used: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
enum RetainedDefinitionState<'db> {
    Defined(Definition<'db>, DefinitionUsage),
    Undefined,
    Deleted,
}

impl<'db> RetainedDefinitionState<'db> {
    fn new(state: DefinitionState<'db>, used: bool, multipart_import_used: bool) -> Self {
        match state {
            DefinitionState::Defined(definition) => Self::Defined(
                definition,
                DefinitionUsage {
                    is_used: used,
                    is_multipart_import_used: multipart_import_used,
                },
            ),
            DefinitionState::Undefined => {
                debug_assert!(!used);
                debug_assert!(!multipart_import_used);
                Self::Undefined
            }
            DefinitionState::Deleted => {
                debug_assert!(!used);
                debug_assert!(!multipart_import_used);
                Self::Deleted
            }
        }
    }

    fn state(self) -> DefinitionState<'db> {
        match self {
            Self::Defined(definition, _) => DefinitionState::Defined(definition),
            Self::Undefined => DefinitionState::Undefined,
            Self::Deleted => DefinitionState::Deleted,
        }
    }

    fn is_used(self) -> bool {
        matches!(self, Self::Defined(_, usage) if usage.is_used)
    }

    fn is_multipart_import_used(self) -> bool {
        matches!(self, Self::Defined(_, usage) if usage.is_multipart_import_used)
    }
}

static_assertions::assert_eq_size!(RetainedDefinitionState<'static>, DefinitionState<'static>);

/// Retained definition states, excluding the implicit unbound definition at index zero.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct RetainedDefinitions<'db> {
    states: Box<[RetainedDefinitionState<'db>]>,
}

impl<'db> RetainedDefinitions<'db> {
    fn new(
        states: IndexVec<ScopedDefinitionId, DefinitionState<'db>>,
        used: IndexVec<ScopedDefinitionId, bool>,
        multipart_import_used: IndexVec<ScopedDefinitionId, bool>,
    ) -> Self {
        let mut states = states.into_iter();
        let mut used = used.into_iter();
        let mut multipart_import_used = multipart_import_used.into_iter();

        let unbound_state = states.next();
        let unbound_used = used.next();
        let unbound_multipart_import_used = multipart_import_used.next();
        debug_assert_eq!(unbound_state, Some(DefinitionState::Undefined));
        debug_assert_eq!(unbound_used, Some(false));
        debug_assert_eq!(unbound_multipart_import_used, Some(false));

        Self {
            states: states
                .zip(used)
                .zip(multipart_import_used)
                .map(|((state, used), multipart_import_used)| {
                    RetainedDefinitionState::new(state, used, multipart_import_used)
                })
                .collect(),
        }
    }

    #[inline]
    fn get(&self, id: ScopedDefinitionId) -> RetainedDefinitionState<'db> {
        let index = id.index();
        if index == 0 {
            RetainedDefinitionState::Undefined
        } else {
            self.states[index - 1]
        }
    }

    fn iter_enumerated(
        &self,
    ) -> impl Iterator<Item = (ScopedDefinitionId, RetainedDefinitionState<'db>)> + '_ {
        std::iter::once((
            ScopedDefinitionId::UNBOUND,
            RetainedDefinitionState::Undefined,
        ))
        .chain(
            self.states
                .iter()
                .copied()
                .enumerate()
                .map(|(index, state)| (ScopedDefinitionId::new(index + 1), state)),
        )
    }
}

/// Applicable definitions and constraints for every use of a name.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub struct UseDefMap<'db> {
    /// Definition states in this scope, plus an implicit "unbound"/"undeclared" definition at
    /// index zero.
    all_definitions: RetainedDefinitions<'db>,

    /// Constraint lookup tables, absent when all retained constraints are built-in terminal
    /// values that require no table lookup.
    constraint_tables: Option<Box<ConstraintTables<'db>>>,

    /// Interned [`Bindings`] values.
    interned_bindings: RetainedBindings,
    /// Interned [`Declarations`] values.
    interned_declarations: RetainedDeclarations,

    /// Tracks the reachability constraint for statements and certain sub-expressions
    /// (e.g. ternary branches, boolean operator operands), keyed by their text range.
    /// Used to suppress diagnostics in unreachable code.
    range_reachability: Box<[(TextRange, RangeInfo)]>,

    /// If the definition is a binding (only) -- `x = 1` for example -- then we need
    /// [`Declarations`] to know whether this binding is permitted by the live declarations.
    ///
    /// If the definition is both a declaration and a binding -- `x: int = 1` for example -- then
    /// we don't actually need anything here, all we'll need to validate is that our own RHS is a
    /// valid assignment to our own annotation.
    ///
    /// If the definition is a declaration (only) -- `x: int` for example -- then we need
    /// [`Bindings`] to know whether this declaration is consistent with the previously
    /// inferred type.
    ///
    /// If we see a binding to a `Final`-qualified symbol, we also need the bindings to find
    /// previous bindings to that symbol. If there are any, the assignment is invalid.
    ///
    /// Entries whose prior state is the start-of-scope default (always unbound and, if present,
    /// always undeclared) are omitted. Lookups use [`ALWAYS_UNBOUND_BINDINGS`] and
    /// [`ALWAYS_UNDECLARED_DECLARATIONS`], which are initialized lazily and shared by every map.
    definitions_by_definition: FrozenMap<
        Definition<'db>,
        DefinitionsAtDefinition<InternedBindingsId, InternedDeclarationsId>,
    >,

    /// Retained [`PlaceState`] values for each symbol.
    symbol_states: FrozenIndexVec<ScopedSymbolId, RetainedPlaceStates<InternedPlaceStateId>>,

    /// Collection fields omitted when they would all be empty.
    extra: Option<Box<UseDefMapExtra>>,

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
    /// This is used by `can_implicitly_return_none` in the `ty_python_semantic` crate.
    end_of_scope_reachability: ScopedReachabilityConstraintId,
}

/// Information about a given range of source code.
#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize)]
struct RangeInfo {
    reachability: ScopedReachabilityConstraintId,
    in_type_checking_block: bool,
}

impl Default for RangeInfo {
    fn default() -> Self {
        Self {
            reachability: ScopedReachabilityConstraintId::ALWAYS_TRUE,
            in_type_checking_block: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
struct MultiBindingsByUse(ThinVec<(ScopedUseId, Box<[Bindings]>)>);

impl MultiBindingsByUse {
    fn from_map(map: FxHashMap<ScopedUseId, Vec<Bindings>>) -> Self {
        let mut entries = map
            .into_iter()
            .map(|(use_id, bindings)| (use_id, bindings.into_boxed_slice()))
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|(use_id, _)| *use_id);
        Self(entries.into_iter().collect())
    }

    fn get(&self, use_id: ScopedUseId) -> Option<&[Bindings]> {
        self.0
            .binary_search_by_key(&use_id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| self.0[index].1.as_ref())
    }
}

pub enum ApplicableConstraints<'map, 'db> {
    UnboundBinding(NarrowingEvaluator<'map, 'db>),
    ConstrainedBindings(BindingWithConstraintsIterator<'map, 'db>),
}

impl<'db> UseDefMap<'db> {
    fn constraint_tables(&self) -> &ConstraintTables<'db> {
        self.constraint_tables
            .as_deref()
            .map_or(&EMPTY_CONSTRAINT_TABLES, |tables| tables)
    }

    fn extra(&self) -> &UseDefMapExtra {
        self.extra
            .as_deref()
            .expect("extra use-def data should have been retained")
    }

    pub fn loop_header(&self, id: LoopHeaderId) -> &LoopHeader {
        &self.extra().loop_headers[id]
    }

    pub fn reachability_constraints(&self) -> &ReachabilityConstraints {
        &self.constraint_tables().reachability_constraints
    }

    pub fn narrowing_constraints(&self) -> &NarrowingConstraints {
        &self.constraint_tables().narrowing_constraints
    }

    pub fn predicates(&self) -> &Predicates<'db> {
        &self.constraint_tables().predicates
    }

    pub fn range_reachability(
        &self,
    ) -> impl Iterator<Item = (TextRange, ScopedReachabilityConstraintId)> + '_ {
        self.range_reachability
            .iter()
            .map(|&(range, RangeInfo { reachability, .. })| (range, reachability))
    }

    pub fn end_of_scope_reachability(&self) -> ScopedReachabilityConstraintId {
        self.end_of_scope_reachability
    }

    pub fn all_definitions_with_usage(
        &self,
    ) -> impl Iterator<Item = (ScopedDefinitionId, DefinitionState<'db>, bool)> + '_ {
        self.all_definitions
            .iter_enumerated()
            .map(|(id, state)| (id, state.state(), state.is_used()))
    }

    pub fn is_multipart_import_definition_used(&self, definition: ScopedDefinitionId) -> bool {
        self.all_definitions
            .get(definition)
            .is_multipart_import_used()
    }

    pub fn bindings_at_use(&self, use_id: ScopedUseId) -> BindingWithConstraintsIterator<'_, 'db> {
        let bindings_id = self.extra().bindings_by_use[use_id];
        self.bindings_iterator(
            &self.interned_bindings[bindings_id],
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub fn multi_bindings_at_use(
        &self,
        use_id: ScopedUseId,
    ) -> impl Iterator<Item = BindingWithConstraintsIterator<'_, 'db>> {
        self.extra
            .as_deref()
            .and_then(|extra| extra.multi_bindings_by_use.get(use_id))
            .map(|member_bindings| {
                member_bindings.iter().map(|bindings| {
                    self.bindings_iterator(
                        bindings.as_slice(),
                        BoundnessAnalysis::BasedOnUnboundVisibility,
                    )
                })
            })
            .into_iter()
            .flatten()
    }

    pub fn applicable_constraints(
        &self,
        constraint_key: ConstraintKey,
        enclosing_scope: FileScopeId,
        expr: PlaceExprRef,
        index: &'db SemanticIndex,
    ) -> ApplicableConstraints<'_, 'db> {
        match constraint_key {
            ConstraintKey::NarrowingConstraint(constraint) => {
                ApplicableConstraints::UnboundBinding(NarrowingEvaluator {
                    constraint,
                    constraint_tables: self.constraint_tables(),
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

    pub fn definition(&self, id: ScopedDefinitionId) -> DefinitionState<'db> {
        self.all_definitions.get(id).state()
    }

    pub fn narrowing_evaluator(
        &self,
        constraint: ScopedNarrowingConstraint,
    ) -> NarrowingEvaluator<'_, 'db> {
        NarrowingEvaluator {
            constraint,
            constraint_tables: self.constraint_tables(),
        }
    }

    pub(crate) fn is_range_in_type_checking_block(&self, range: TextRange) -> bool {
        self.range_reachability
            .iter()
            .take_while(|(entry_range, _)| entry_range.start() <= range.start())
            .any(|&(entry_range, block)| {
                block.in_type_checking_block && entry_range.contains_range(range)
            })
    }
    pub fn end_of_scope_bindings(
        &self,
        place: ScopedPlaceId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.end_of_scope_symbol_bindings(symbol),
            ScopedPlaceId::Member(member) => self.end_of_scope_member_bindings(member),
        }
    }

    pub fn end_of_scope_symbol_bindings(
        &self,
        symbol: ScopedSymbolId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        let place_state_id = self.symbol_states[symbol].end_of_scope;
        self.bindings_iterator(
            &self.interned_bindings[place_state_id.bindings_id()],
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub(crate) fn end_of_scope_member_bindings(
        &self,
        member: ScopedMemberId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        let place_state_id = self.extra().member_states[member].end_of_scope;
        self.bindings_iterator(
            &self.interned_bindings[place_state_id.bindings_id()],
            BoundnessAnalysis::BasedOnUnboundVisibility,
        )
    }

    pub fn reachable_bindings(
        &self,
        place: ScopedPlaceId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.reachable_symbol_bindings(symbol),
            ScopedPlaceId::Member(member) => self.reachable_member_bindings(member),
        }
    }

    pub fn reachable_symbol_bindings(
        &self,
        symbol: ScopedSymbolId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        let place_state_id = self.symbol_states[symbol].reachable;
        let bindings = &self.interned_bindings[place_state_id.bindings_id()];
        self.bindings_iterator(bindings, BoundnessAnalysis::AssumeBound)
    }

    pub fn reachable_member_bindings(
        &self,
        member: ScopedMemberId,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        let place_state_id = self.extra().member_states[member].reachable;
        let bindings = &self.interned_bindings[place_state_id.bindings_id()];
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

        let Some(extra) = self.extra.as_deref() else {
            return EnclosingSnapshotResult::NotFound;
        };

        match extra.enclosing_snapshots.get(snapshot_id) {
            Some(InternedEnclosingSnapshotId::Constraint(constraint)) => {
                EnclosingSnapshotResult::FoundConstraint(*constraint)
            }
            Some(InternedEnclosingSnapshotId::Bindings(bindings_id)) => {
                EnclosingSnapshotResult::FoundBindings(
                    self.bindings_iterator(
                        &self.interned_bindings[*bindings_id],
                        boundness_analysis,
                    ),
                )
            }
            None => EnclosingSnapshotResult::NotFound,
        }
    }

    pub fn bindings_at_definition(
        &self,
        definition: Definition<'db>,
    ) -> BindingWithConstraintsIterator<'_, 'db> {
        let bindings = self.definitions_by_definition.get(&definition).map_or_else(
            || ALWAYS_UNBOUND_BINDINGS.as_slice(),
            |definitions| &self.interned_bindings[definitions.bindings],
        );
        self.bindings_iterator(bindings, BoundnessAnalysis::BasedOnUnboundVisibility)
    }

    pub fn declarations_at_binding(
        &self,
        binding: Definition<'db>,
    ) -> DeclarationsIterator<'_, 'db> {
        let declarations = self.definitions_by_definition.get(&binding).map_or_else(
            || ALWAYS_UNDECLARED_DECLARATIONS.as_slice(),
            |definitions| {
                &self.interned_declarations[definitions
                    .declarations
                    .expect("binding definition should have retained declarations")]
            },
        );
        self.declarations_iterator(declarations, BoundnessAnalysis::BasedOnUnboundVisibility)
    }

    pub fn end_of_scope_declarations<'map>(
        &'map self,
        place: ScopedPlaceId,
    ) -> DeclarationsIterator<'map, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.end_of_scope_symbol_declarations(symbol),
            ScopedPlaceId::Member(member) => self.end_of_scope_member_declarations(member),
        }
    }

    pub fn end_of_scope_symbol_declarations<'map>(
        &'map self,
        symbol: ScopedSymbolId,
    ) -> DeclarationsIterator<'map, 'db> {
        let place_state_id = self.symbol_states[symbol].end_of_scope;
        let declarations = &self.interned_declarations[place_state_id.declarations_id()];
        self.declarations_iterator(declarations, BoundnessAnalysis::BasedOnUnboundVisibility)
    }

    pub(crate) fn end_of_scope_member_declarations<'map>(
        &'map self,
        member: ScopedMemberId,
    ) -> DeclarationsIterator<'map, 'db> {
        let place_state_id = self.extra().member_states[member].end_of_scope;
        let declarations = &self.interned_declarations[place_state_id.declarations_id()];
        self.declarations_iterator(declarations, BoundnessAnalysis::BasedOnUnboundVisibility)
    }

    pub fn reachable_symbol_declarations(
        &self,
        symbol: ScopedSymbolId,
    ) -> DeclarationsIterator<'_, 'db> {
        let place_state_id = self.symbol_states[symbol].reachable;
        let declarations = &self.interned_declarations[place_state_id.declarations_id()];
        self.declarations_iterator(declarations, BoundnessAnalysis::AssumeBound)
    }

    pub fn reachable_member_declarations(
        &self,
        member: ScopedMemberId,
    ) -> DeclarationsIterator<'_, 'db> {
        let place_state_id = self.extra().member_states[member].reachable;
        let declarations = &self.interned_declarations[place_state_id.declarations_id()];
        self.declarations_iterator(declarations, BoundnessAnalysis::AssumeBound)
    }

    pub fn reachable_declarations(&self, place: ScopedPlaceId) -> DeclarationsIterator<'_, 'db> {
        match place {
            ScopedPlaceId::Symbol(symbol) => self.reachable_symbol_declarations(symbol),
            ScopedPlaceId::Member(member) => self.reachable_member_declarations(member),
        }
    }

    pub fn all_end_of_scope_symbol_declarations<'map>(
        &'map self,
    ) -> impl Iterator<Item = (ScopedSymbolId, DeclarationsIterator<'map, 'db>)> + 'map {
        self.symbol_states
            .indices()
            .map(|symbol_id| (symbol_id, self.end_of_scope_symbol_declarations(symbol_id)))
    }

    pub fn all_end_of_scope_symbol_bindings<'map>(
        &'map self,
    ) -> impl Iterator<Item = (ScopedSymbolId, BindingWithConstraintsIterator<'map, 'db>)> + 'map
    {
        self.symbol_states
            .indices()
            .map(|symbol_id| (symbol_id, self.end_of_scope_symbol_bindings(symbol_id)))
    }

    pub fn all_reachable_symbols<'map>(
        &'map self,
    ) -> impl Iterator<
        Item = (
            ScopedSymbolId,
            DeclarationsIterator<'map, 'db>,
            BindingWithConstraintsIterator<'map, 'db>,
        ),
    > + 'map {
        self.symbol_states.iter_enumerated().map(
            |(symbol_id, RetainedPlaceStates { reachable, .. })| {
                let declarations = self.declarations_iterator(
                    &self.interned_declarations[reachable.declarations_id()],
                    BoundnessAnalysis::AssumeBound,
                );
                let bindings = self.bindings_iterator(
                    &self.interned_bindings[reachable.bindings_id()],
                    BoundnessAnalysis::AssumeBound,
                );
                (symbol_id, declarations, bindings)
            },
        )
    }

    fn bindings_iterator<'map>(
        &'map self,
        bindings: &'map [LiveBinding],
        boundness_analysis: BoundnessAnalysis,
    ) -> BindingWithConstraintsIterator<'map, 'db> {
        BindingWithConstraintsIterator {
            all_definitions: &self.all_definitions,
            constraint_tables: self.constraint_tables(),
            boundness_analysis,
            inner: bindings.iter(),
        }
    }

    fn declarations_iterator<'map>(
        &'map self,
        declarations: &'map [LiveDeclaration],
        boundness_analysis: BoundnessAnalysis,
    ) -> DeclarationsIterator<'map, 'db> {
        DeclarationsIterator {
            all_definitions: &self.all_definitions,
            constraint_tables: self.constraint_tables(),
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, get_size2::GetSize)]
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
pub struct BindingWithConstraintsIterator<'map, 'db> {
    all_definitions: &'map RetainedDefinitions<'db>,
    constraint_tables: &'map ConstraintTables<'db>,
    boundness_analysis: BoundnessAnalysis,
    inner: LiveBindingsIterator<'map>,
}

impl<'map, 'db> BindingWithConstraintsIterator<'map, 'db> {
    pub const fn predicates(&self) -> &'map Predicates<'db> {
        &self.constraint_tables.predicates
    }

    pub const fn reachability_constraints(&self) -> &'map ReachabilityConstraints {
        &self.constraint_tables.reachability_constraints
    }

    pub const fn boundness_analysis(&self) -> BoundnessAnalysis {
        self.boundness_analysis
    }
}

impl<'map, 'db> Iterator for BindingWithConstraintsIterator<'map, 'db> {
    type Item = BindingWithConstraints<'map, 'db>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|live_binding| BindingWithConstraints {
                binding: self.all_definitions.get(live_binding.binding()).state(),
                binding_order: live_binding.binding(),
                narrowing_constraint: NarrowingEvaluator {
                    constraint: live_binding.narrowing_constraint(),
                    constraint_tables: self.constraint_tables,
                },
                reachability_constraint: live_binding.reachability_constraint(),
            })
    }
}

impl std::iter::FusedIterator for BindingWithConstraintsIterator<'_, '_> {}

pub struct BindingWithConstraints<'map, 'db> {
    pub binding: DefinitionState<'db>,
    /// Stable binding order within the containing scope.
    pub binding_order: ScopedDefinitionId,
    pub narrowing_constraint: NarrowingEvaluator<'map, 'db>,
    pub reachability_constraint: ScopedReachabilityConstraintId,
}

pub struct NarrowingEvaluator<'map, 'db> {
    constraint: ScopedNarrowingConstraint,
    constraint_tables: &'map ConstraintTables<'db>,
}

impl<'map, 'db> NarrowingEvaluator<'map, 'db> {
    pub fn constraint(&self) -> ScopedNarrowingConstraint {
        self.constraint
    }

    pub fn predicates(&self) -> &'map Predicates<'db> {
        &self.constraint_tables.predicates
    }

    pub fn narrowing_constraints(&self) -> &'map NarrowingConstraints {
        &self.constraint_tables.narrowing_constraints
    }
}

#[derive(Clone)]
pub struct DeclarationsIterator<'map, 'db> {
    all_definitions: &'map RetainedDefinitions<'db>,
    constraint_tables: &'map ConstraintTables<'db>,
    boundness_analysis: BoundnessAnalysis,
    inner: LiveDeclarationsIterator<'map>,
}

impl<'map, 'db> DeclarationsIterator<'map, 'db> {
    pub const fn predicates(&self) -> &'map Predicates<'db> {
        &self.constraint_tables.predicates
    }

    pub const fn reachability_constraints(&self) -> &'map ReachabilityConstraints {
        &self.constraint_tables.reachability_constraints
    }

    pub const fn boundness_analysis(&self) -> BoundnessAnalysis {
        self.boundness_analysis
    }
}

#[derive(Debug, Clone)]
pub struct DeclarationWithConstraint<'db> {
    pub declaration: DefinitionState<'db>,
    /// Stable declaration order within the containing scope.
    pub declaration_order: ScopedDefinitionId,
    pub reachability_constraint: ScopedReachabilityConstraintId,
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
                    declaration: self.all_definitions.get(*declaration).state(),
                    declaration_order: *declaration,
                    reachability_constraint: *reachability_constraint,
                }
            },
        )
    }
}

impl std::iter::FusedIterator for DeclarationsIterator<'_, '_> {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
struct ReachableDefinitions {
    bindings: Bindings,
    declarations: Declarations,
}

/// A snapshot of the definitions and constraints state at a particular point in control flow.
#[derive(Clone, Debug)]
pub(super) struct FlowSnapshot {
    symbol_states: IndexVec<ScopedSymbolId, PendingPlaceState>,
    member_states: IndexVec<ScopedMemberId, PendingPlaceState>,
    reachability: ScopedReachabilityConstraintId,
    pending_reachability: PendingReachabilityId,
}

impl FlowSnapshot {
    pub(super) fn is_always_unreachable(&self) -> bool {
        self.reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE
    }
}

/// Identifies a node in the tree of pending reachability constraints.
#[newtype_index]
struct PendingReachabilityId;

#[derive(Debug)]
struct PendingReachabilityConstraint {
    parent: PendingReachabilityId,
    constraint: ScopedReachabilityConstraintId,
}

/// An append-only tree of scope-wide reachability constraints.
///
/// Each [`PendingPlaceState`] remembers the last node applied to its place state, so snapshots can
/// share place states and defer applying subsequent constraints until the place is observed or
/// changed.
#[derive(Debug)]
struct PendingReachability {
    constraints: IndexVec<PendingReachabilityId, PendingReachabilityConstraint>,
    current: PendingReachabilityId,
}

impl Default for PendingReachability {
    fn default() -> Self {
        let mut constraints = IndexVec::new();
        let root = constraints.next_index();
        constraints.push(PendingReachabilityConstraint {
            parent: root,
            constraint: ScopedReachabilityConstraintId::ALWAYS_TRUE,
        });
        Self {
            constraints,
            current: root,
        }
    }
}

impl PendingReachability {
    fn push(&mut self, constraint: ScopedReachabilityConstraintId) {
        self.current = self.constraints.push(PendingReachabilityConstraint {
            parent: self.current,
            constraint,
        });
    }

    /// Applies the constraints between the place's last materialized node and `target`.
    ///
    /// The place's node must be an ancestor of `target`. After materialization, the place is
    /// uniquely owned for mutation and records `target` as its last applied node.
    fn materialize<'a>(
        &self,
        pending: &'a mut PendingPlaceState,
        target: PendingReachabilityId,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) -> &'a mut PlaceState {
        if pending.reachability != target {
            let mut unapplied = SmallVec::<[ScopedReachabilityConstraintId; 4]>::new();
            let mut current = target;
            while current != pending.reachability {
                let event = &self.constraints[current];
                unapplied.push(event.constraint);
                assert_ne!(
                    current, event.parent,
                    "pending reachability must be an ancestor"
                );
                current = event.parent;
            }

            let state = Rc::make_mut(&mut pending.state);
            for constraint in unapplied.into_iter().rev() {
                state.record_reachability_constraint(reachability_constraints, constraint);
            }
            pending.reachability = target;
        }

        Rc::make_mut(&mut pending.state)
    }

    /// Returns the materialized place state for immutable access.
    ///
    /// Call this instead of [`Self::materialize`] when the state will only be read. If the pending
    /// constraints are already materialized, this preserves the shared [`Rc`] instead of making
    /// the state uniquely owned.
    fn materialize_ref<'a>(
        &self,
        pending: &'a mut PendingPlaceState,
        target: PendingReachabilityId,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) -> &'a PlaceState {
        if pending.reachability != target {
            self.materialize(pending, target, reachability_constraints);
        }
        &pending.state
    }

    /// Combines the constraints after `ancestor` through `target` into a single constraint.
    ///
    /// `ancestor` must be an ancestor of `target`.
    fn constraint_between(
        &self,
        ancestor: PendingReachabilityId,
        target: PendingReachabilityId,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) -> ScopedReachabilityConstraintId {
        let mut constraint = ScopedReachabilityConstraintId::ALWAYS_TRUE;
        let mut current = target;
        while current != ancestor {
            let event = &self.constraints[current];
            constraint = reachability_constraints.add_and_constraint(constraint, event.constraint);
            assert_ne!(
                current, event.parent,
                "pending reachability must be an ancestor"
            );
            current = event.parent;
        }
        constraint
    }
}

/// A copy-on-write place state and the last reachability node materialized into it.
#[derive(Clone, Debug)]
struct PendingPlaceState {
    state: Rc<PlaceState>,
    reachability: PendingReachabilityId,
}

impl PendingPlaceState {
    fn new(state: PlaceState, reachability: PendingReachabilityId) -> Self {
        Self {
            state: Rc::new(state),
            reachability,
        }
    }
}

fn pending_place_state_mut<'a>(
    place: ScopedPlaceId,
    symbol_states: &'a mut IndexVec<ScopedSymbolId, PendingPlaceState>,
    member_states: &'a mut IndexVec<ScopedMemberId, PendingPlaceState>,
) -> &'a mut PendingPlaceState {
    match place {
        ScopedPlaceId::Symbol(symbol) => &mut symbol_states[symbol],
        ScopedPlaceId::Member(member) => &mut member_states[member],
    }
}

impl PendingReachability {
    /// Merges an alternative branch's place states into the current control-flow path.
    ///
    /// States shared by both branches only need their path constraints merged. States that differ
    /// are materialized before their bindings and declarations are merged, while places absent
    /// from the alternative branch are treated as undefined on that path.
    fn merge_place_states<I: Idx>(
        &self,
        current_states: &mut IndexVec<I, PendingPlaceState>,
        branch_states: IndexVec<I, PendingPlaceState>,
        branch: PendingReachabilityId,
        branch_reachability: ScopedReachabilityConstraintId,
        narrowing_constraints: &mut NarrowingConstraintsBuilder,
        reachability_constraints: &mut ReachabilityConstraintsBuilder,
    ) {
        let mut branch_states = branch_states.into_iter();
        for current in current_states {
            let Some(mut branch_state) = branch_states.next() else {
                let current = self.materialize(current, self.current, reachability_constraints);
                current.merge(
                    PlaceState::undefined(branch_reachability),
                    narrowing_constraints,
                    reachability_constraints,
                );
                continue;
            };

            // If neither branch changed the place itself, merge just the path constraints. The
            // common case is a truthy/falsy pair whose constraints cancel to `ALWAYS_TRUE`, leaving
            // the shared state untouched.
            if current.reachability == branch_state.reachability
                && Rc::ptr_eq(&current.state, &branch_state.state)
            {
                if self.current == branch {
                    continue;
                }

                let current_constraint = self.constraint_between(
                    current.reachability,
                    self.current,
                    reachability_constraints,
                );
                let branch_constraint = self.constraint_between(
                    branch_state.reachability,
                    branch,
                    reachability_constraints,
                );
                let merged_constraint = reachability_constraints
                    .add_or_constraint(current_constraint, branch_constraint);
                if merged_constraint != ScopedReachabilityConstraintId::ALWAYS_TRUE {
                    Rc::make_mut(&mut current.state).record_reachability_constraint(
                        reachability_constraints,
                        merged_constraint,
                    );
                }
                current.reachability = self.current;
                continue;
            }

            self.materialize(&mut branch_state, branch, reachability_constraints);
            let branch_state = Rc::unwrap_or_clone(branch_state.state);
            let current = self.materialize(current, self.current, reachability_constraints);
            current.merge(
                branch_state,
                narrowing_constraints,
                reachability_constraints,
            );
        }
    }
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

    /// Tracks whether each binding definition has at least one use.
    ///
    /// Uses the same index as `all_definitions`.
    used_bindings: IndexVec<ScopedDefinitionId, bool>,

    /// Tracks whether each multipart import definition has a dotted attribute use.
    ///
    /// Uses the same index as `all_definitions`.
    used_multipart_imports: IndexVec<ScopedDefinitionId, bool>,

    /// Builder of predicates.
    pub(super) predicates: PredicatesBuilder<'db>,

    /// Builder of reachability constraints.
    pub(super) reachability_constraints: ReachabilityConstraintsBuilder,

    /// Builder of narrowing constraints.
    pub(super) narrowing_constraints: NarrowingConstraintsBuilder,

    /// Live bindings at each so-far-recorded use.
    bindings_by_use: IndexVec<ScopedUseId, Bindings>,

    /// Live bindings associated with each so-far-recorded use.
    ///
    /// Unlike `bindings_by_use`, this field supports associating multiple bindings with a
    /// single use. This is only used for kwargs expressions, whose corresponding `bindings_by_use`
    /// entry is empty.
    multi_bindings_by_use: FxHashMap<ScopedUseId, Vec<Bindings>>,

    /// Tracks whether or not the current point in control flow is reachable from the
    /// start of the scope.
    pub(super) reachability: ScopedReachabilityConstraintId,

    /// Tracks the reachability constraint for statements and certain sub-expressions,
    /// keyed by their text range.
    range_reachability: Vec<(TextRange, RangeInfo)>,

    /// Live bindings for each so-far-recorded definition and, for binding-only definitions, the
    /// live declarations.
    definitions_by_definition:
        FxHashMap<Definition<'db>, DefinitionsAtDefinition<Bindings, Declarations>>,

    /// Currently live bindings and declarations for each place.
    symbol_states: IndexVec<ScopedSymbolId, PendingPlaceState>,

    member_states: IndexVec<ScopedMemberId, PendingPlaceState>,

    /// Reachability constraints that apply to every currently live place are recorded here and
    /// folded into individual place states only when that place is observed or changed.
    pending_reachability: PendingReachability,

    /// All potentially reachable bindings and declarations, for each place.
    reachable_symbol_definitions: IndexVec<ScopedSymbolId, ReachableDefinitions>,

    reachable_member_definitions: IndexVec<ScopedMemberId, ReachableDefinitions>,

    /// Snapshots of place states in this scope that can be used to resolve a reference in a
    /// nested scope.
    enclosing_snapshots: EnclosingSnapshots,

    /// Loop headers reserved before walking a loop and populated afterward.
    loop_headers: IndexVec<LoopHeaderId, LoopHeader>,

    /// Is this a class scope?
    is_class_scope: bool,
}

impl<'db> UseDefMapBuilder<'db> {
    pub(super) fn new(is_class_scope: bool) -> Self {
        Self {
            all_definitions: IndexVec::from_iter([DefinitionState::Undefined]),
            used_bindings: IndexVec::from_iter([false]),
            used_multipart_imports: IndexVec::from_iter([false]),
            predicates: PredicatesBuilder::default(),
            reachability_constraints: ReachabilityConstraintsBuilder::default(),
            narrowing_constraints: NarrowingConstraintsBuilder::default(),
            bindings_by_use: IndexVec::new(),
            multi_bindings_by_use: FxHashMap::default(),
            reachability: ScopedReachabilityConstraintId::ALWAYS_TRUE,
            range_reachability: Vec::new(),
            definitions_by_definition: FxHashMap::default(),
            symbol_states: IndexVec::new(),
            member_states: IndexVec::new(),
            pending_reachability: PendingReachability::default(),
            reachable_member_definitions: IndexVec::new(),
            reachable_symbol_definitions: IndexVec::new(),
            enclosing_snapshots: EnclosingSnapshots::default(),
            loop_headers: IndexVec::new(),
            is_class_scope,
        }
    }

    pub(super) fn reserve_loop_header(&mut self) -> LoopHeaderId {
        self.loop_headers.push(LoopHeader::new())
    }

    pub(super) fn set_loop_header(&mut self, id: LoopHeaderId, header: LoopHeader) {
        self.loop_headers[id] = header;
    }

    fn push_definition(&mut self, state: DefinitionState<'db>) -> ScopedDefinitionId {
        let def_id = self.all_definitions.push(state);
        let used_id = self.used_bindings.push(false);
        let multipart_used_id = self.used_multipart_imports.push(false);
        debug_assert_eq!(def_id, used_id);
        debug_assert_eq!(def_id, multipart_used_id);
        def_id
    }

    pub(super) fn definition(&self, def_id: ScopedDefinitionId) -> DefinitionState<'db> {
        self.all_definitions[def_id]
    }

    pub(super) fn mark_unreachable(&mut self) {
        self.record_reachability_constraint(ScopedReachabilityConstraintId::ALWAYS_FALSE);
    }

    pub(super) fn add_place(&mut self, place: ScopedPlaceId) {
        match place {
            ScopedPlaceId::Symbol(symbol) => {
                let new_place = self.symbol_states.push(PendingPlaceState::new(
                    PlaceState::undefined(self.reachability),
                    self.pending_reachability.current,
                ));
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
                let new_place = self.member_states.push(PendingPlaceState::new(
                    PlaceState::undefined(self.reachability),
                    self.pending_reachability.current,
                ));
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

    pub(super) fn next_definition_id(&self) -> ScopedDefinitionId {
        self.all_definitions.next_index()
    }

    pub(super) fn record_binding(
        &mut self,
        place: ScopedPlaceId,
        binding: Definition<'db>,
        previous_definitions: PreviousDefinitions,
        can_be_shadowed: FutureDefinitions,
    ) {
        let pending = self.pending_reachability.current;
        let def_id = self.push_definition(DefinitionState::Defined(binding));
        let place_state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let place_state = self.pending_reachability.materialize(
            place_state,
            pending,
            &mut self.reachability_constraints,
        );
        let definitions_at_definition = DefinitionsAtDefinition {
            bindings: place_state.bindings().clone(),
            declarations: Some(place_state.declarations().clone()),
        };

        place_state.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
            previous_definitions,
            can_be_shadowed,
        );
        self.definitions_by_definition
            .insert(binding, definitions_at_definition);

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
            can_be_shadowed,
        );
    }

    pub(crate) fn bindings_at_use(
        &self,
        use_id: ScopedUseId,
    ) -> impl Iterator<Item = &LiveBinding> {
        self.bindings_by_use[use_id].iter()
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

    /// Records a narrowing constraint for only the specified places.
    pub(super) fn record_narrowing_constraint_for_places(
        &mut self,
        predicate: ScopedPredicateId,
        places: &PossiblyNarrowedPlaces,
    ) {
        if predicate == ScopedPredicateId::ALWAYS_TRUE
            || predicate == ScopedPredicateId::ALWAYS_FALSE
        {
            // No need to record a narrowing constraint for `True` or `False`.
            return;
        }

        let atom = self.narrowing_constraints.add_atom(predicate);
        self.record_narrowing_constraint_node_for_places(atom, places);
    }

    /// Records a narrowing constraint on the current live bindings that were read by the
    /// corresponding earlier uses.
    pub(super) fn record_narrowing_constraint_for_bindings_at_use(
        &mut self,
        predicate: ScopedPredicateId,
        place: ScopedPlaceId,
        use_id: ScopedUseId,
    ) {
        if predicate == ScopedPredicateId::ALWAYS_TRUE
            || predicate == ScopedPredicateId::ALWAYS_FALSE
        {
            return;
        }

        let constraint = self.narrowing_constraints.add_atom(predicate);
        let pending = self.pending_reachability.current;
        let state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let state = self.pending_reachability.materialize(
            state,
            pending,
            &mut self.reachability_constraints,
        );
        state.record_narrowing_constraint_for_bindings_at_use(
            &mut self.narrowing_constraints,
            constraint,
            &self.bindings_by_use[use_id],
        );
    }

    /// Records a narrowing constraint on the current live bindings selected by definition ID.
    pub(super) fn record_narrowing_constraint_for_bindings(
        &mut self,
        predicate: ScopedPredicateId,
        place: ScopedPlaceId,
        bindings: &[ScopedDefinitionId],
    ) {
        if predicate == ScopedPredicateId::ALWAYS_TRUE
            || predicate == ScopedPredicateId::ALWAYS_FALSE
        {
            return;
        }

        let constraint = self.narrowing_constraints.add_atom(predicate);
        let pending = self.pending_reachability.current;
        let state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let state = self.pending_reachability.materialize(
            state,
            pending,
            &mut self.reachability_constraints,
        );
        state.record_narrowing_constraint_for_bindings(
            &mut self.narrowing_constraints,
            constraint,
            bindings,
        );
    }

    /// Records a negated narrowing constraint for only the specified places.
    ///
    /// The positive and negative constraints use the same predicate ID. This lets `P or not P`
    /// simplify to `ALWAYS_TRUE`, so narrowing cancels out after a complete `if`/`else`.
    pub(super) fn record_negated_narrowing_constraint_for_places(
        &mut self,
        predicate: ScopedPredicateId,
        places: &PossiblyNarrowedPlaces,
    ) {
        if predicate == ScopedPredicateId::ALWAYS_TRUE
            || predicate == ScopedPredicateId::ALWAYS_FALSE
        {
            return;
        }

        let negated = self.narrowing_constraints.add_negated_atom(predicate);
        self.record_narrowing_constraint_node_for_places(negated, places);
    }

    /// Records a narrowing constraint node for the specified places.
    fn record_narrowing_constraint_node_for_places(
        &mut self,
        constraint: ScopedNarrowingConstraint,
        places: &PossiblyNarrowedPlaces,
    ) {
        let pending = self.pending_reachability.current;
        #[expect(
            clippy::iter_over_hash_type,
            reason = "the same constraint is recorded independently for each place"
        )]
        for place in places {
            match place {
                ScopedPlaceId::Symbol(symbol_id) => {
                    if let Some(state) = self.symbol_states.get_mut(*symbol_id) {
                        let state = self.pending_reachability.materialize(
                            state,
                            pending,
                            &mut self.reachability_constraints,
                        );
                        state.record_narrowing_constraint(
                            &mut self.narrowing_constraints,
                            constraint,
                        );
                    }
                }
                ScopedPlaceId::Member(member_id) => {
                    if let Some(state) = self.member_states.get_mut(*member_id) {
                        let state = self.pending_reachability.materialize(
                            state,
                            pending,
                            &mut self.reachability_constraints,
                        );
                        state.record_narrowing_constraint(
                            &mut self.narrowing_constraints,
                            constraint,
                        );
                    }
                }
            }
        }
    }

    /// Snapshot the state of a single symbol and all of its associated members, at the current
    /// point in control flow.
    ///
    /// This is only used for `*`-import reachability constraints, which are handled differently
    /// to most other reachability constraints. See the doc-comment for
    /// [`Self::record_and_negate_star_import_reachability_constraint`] for more details.
    pub(super) fn single_symbol_snapshot(
        &mut self,
        symbol: ScopedSymbolId,
        associated_member_ids: &[ScopedMemberId],
    ) -> SingleSymbolSnapshot {
        let pending = self.pending_reachability.current;
        let symbol_state = self
            .pending_reachability
            .materialize_ref(
                &mut self.symbol_states[symbol],
                pending,
                &mut self.reachability_constraints,
            )
            .clone();
        let mut associated_member_states = FxHashMap::default();
        for &member_id in associated_member_ids {
            let state = self.pending_reachability.materialize_ref(
                &mut self.member_states[member_id],
                pending,
                &mut self.reachability_constraints,
            );
            associated_member_states.insert(member_id, state.clone());
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
        let pending = self.pending_reachability.current;

        let symbol_state = self.pending_reachability.materialize(
            &mut self.symbol_states[symbol],
            pending,
            &mut self.reachability_constraints,
        );
        let mut post_definition_state =
            std::mem::replace(symbol_state, pre_definition.symbol_state);

        post_definition_state
            .record_reachability_constraint(&mut self.reachability_constraints, reachability_id);

        symbol_state.record_reachability_constraint(
            &mut self.reachability_constraints,
            negated_reachability_id,
        );

        symbol_state.merge(
            post_definition_state,
            &mut self.narrowing_constraints,
            &mut self.reachability_constraints,
        );

        // And similarly for all associated members:
        #[expect(
            clippy::iter_over_hash_type,
            reason = "associated member states are merged independently"
        )]
        for (member_id, pre_definition_member_state) in pre_definition.associated_member_states {
            let member_state = self.pending_reachability.materialize(
                &mut self.member_states[member_id],
                pending,
                &mut self.reachability_constraints,
            );
            let mut post_definition_state =
                std::mem::replace(member_state, pre_definition_member_state);

            post_definition_state.record_reachability_constraint(
                &mut self.reachability_constraints,
                reachability_id,
            );

            member_state.record_reachability_constraint(
                &mut self.reachability_constraints,
                negated_reachability_id,
            );

            member_state.merge(
                post_definition_state,
                &mut self.narrowing_constraints,
                &mut self.reachability_constraints,
            );
        }
    }

    /// Records a narrowing constraint for all places in the current scope.
    ///
    /// This is used to gate narrowing by `IsNonTerminalCall` constraints: when a branch contains
    /// a call to a `NoReturn` function, all narrowing in that branch should be conditional
    /// on the call actually returning `Never`.
    pub(super) fn record_narrowing_constraint_for_all_places(
        &mut self,
        constraint: ScopedNarrowingConstraint,
    ) {
        let pending = self.pending_reachability.current;
        for state in self
            .symbol_states
            .iter_mut()
            .chain(self.member_states.iter_mut())
        {
            let state = self.pending_reachability.materialize(
                state,
                pending,
                &mut self.reachability_constraints,
            );
            state.record_narrowing_constraint(&mut self.narrowing_constraints, constraint);
        }
    }

    pub(super) fn record_reachability_constraint(
        &mut self,
        constraint: ScopedReachabilityConstraintId,
    ) {
        self.reachability = self
            .reachability_constraints
            .add_and_constraint(self.reachability, constraint);
        self.pending_reachability.push(constraint);
    }

    pub(super) fn record_declaration(
        &mut self,
        place: ScopedPlaceId,
        declaration: Definition<'db>,
    ) {
        let def_id = self.push_definition(DefinitionState::Defined(declaration));
        let pending = self.pending_reachability.current;
        let place_state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let place_state = self.pending_reachability.materialize(
            place_state,
            pending,
            &mut self.reachability_constraints,
        );

        self.definitions_by_definition.insert(
            declaration,
            DefinitionsAtDefinition {
                bindings: place_state.bindings().clone(),
                declarations: None,
            },
        );
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
        // We don't need to store prior state for a definition that is both a declaration and a
        // binding.
        let def_id = self.push_definition(DefinitionState::Defined(definition));
        let pending = self.pending_reachability.current;
        let place_state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let place_state = self.pending_reachability.materialize(
            place_state,
            pending,
            &mut self.reachability_constraints,
        );
        place_state.record_declaration(def_id, self.reachability);
        place_state.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
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
            FutureDefinitions::ShadowThisOne,
        );
    }

    pub(super) fn delete_binding(&mut self, place: ScopedPlaceId) {
        let def_id = self.push_definition(DefinitionState::Deleted);
        let pending = self.pending_reachability.current;
        let place_state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let place_state = self.pending_reachability.materialize(
            place_state,
            pending,
            &mut self.reachability_constraints,
        );

        place_state.record_binding(
            def_id,
            self.reachability,
            self.is_class_scope,
            place.is_symbol(),
            PreviousDefinitions::AreShadowed,
            FutureDefinitions::ShadowThisOne,
        );
    }

    pub(super) fn record_use(&mut self, place: ScopedPlaceId, use_id: ScopedUseId) {
        let pending = self.pending_reachability.current;
        let place_state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let bindings = self
            .pending_reachability
            .materialize_ref(place_state, pending, &mut self.reachability_constraints)
            .bindings()
            .clone();

        self.record_use_bindings(bindings, use_id);
    }

    pub(super) fn reachable_symbol_binding_definition_ids(
        &self,
        symbol: ScopedSymbolId,
    ) -> Vec<ScopedDefinitionId> {
        self.reachable_symbol_definitions[symbol]
            .bindings
            .iter()
            .map(LiveBinding::binding)
            .collect()
    }

    pub(super) fn mark_multipart_import_definition_used(&mut self, definition: ScopedDefinitionId) {
        if definition.is_unbound() {
            return;
        }

        if matches!(
            self.all_definitions[definition],
            DefinitionState::Defined(_)
        ) {
            self.used_multipart_imports[definition] = true;
        }
    }

    pub(super) fn record_multi_use(
        &mut self,
        places: impl Iterator<Item = ScopedPlaceId>,
        use_id: ScopedUseId,
    ) {
        let pending = self.pending_reachability.current;
        for place in places {
            let place_state =
                pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
            let bindings = self
                .pending_reachability
                .materialize_ref(place_state, pending, &mut self.reachability_constraints)
                .bindings()
                .clone();

            let binding_definition_ids = bindings.iter().map(LiveBinding::binding);
            self.mark_definition_ids_used(binding_definition_ids);

            self.multi_bindings_by_use
                .entry(use_id)
                .or_default()
                .push(bindings);
        }

        // Record a placeholder use of the parent expression to preserve the indices of `bindings_by_use`.
        self.record_use_bindings(Bindings::default(), use_id);
    }

    fn record_use_bindings(&mut self, bindings: Bindings, use_id: ScopedUseId) {
        let binding_definition_ids = bindings.iter().map(LiveBinding::binding);
        self.mark_definition_ids_used(binding_definition_ids);

        // We have a use of a place; clone the current bindings for that place, and record them
        // as the live bindings for this use.
        let new_use = self.bindings_by_use.push(bindings);
        debug_assert_eq!(use_id, new_use);
    }

    pub(super) fn symbol_binding_definition_ids(
        &self,
        symbol: ScopedSymbolId,
    ) -> impl Iterator<Item = ScopedDefinitionId> + '_ {
        self.symbol_states[symbol]
            .state
            .bindings()
            .iter()
            .map(LiveBinding::binding)
    }

    pub(super) fn mark_binding_definitions_used(
        &mut self,
        binding_definition_ids: impl IntoIterator<Item = ScopedDefinitionId>,
    ) {
        self.mark_definition_ids_used(binding_definition_ids);
    }

    pub(super) fn record_range_reachability(
        &mut self,
        range: TextRange,
        is_type_checking_block: bool,
    ) {
        let this_range_info = RangeInfo {
            reachability: self.reachability,
            in_type_checking_block: is_type_checking_block,
        };

        // If the last entry has the same reachability constraint and the same
        // "in-TYPE_CHECKING" status, extend it to cover this range too, collapsing
        // consecutive statements in a contiguous range into a single entry.
        if let Some((last_range, last_range_info)) = self.range_reachability.last_mut()
            && *last_range_info == this_range_info
        {
            *last_range = last_range.cover(range);
            return;
        }
        self.range_reachability.push((range, this_range_info));
    }

    pub(super) fn snapshot_enclosing_state(
        &mut self,
        enclosing_place: ScopedPlaceId,
        enclosing_scope: ScopeKind,
        enclosing_place_expr: PlaceExprRef,
        is_parent_of_annotation_scope: bool,
    ) -> ScopedEnclosingSnapshotId {
        let pending = self.pending_reachability.current;
        let place_state = pending_place_state_mut(
            enclosing_place,
            &mut self.symbol_states,
            &mut self.member_states,
        );
        let bindings = self
            .pending_reachability
            .materialize_ref(place_state, pending, &mut self.reachability_constraints)
            .bindings();

        let is_class_symbol = enclosing_scope.is_class() && enclosing_place.is_symbol();
        let is_forwarding_symbol = enclosing_place_expr
            .as_symbol()
            .is_some_and(|symbol| symbol.is_global() || symbol.is_nonlocal());
        let stores_visible_bindings = enclosing_place_expr.is_bound()
            && bindings
                .iter()
                .any(|binding| !binding.binding().is_unbound());
        // Names bound in class scopes are never visible to nested scopes (but
        // attributes/subscripts are visible), so we never need to save eager scope bindings in a
        // class scope. There is one exception to this rule: annotation scopes can see names
        // defined in an immediately-enclosing class scope. Likewise, unbound `global` and
        // `nonlocal` symbols in the enclosing scope are forwarding declarations, so nested scopes
        // should continue walking outward instead of treating any bindings here as owned by this
        // scope. However, if the enclosing scope actually rebound the forwarded name, that visible
        // state needs to be snapshotted so nested scopes can see the rebound type.
        if (is_class_symbol && !is_parent_of_annotation_scope)
            || !enclosing_place_expr.is_bound()
            || (is_forwarding_symbol && !stores_visible_bindings)
        {
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
        let pending = self.pending_reachability.current;
        let new_bindings = self
            .pending_reachability
            .materialize_ref(
                &mut self.symbol_states[enclosing_symbol],
                pending,
                &mut self.reachability_constraints,
            )
            .bindings()
            .clone();
        match self.enclosing_snapshots.get_mut(snapshot_id) {
            Some(EnclosingSnapshot::Bindings(bindings)) => {
                bindings.merge(
                    new_bindings,
                    &mut self.narrowing_constraints,
                    &mut self.reachability_constraints,
                );
            }
            Some(EnclosingSnapshot::Constraint(constraint)) => {
                *constraint = ScopedNarrowingConstraint::ALWAYS_TRUE;
            }
            None => {}
        }
    }

    fn mark_definition_ids_used(
        &mut self,
        definition_ids: impl IntoIterator<Item = ScopedDefinitionId>,
    ) {
        for definition_id in definition_ids {
            self.mark_definition_used(definition_id);
        }
    }

    fn mark_definition_used(&mut self, definition_id: ScopedDefinitionId) {
        if definition_id.is_unbound() {
            return;
        }

        if matches!(
            self.all_definitions[definition_id],
            DefinitionState::Defined(_)
        ) {
            self.used_bindings[definition_id] = true;
        }
    }

    /// Take a snapshot of the current visible-places state.
    pub(super) fn snapshot(&self) -> FlowSnapshot {
        FlowSnapshot {
            symbol_states: self.symbol_states.clone(),
            member_states: self.member_states.clone(),
            reachability: self.reachability,
            pending_reachability: self.pending_reachability.current,
        }
    }

    /// Get the current live bindings for a place.
    pub(super) fn current_bindings(
        &mut self,
        place: ScopedPlaceId,
    ) -> impl Iterator<Item = LiveBinding> + '_ {
        let pending = self.pending_reachability.current;
        let place_state =
            pending_place_state_mut(place, &mut self.symbol_states, &mut self.member_states);
        let bindings = self
            .pending_reachability
            .materialize_ref(place_state, pending, &mut self.reachability_constraints)
            .bindings();

        bindings.iter().copied()
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
        self.pending_reachability.current = snapshot.pending_reachability;

        // If the snapshot we are restoring is missing some places we've recorded since, we need
        // to fill them in so the place IDs continue to line up. Since they don't exist in the
        // snapshot, the correct state to fill them in with is "undefined".
        let undefined = PendingPlaceState::new(
            PlaceState::undefined(self.reachability),
            self.pending_reachability.current,
        );
        self.symbol_states.resize(num_symbols, undefined.clone());
        self.member_states.resize(num_members, undefined);
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

        let branch = snapshot.pending_reachability;
        self.pending_reachability.merge_place_states(
            &mut self.symbol_states,
            snapshot.symbol_states,
            branch,
            snapshot.reachability,
            &mut self.narrowing_constraints,
            &mut self.reachability_constraints,
        );
        self.pending_reachability.merge_place_states(
            &mut self.member_states,
            snapshot.member_states,
            branch,
            snapshot.reachability,
            &mut self.narrowing_constraints,
            &mut self.reachability_constraints,
        );

        self.reachability = self
            .reachability_constraints
            .add_or_constraint(self.reachability, snapshot.reachability);
    }

    pub(super) fn finish(mut self: Box<Self>) -> UseDefMap<'db> {
        let pending = self.pending_reachability.current;
        for state in self
            .symbol_states
            .iter_mut()
            .chain(self.member_states.iter_mut())
        {
            self.pending_reachability.materialize(
                state,
                pending,
                &mut self.reachability_constraints,
            );
        }

        let place_state_count = self.symbol_states.len()
            + self.member_states.len()
            + self.reachable_symbol_definitions.len()
            + self.reachable_member_definitions.len();
        let definitions_with_declarations_count = self
            .definitions_by_definition
            .values()
            .filter(|definitions| definitions.declarations.is_some())
            .count();
        let interned_bindings_capacity = self.definitions_by_definition.len()
            + self.bindings_by_use.len()
            + self.enclosing_snapshots.len()
            + place_state_count;
        let interned_declarations_capacity =
            definitions_with_declarations_count + place_state_count;
        let interned_ids_by_declarations_capacity =
            definitions_with_declarations_count + self.member_states.len();
        let mut place_state_interner = PlaceStateInterner::with_capacity(
            interned_bindings_capacity,
            interned_ids_by_declarations_capacity,
            interned_declarations_capacity,
        );
        // These fields are manually interned because they have a statistically high duplication rate (>50%).
        let definitions_by_definition = Self::intern_definitions_by_definition(
            self.definitions_by_definition,
            &mut place_state_interner,
        );
        let bindings_by_use =
            Self::intern_bindings_by_use(self.bindings_by_use, &mut place_state_interner);
        let symbol_states = self
            .symbol_states
            .into_iter()
            .map(|state| Rc::unwrap_or_clone(state.state))
            .collect();
        let member_states = self
            .member_states
            .into_iter()
            .map(|state| Rc::unwrap_or_clone(state.state))
            .collect();
        let end_of_scope_symbols = Self::intern_place_states(
            symbol_states,
            PlaceState::into_parts,
            &mut place_state_interner,
        );
        let end_of_scope_members =
            Self::intern_end_of_scope_members(member_states, &mut place_state_interner);
        let reachable_definitions_by_symbol = Self::intern_place_states(
            self.reachable_symbol_definitions,
            |definitions| (definitions.bindings, definitions.declarations),
            &mut place_state_interner,
        );
        let reachable_definitions_by_member = Self::intern_place_states(
            self.reachable_member_definitions,
            |definitions| (definitions.bindings, definitions.declarations),
            &mut place_state_interner,
        );
        let enclosing_snapshots =
            Self::intern_enclosing_snapshots(self.enclosing_snapshots, &mut place_state_interner);
        let PlaceStateInterner {
            interned_bindings,
            interned_declarations,
            ..
        } = place_state_interner;

        // We only walk the fields that are copied through to the UseDefMap when we finish building
        // it.
        let interned_bindings = interned_bindings.finish(
            &mut self.narrowing_constraints,
            &mut self.reachability_constraints,
        );
        let interned_declarations =
            interned_declarations.finish(&mut self.reachability_constraints);
        for bindings in self.multi_bindings_by_use.values_mut().flatten() {
            bindings.finish(
                &mut self.narrowing_constraints,
                &mut self.reachability_constraints,
            );
        }
        // Keep default entries while building so they remain barriers between non-contiguous
        // ranges with the same metadata. Once construction is complete, absence represents the
        // default of reachable code outside a `TYPE_CHECKING` block.
        self.range_reachability
            .retain(|(_, info)| *info != RangeInfo::default());
        for &(_, RangeInfo { reachability, .. }) in &self.range_reachability {
            self.reachability_constraints.mark_used(reachability);
        }
        for enclosing_snapshot in &enclosing_snapshots {
            // Bindings are already marked above.
            if let InternedEnclosingSnapshotId::Constraint(constraint) = enclosing_snapshot {
                self.narrowing_constraints.mark_used(*constraint);
            }
        }
        self.reachability_constraints.mark_used(self.reachability);
        let symbol_states =
            Self::zip_place_states(end_of_scope_symbols, reachable_definitions_by_symbol);
        let member_states =
            Self::zip_place_states(end_of_scope_members, reachable_definitions_by_member);
        let multi_bindings_by_use = MultiBindingsByUse::from_map(self.multi_bindings_by_use);
        let loop_headers = self.loop_headers;
        let extra = (!bindings_by_use.is_empty()
            || !member_states.is_empty()
            || !enclosing_snapshots.is_empty()
            || !loop_headers.is_empty())
        .then(|| {
            Box::new(UseDefMapExtra {
                bindings_by_use: bindings_by_use.into(),
                multi_bindings_by_use,
                member_states,
                enclosing_snapshots: enclosing_snapshots.into(),
                loop_headers: loop_headers.into(),
            })
        });
        let predicates = self.predicates.build();
        let reachability_constraints = self.reachability_constraints.build();
        let narrowing_constraints = self.narrowing_constraints.build();
        let constraint_tables = (!reachability_constraints.used_interiors().is_empty()
            || !narrowing_constraints.is_empty())
        .then(|| {
            Box::new(ConstraintTables {
                predicates,
                reachability_constraints,
                narrowing_constraints,
            })
        });
        let all_definitions = RetainedDefinitions::new(
            self.all_definitions,
            self.used_bindings,
            self.used_multipart_imports,
        );

        UseDefMap {
            all_definitions,
            constraint_tables,
            interned_bindings,
            interned_declarations,
            range_reachability: self.range_reachability.into_boxed_slice(),
            symbol_states,
            definitions_by_definition,
            extra,
            end_of_scope_reachability: self.reachability,
        }
    }

    fn zip_place_states<I: Idx, T>(
        end_of_scope: IndexVec<I, T>,
        reachable: IndexVec<I, T>,
    ) -> FrozenIndexVec<I, RetainedPlaceStates<T>> {
        assert_eq!(end_of_scope.len(), reachable.len());

        end_of_scope
            .into_iter()
            .zip(reachable)
            .map(|(end_of_scope, reachable)| RetainedPlaceStates {
                end_of_scope,
                reachable,
            })
            .collect()
    }

    fn intern_definitions_by_definition(
        definitions_by_definition: FxHashMap<
            Definition<'db>,
            DefinitionsAtDefinition<Bindings, Declarations>,
        >,
        place_state_interner: &mut PlaceStateInterner,
    ) -> FrozenMap<
        Definition<'db>,
        DefinitionsAtDefinition<InternedBindingsId, InternedDeclarationsId>,
    > {
        let mut interned_ids_by_definition = Vec::with_capacity(definitions_by_definition.len());

        // Keep the builder map hash-based because it is updated for every definition. We only need
        // stable iteration here, where insertion order determines the generated interned IDs.
        let mut definitions_by_definition =
            definitions_by_definition.into_iter().collect::<Vec<_>>();
        definitions_by_definition.sort_unstable_by_key(|(definition, _)| *definition);

        for (
            definition,
            DefinitionsAtDefinition {
                bindings,
                declarations,
            },
        ) in definitions_by_definition
        {
            // Lookups use the shared start-of-scope defaults for these omitted entries.
            if bindings.is_always_unbound()
                && declarations
                    .as_ref()
                    .is_none_or(Declarations::is_always_undeclared)
            {
                continue;
            }

            let bindings = place_state_interner.intern_bindings(&bindings);
            let declarations = declarations
                .map(|declarations| place_state_interner.intern_declarations(declarations));
            interned_ids_by_definition.push((
                definition,
                DefinitionsAtDefinition {
                    bindings,
                    declarations,
                },
            ));
        }

        FrozenMap::from_entries(interned_ids_by_definition)
    }

    fn intern_bindings_by_use(
        bindings_by_use: IndexVec<ScopedUseId, Bindings>,
        place_state_interner: &mut PlaceStateInterner,
    ) -> IndexVec<ScopedUseId, InternedBindingsId> {
        let mut interned_ids_by_use: IndexVec<ScopedUseId, InternedBindingsId> =
            IndexVec::with_capacity(bindings_by_use.len());

        for bindings in bindings_by_use {
            let interned_id = place_state_interner.intern_bindings(&bindings);
            interned_ids_by_use.push(interned_id);
        }

        interned_ids_by_use
    }

    fn intern_place_states<I: Idx, T>(
        place_states: IndexVec<I, T>,
        get_parts: impl Fn(T) -> (Bindings, Declarations),
        place_state_interner: &mut PlaceStateInterner,
    ) -> IndexVec<I, InternedPlaceStateId> {
        let mut interned_ids_by_place = IndexVec::with_capacity(place_states.len());

        for place_state in place_states {
            let (bindings, declarations) = get_parts(place_state);
            let interned_id = place_state_interner.retain_place_state(&bindings, declarations);
            interned_ids_by_place.push(interned_id);
        }

        interned_ids_by_place
    }

    fn intern_end_of_scope_members(
        end_of_scope_members: IndexVec<ScopedMemberId, PlaceState>,
        place_state_interner: &mut PlaceStateInterner,
    ) -> IndexVec<ScopedMemberId, InternedPlaceStateId> {
        let mut interned_ids_by_member = IndexVec::with_capacity(end_of_scope_members.len());
        let mut interned_ids_by_place_state =
            FxHashMap::with_capacity_and_hasher(end_of_scope_members.len(), FxBuildHasher);

        for place_state in end_of_scope_members {
            let interned_id = match interned_ids_by_place_state.entry(place_state) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let place_state = entry.key();
                    let interned_id = place_state_interner.intern_place_state(
                        place_state.bindings(),
                        place_state.declarations().clone(),
                    );
                    entry.insert(interned_id);
                    interned_id
                }
            };
            interned_ids_by_member.push(interned_id);
        }

        interned_ids_by_member
    }

    fn intern_enclosing_snapshots(
        enclosing_snapshots: EnclosingSnapshots,
        place_state_interner: &mut PlaceStateInterner,
    ) -> IndexVec<ScopedEnclosingSnapshotId, InternedEnclosingSnapshotId> {
        let mut interned_ids_by_snapshot: IndexVec<
            ScopedEnclosingSnapshotId,
            InternedEnclosingSnapshotId,
        > = IndexVec::with_capacity(enclosing_snapshots.len());

        for snapshot in enclosing_snapshots {
            let interned_id = match snapshot {
                EnclosingSnapshot::Bindings(bindings) => {
                    let interned_bindings_id = place_state_interner.intern_bindings(&bindings);
                    InternedEnclosingSnapshotId::Bindings(interned_bindings_id)
                }
                EnclosingSnapshot::Constraint(constraint) => {
                    InternedEnclosingSnapshotId::Constraint(constraint)
                }
            };
            interned_ids_by_snapshot.push(interned_id);
        }

        interned_ids_by_snapshot
    }
}
