//! Constraints under which type properties hold
//!
//! For "concrete" types (which contain no type variables), type properties like assignability have
//! simple answers: one type is either assignable to another type, or it isn't. (The _rules_ for
//! comparing two particular concrete types can be rather complex, but the _answer_ is a simple
//! "yes" or "no".)
//!
//! These properties are more complex when type variables are involved, because there are (usually)
//! many different concrete types that a typevar can be specialized to, and the type property might
//! hold for some specializations, but not for others. That means that for types that include
//! typevars, "Is this type assignable to another?" no longer makes sense as a question. The better
//! question is: "Under what constraints is this type assignable to another?".
//!
//! This module provides the machinery for representing the "under what constraints" part of that
//! question.
//!
//! An individual constraint restricts the specialization of a single typevar to be within a
//! particular lower and upper bound. (A type is within a lower and upper bound if it is a
//! supertype of the lower bound and a subtype of the upper bound.) You can then build up more
//! complex constraint sets using union, intersection, and negation operations. We use a ternary
//! decision diagram (TDD), as described in §11.2 of [Duboc's thesis][duboc], to represent a
//! constraint set.
//!
//! A TDD is an extension of a binary decision diagram (BDD). Each interior node has three
//! outgoing edges instead of two:
//!
//! - `if_true`: taken when the constraint holds (called `C` by Duboc)
//! - `if_uncertain`: included regardless of the constraint's truth value (`U`)
//! - `if_false`: taken when the constraint does not hold (`D`)
//!
//! BDD and TDD nodes can be considered "if-then-else" or ternary operators:
//!
//! ```text
//! [BDD]  n? T: F    = (n ∧ T) ∨ (¬n ∧ F)
//! [TDD]  n? C: U: D = (n ∧ C) ∨ U ∨ (¬n ∧ D)
//! ```
//!
//! The key benefit of TDDs over BDDs is that unions are more efficient. When computing the union
//! of two TDDs with different root constraints, the second operand is "parked" in the uncertain
//! branch rather than duplicated into both the true and false branches. This avoids an
//! exponential blowup in diagram size that can occur when OR-ing together many constraint sets
//! (e.g., when inferring specializations for overloaded callables).
//!
//! When `if_uncertain` is `ALWAYS_FALSE` everywhere, the TDD degenerates to a standard BDD, and
//! all operations have zero overhead compared to the binary case.
//!
//! NOTE: This module is currently in a transitional state. We've added the BDD [`ConstraintSet`]
//! representation, and updated all of our property checks to build up a constraint set and then
//! check whether it is ever or always satisfiable, as appropriate. We are not yet inferring
//! specializations from those constraints.
//!
//! ### Examples
//!
//! For instance, in the following Python code:
//!
//! ```py
//! class A: ...
//! class B(A): ...
//!
//! def _[T: B](t: T) -> None: ...
//! def _[U: (int, str)](u: U) -> None: ...
//! ```
//!
//! The typevar `T` has an upper bound of `B`, which would translate into the constraint `Never ≤ T
//! ≤ B`. (Every type is a supertype of `Never`, so having `Never` as a lower bound means that
//! there is effectively no lower bound. Similarly, an upper bound of `object` means that there is
//! effectively no upper bound.) The `T ≤ B` part expresses that the type can specialize to any
//! type that is a subtype of B.
//!
//! The typevar `U` is constrained to be either `int` or `str`, which would translate into the
//! constraint `(int ≤ T ≤ int) ∪ (str ≤ T ≤ str)`. When the lower and upper bounds are the same,
//! the constraint says that the typevar must specialize to that _exact_ type, not to a subtype or
//! supertype of it.
//!
//! ### Tracing
//!
//! This module is instrumented with debug- and trace-level `tracing` messages. You can set the
//! `TY_LOG` environment variable to see this output when testing locally. `tracing` log messages
//! typically have a `target` field, which is the name of the module the message appears in — in
//! this case, `ty_python_semantic::types::constraints`. We add additional detail to these targets,
//! in case you only want to debug parts of the implementation. For instance, if you want to debug
//! how we construct sequent maps, you could use
//!
//! ```sh
//! env TY_LOG=ty_python_semantic::types::constraints::SequentMap=trace ty check ...
//! ```
//!
//! [duboc]: https://gldubc.github.io/#thesis

use std::cell::{Ref, RefCell};
use std::cmp::Ordering;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::ops::Range;

use indexmap::map::Entry;
use itertools::Itertools;
use ruff_index::{Idx, IndexVec, newtype_index};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::types::class::GenericAlias;
use crate::types::generics::InferableTypeVars;
use crate::types::typevar::{BoundTypeVarIdentity, walk_bound_type_var_type};
use crate::types::variance::VarianceInferable;
use crate::types::visitor::{
    TypeCollector, TypeVisitor, any_over_type, walk_type_with_recursion_guard,
};
use crate::types::{
    BoundTypeVarInstance, IntersectionType, Type, TypeVarBoundOrConstraints, TypeVarVariance,
    UnionType,
};
use crate::{Db, FxIndexMap, FxIndexSet};

/// An extension trait for building constraint sets from [`Option`] values.
pub(crate) trait OptionConstraintsExtension<T> {
    /// Returns a constraint set that is always satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_none_or<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c>;

    /// Returns a constraint set that is never satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_some_and<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c>;
}

impl<T> OptionConstraintsExtension<T> for Option<T> {
    fn when_none_or<'db, 'c>(
        self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::always(builder),
        }
    }

    fn when_some_and<'db, 'c>(
        self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::never(builder),
        }
    }
}

/// An extension trait for building constraint sets from an [`Iterator`].
pub(crate) trait IteratorConstraintsExtension<T> {
    /// Returns the constraints under which any element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_always_satisfied`][ConstraintSet::is_always_satisfied], then the overall result
    /// must be as well, and we stop consuming elements from the iterator.
    fn when_any<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c>;

    /// Returns the constraints under which every element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_never_satisfied`][ConstraintSet::is_never_satisfied], then the overall result
    /// must be as well, and we stop consuming elements from the iterator.
    fn when_all<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c>;
}

impl<I, T> IteratorConstraintsExtension<T> for I
where
    I: Iterator<Item = T>,
{
    fn when_any<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        mut f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let node = NodeId::distributed_or(
            db,
            builder,
            self.map(|element| {
                let constraint = f(element);
                constraint.verify_builder(builder);
                constraint.node
            }),
        );
        ConstraintSet::from_node(builder, node)
    }

    fn when_all<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        mut f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let node = NodeId::distributed_and(
            db,
            builder,
            self.map(|element| {
                let constraint = f(element);
                constraint.verify_builder(builder);
                constraint.node
            }),
        );
        ConstraintSet::from_node(builder, node)
    }
}

/// An owned copy of a [`ConstraintSet`]. Unlike [`ConstraintSet`], this type owns the storage
/// arenas that hold its BDD.
///
/// Owned constraint sets are immutable snapshots of a builder's arenas. They are used by
/// Salsa-cached relation queries, and by the
/// [`InternedConstraintSet`][crate::types::InternedConstraintSet] wrapper that lets us create and
/// operate on constraint sets in mdtests.
///
/// Note that you cannot interrogate an owned constraint set directly. Instead, use
/// [`query`][OwnedConstraintSet::query] to query it in a builder with matching arenas, or
/// [`load`][ConstraintSetBuilder::load] to remap it into an existing builder.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct OwnedConstraintSet<'db> {
    node: NodeId,
    constraints: IndexVec<ConstraintId, Constraint<'db>>,
    typevars: IndexVec<TypeVarId, BoundTypeVarIdentity<'db>>,
    nodes: IndexVec<NodeId, InteriorNodeData>,
}

impl Default for OwnedConstraintSet<'_> {
    fn default() -> Self {
        Self {
            node: ALWAYS_FALSE,
            constraints: IndexVec::default(),
            typevars: IndexVec::default(),
            nodes: IndexVec::default(),
        }
    }
}

impl<'db> OwnedConstraintSet<'db> {
    /// Loads this constraint set into a new builder, invokes a callback with that builder, and
    /// returns the result.
    ///
    /// This is more efficient than [`ConstraintSetBuilder::load`] when this is the only set you
    /// need to load into the new builder.
    pub(crate) fn query<F, R>(&self, f: F) -> R
    where
        F: for<'c> FnOnce(&'c ConstraintSetBuilder<'db>, ConstraintSet<'db, 'c>) -> R,
    {
        let constraint_cache = self
            .constraints
            .iter_enumerated()
            .map(|(id, constraint)| (*constraint, id))
            .collect();
        let typevar_cache = self
            .typevars
            .iter_enumerated()
            .map(|(id, bound_typevar)| (*bound_typevar, id))
            .collect();
        let node_cache = self
            .nodes
            .iter_enumerated()
            .map(|(id, interior)| (*interior, id))
            .collect();
        let storage = ConstraintSetStorage {
            constraints: self.constraints.clone(),
            typevars: self.typevars.clone(),
            nodes: self.nodes.clone(),
            constraint_cache,
            typevar_cache,
            node_cache,
            ..ConstraintSetStorage::default()
        };
        let builder = ConstraintSetBuilder {
            storage: RefCell::new(storage),
        };
        let set = ConstraintSet::from_node(&builder, self.node);
        f(&builder, set)
    }
}

/// A set of constraints under which a type property holds.
///
/// This is called a "set of constraint sets", and denoted _𝒮_, in [[POPL2015][]].
///
/// The underlying representation tracks the order that individual constraints are added to the
/// constraint set, which typically tracks when they appear in the underlying Python source. For
/// this to work, you should ensure that you call "combining" operators like [`and`][Self::and] and
/// [`or`][Self::or] in a consistent order.
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Copy)]
pub struct ConstraintSet<'db, 'c> {
    /// The BDD representing this constraint set
    node: NodeId,

    /// A reference to the builder that holds the storage for this constraint set's BDD
    builder: &'c ConstraintSetBuilder<'db>,

    /// Ensures that the `'c` lifetime is invariant
    _invariant: PhantomData<fn(&'c ()) -> &'c ()>,
}

impl<'db, 'c> ConstraintSet<'db, 'c> {
    fn from_node(builder: &'c ConstraintSetBuilder<'db>, node: NodeId) -> Self {
        Self {
            node,
            builder,
            _invariant: PhantomData,
        }
    }

    fn never(builder: &'c ConstraintSetBuilder<'db>) -> Self {
        Self::from_node(builder, ALWAYS_FALSE)
    }

    fn always(builder: &'c ConstraintSetBuilder<'db>) -> Self {
        Self::from_node(builder, ALWAYS_TRUE)
    }

    pub(crate) fn from_bool(builder: &'c ConstraintSetBuilder<'db>, b: bool) -> Self {
        if b {
            Self::always(builder)
        } else {
            Self::never(builder)
        }
    }

    /// Returns a constraint set that constraints a typevar to a particular range of types.
    pub(crate) fn constrain_typevar(
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Self {
        Self::from_node(
            builder,
            Constraint::new_node(db, builder, typevar, lower, upper),
        )
    }

    /// Verifies that this constraint set was created by `builder`
    #[track_caller]
    fn verify_builder(self, builder: &'c ConstraintSetBuilder<'db>) {
        debug_assert!(std::ptr::eq(self.builder, builder));
    }

    /// Returns whether this constraint set never holds
    pub(crate) fn is_never_satisfied(self, db: &'db dyn Db) -> bool {
        self.node.is_never_satisfied(db, self.builder)
    }

    /// Returns whether this constraint set always holds
    pub(crate) fn is_always_satisfied(self, db: &'db dyn Db) -> bool {
        self.node.is_always_satisfied(db, self.builder)
    }

    /// Returns the constraints under which `lhs` is a subtype of `rhs`, assuming that the
    /// constraints in this constraint set hold. Panics if neither of the types being compared are
    /// a typevar. (That case is handled by `Type::has_relation_to`.)
    pub(crate) fn implies_subtype_of(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        lhs: Type<'db>,
        rhs: Type<'db>,
    ) -> Self {
        self.verify_builder(builder);
        Self::from_node(builder, self.node.implies_subtype_of(db, builder, lhs, rhs))
    }

    /// Returns whether this constraint set is satisfied by all of the typevars that it mentions.
    ///
    /// Each typevar has a set of _valid specializations_, which is defined by any upper bound or
    /// constraints that the typevar has.
    ///
    /// Each typevar is also either _inferable_ or _non-inferable_. (You provide a list of the
    /// `inferable` typevars; all others are considered non-inferable.) For an inferable typevar,
    /// then there must be _some_ valid specialization that satisfies the constraint set. For a
    /// non-inferable typevar, then _all_ valid specializations must satisfy it.
    ///
    /// Note that we don't have to consider typevars that aren't mentioned in the constraint set,
    /// since the constraint set cannot be affected by any typevars that it does not mention. That
    /// means that those additional typevars trivially satisfy the constraint set, regardless of
    /// whether they are inferable or not.
    pub(crate) fn satisfied_by_all_typevars(
        &self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> bool {
        self.verify_builder(builder);
        self.node.satisfied_by_all_typevars(db, builder, inferable)
    }

    /// Updates this constraint set to hold the union of itself and another constraint set.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn union(
        &mut self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: Self,
    ) -> Self {
        self.verify_builder(builder);
        self.node = self.node.or_with_offset(builder, other.node);
        *self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn intersect(
        &mut self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: Self,
    ) -> Self {
        self.verify_builder(builder);
        self.node = self.node.and_with_offset(builder, other.node);
        *self
    }

    /// Returns the negation of this constraint set.
    pub(crate) fn negate(self, _db: &'db dyn Db, builder: &'c ConstraintSetBuilder<'db>) -> Self {
        self.verify_builder(builder);
        Self::from_node(builder, self.node.negate(builder))
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn and(
        mut self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> Self,
    ) -> Self {
        self.verify_builder(builder);
        if !self.is_never_satisfied(db) {
            let other = other();
            other.verify_builder(builder);
            self.intersect(db, builder, other);
        }
        self
    }

    /// Returns the union of this constraint set and another. The other constraint set is provided
    /// as a thunk, to implement short-circuiting: the thunk is not forced if the constraint set is
    /// already saturated.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn or(
        mut self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> Self,
    ) -> Self {
        self.verify_builder(builder);
        if !self.is_always_satisfied(db) {
            let other = other();
            other.verify_builder(builder);
            self.union(db, builder, other);
        }
        self
    }

    /// Returns a constraint set encoding that this constraint set implies another.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn implies(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> Self,
    ) -> Self {
        self.negate(db, builder).or(db, builder, other)
    }

    /// Returns a constraint set encoding that this constraint set is equivalent to another.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn iff(
        self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: Self,
    ) -> Self {
        self.verify_builder(builder);
        Self::from_node(builder, self.node.iff_with_offset(builder, other.node))
    }

    /// Reduces the set of inferable typevars for this constraint set. You provide an iterator of
    /// the typevars that were inferable when this constraint set was created, and which should be
    /// abstracted away. Those typevars will be removed from the constraint set, and the constraint
    /// set will return true whenever there was _any_ specialization of those typevars that
    /// returned true before.
    pub(crate) fn reduce_inferable(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        to_remove: impl IntoIterator<Item = BoundTypeVarIdentity<'db>>,
    ) -> Self {
        self.verify_builder(builder);
        Self::from_node(builder, self.node.exists(db, builder, to_remove))
    }

    pub(crate) fn remove_noninferable(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> Self {
        self.verify_builder(builder);
        Self::from_node(
            builder,
            self.node.remove_noninferable(db, builder, inferable),
        )
    }

    /// Computes solutions for each BDD path, using a caller-provided hook to select solutions.
    ///
    /// The `choose` hook is called for each typevar on each BDD path with the typevar's
    /// materialized lower and upper bounds. It returns:
    /// - `Some(ty)` to use `ty` as the solution for this typevar on this path
    /// - `None` to fall back to the default solution selection logic
    ///
    /// For multi-path BDDs, the hook is called per-path. The caller is responsible for combining
    /// results across paths (typically via union).
    pub(crate) fn solutions(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
    ) -> Solutions<'db> {
        self.verify_builder(builder);
        self.node.solutions(db, builder)
    }

    #[expect(dead_code)] // Keep this around for debugging purposes
    pub(crate) fn display(self, db: &'db dyn Db) -> impl Display {
        self.node
            .simplify_for_display(db, self.builder)
            .display(db, self.builder)
    }

    #[expect(dead_code)] // Keep this around for debugging purposes
    pub(crate) fn display_graph<'a>(
        self,
        db: &'db dyn Db,
        prefix: &'a dyn Display,
    ) -> impl Display + 'a
    where
        'db: 'a,
        'c: 'a,
    {
        self.node.display_graph(db, self.builder, prefix)
    }
}

impl Debug for ConstraintSet<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConstraintSet")
            .field("node", &self.node)
            .finish()
    }
}

/// Holds the storage for the BDD structure of a related collection of constraint sets.
///
/// This is usually passed around by shared reference to avoid convoluted APIs that thread mutable
/// references to the builder back and forth.
///
/// All of our BDD algorithms rely heavily on interning and memoization, for both correctness and
/// efficiency. These caches are only unique within the context of a particular builder. We do not
/// cache globally across the entire ty process. (The main reason is to avoid any dependencies on
/// the particular order in which files or expressions are visited during type checking. A minor
/// additional benefit is that the builder does not need to be thread-safe or impl [`Sync`].)
///
/// Most core type inference algorithms create a builder, create one or more constraint sets in the
/// builder, interrogate those constraint sets, and then throw the builder away.
///
/// TODO: We are considering creating a single builder in `TypeInferenceBuilder` that would be
/// shared across an entire inference region. That would give us even more sharing opportunities,
/// which could be highly impactful, since it's likely that there will be types and constraints
/// that are repeated within a region. It should still give us the stability that we need, because
/// once we determine that we need _something_ from an inference regions, we always infer _all_ of
/// the definitions and expressions in that region, in a stable order.
#[derive(Default)]
pub(crate) struct ConstraintSetBuilder<'db> {
    storage: RefCell<ConstraintSetStorage<'db>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, get_size2::GetSize)]
struct ConstraintSetStorage<'db> {
    /// Constraints are the variables of our BDD. They are interned to give them a space-efficient
    /// identity. Constraints are added to this arena as they are encountered when constructing
    /// constraint sets. The ordering within the arena defines the BDD variable ordering in our BDD
    /// structures.
    constraints: IndexVec<ConstraintId, Constraint<'db>>,

    /// Typevars are interned so that they have a stable ordering within this builder, which does
    /// not depend on their salsa IDs. (The salsa IDs are not stable, since each typevar can be
    /// used (possibly indirectly) in expressions in different files, and there are no guarantees
    /// about the order or the speed that we process each file.)
    ///
    /// The ordering of typevars within this arena defines which typevars can be the lower/upper
    /// bounds of another (e.g., whether we encode `T ≤ U` as `Never ≤ T ≤ U` or `T ≤ U ≤ object`).
    typevars: IndexVec<TypeVarId, BoundTypeVarIdentity<'db>>,

    /// The BDD nodes that appear in any of the constraint sets constructed in this builder.
    nodes: IndexVec<NodeId, InteriorNodeData>,

    // Everything below are the memoization tables for the arenas and for our BDD operations.
    constraint_cache: FxHashMap<Constraint<'db>, ConstraintId>,
    typevar_cache: FxHashMap<BoundTypeVarIdentity<'db>, TypeVarId>,
    node_cache: FxHashMap<InteriorNodeData, NodeId>,

    negate_cache: FxHashMap<NodeId, NodeId>,
    or_cache: FxHashMap<(NodeId, NodeId, usize), NodeId>,
    and_cache: FxHashMap<(NodeId, NodeId, usize), NodeId>,
    exists_one_cache: FxHashMap<(NodeId, BoundTypeVarIdentity<'db>), NodeId>,
    retain_one_cache: FxHashMap<(NodeId, BoundTypeVarIdentity<'db>), NodeId>,
    restrict_one_cache: FxHashMap<(NodeId, ConstraintAssignment), (NodeId, bool)>,
    simplify_cache: FxHashMap<NodeId, NodeId>,

    single_sequent_cache: FxHashMap<ConstraintId, SequentMap>,
    pair_sequent_cache: FxHashMap<(ConstraintId, ConstraintId), SequentMap>,
}

impl<'db> ConstraintSetBuilder<'db> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Creates an [`OwnedConstraintSet`], consuming this builder in the process. You provide a
    /// callback that constructs a [`ConstraintSet`]. We then package that constraint set up with
    /// the storage arenas from this builder.
    pub(crate) fn into_owned(
        self,
        f: impl for<'c> FnOnce(&'c Self) -> ConstraintSet<'db, 'c>,
    ) -> OwnedConstraintSet<'db> {
        // NOTE: We do not store any of the builder's memoization caches in the result. Owned
        // constraint sets can only be used by adding them to a new builder. Doing so adds copies
        // of the constraints and nodes to the new builder, since they might overlap with
        // constraints and nodes that already exist there. That means the memoization caches from
        // the original builder aren't relevant to the new builder, and don't need to be retained.
        let constraint = f(&self);
        let node = constraint.node;
        let storage = self.storage.into_inner();
        OwnedConstraintSet {
            node,
            constraints: storage.constraints,
            typevars: storage.typevars,
            nodes: storage.nodes,
        }
    }

    /// Loads an [`OwnedConstraintSet`] into this builder.
    ///
    /// The BDD structure inside a builder depends on the ordering of constraints and typevars in
    /// the builder's arenas. (The constraint ordering defines the BDD variable ordering, while the
    /// typevar ordering defines which typevars can be lower/upper bounds of other typevars.) There
    /// is no guarantee that the `OwnedConstraintSet` and this builder have consistent orderings,
    /// so we have to just reload everything, standardizing on _this_ builder's orderings. That's
    /// not the quickest thing in the world, but that is usually an acceptable tradeoff. Prefer
    /// `OwnedConstraintSet::query` when you only need to query a single owned set, since that
    /// avoids remapping and preserves the original TDD structure.
    pub(crate) fn load<'c>(
        &'c self,
        db: &'db dyn Db,
        other: &OwnedConstraintSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        fn rebuild_node<'db>(
            builder: &ConstraintSetBuilder<'db>,
            other: &OwnedConstraintSet<'db>,
            constraints: &IndexVec<ConstraintId, NodeId>,
            cache: &mut FxHashMap<NodeId, NodeId>,
            old_node: NodeId,
        ) -> NodeId {
            if old_node.is_terminal() {
                return old_node;
            }
            if let Some(remapped) = cache.get(&old_node) {
                return *remapped;
            }

            let old_interior = other.nodes[old_node];
            let if_true = rebuild_node(builder, other, constraints, cache, old_interior.if_true);
            let if_uncertain = rebuild_node(
                builder,
                other,
                constraints,
                cache,
                old_interior.if_uncertain,
            );
            let if_false = rebuild_node(builder, other, constraints, cache, old_interior.if_false);
            // `Constraint::new_node` creates standalone nodes whose source order starts at 1.
            // Shift the reloaded condition back to the source order recorded in the owned set;
            // solution extraction uses this order for deterministic unions and intersections.
            let condition = constraints[old_interior.constraint]
                .with_adjusted_source_order(builder, old_interior.source_order.saturating_sub(1));
            let remapped = condition.ite_uncertain(builder, if_true, if_uncertain, if_false);

            cache.insert(old_node, remapped);
            remapped
        }

        // Load all of the constraints into the this builder first, to maximize the chance that the
        // constraints and typevars will appear in the same order. (This is important because many
        // of our mdtests try to force a particular ordering, to test that our algorithms are all
        // order-independent.)
        let constraints = other
            .constraints
            .iter()
            .map(|old_constraint| {
                Constraint::new_node(
                    db,
                    self,
                    old_constraint.typevar,
                    old_constraint.lower,
                    old_constraint.upper,
                )
            })
            .collect();

        // Maps NodeIds in the OwnedConstraintSet to the corresponding NodeIds in this builder.
        let mut cache = FxHashMap::default();
        let node = rebuild_node(self, other, &constraints, &mut cache, other.node);
        ConstraintSet::from_node(self, node)
    }

    /// Interns a single typevar, giving it a stable order in this builder
    fn intern_typevar(&self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarId {
        let identity = typevar.identity(db);
        let mut storage = self.storage.borrow_mut();
        if let Some(id) = storage.typevar_cache.get(&identity) {
            return *id;
        }
        let id = storage.typevars.push(identity);
        storage.typevar_cache.insert(identity, id);
        id
    }

    /// Interns all of the typevars mentioned in a type in a stable order.
    fn intern_mentioned_typevars_in_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        struct InternMentionedTypevars<'a, 'db> {
            builder: &'a ConstraintSetBuilder<'db>,
            recursion_guard: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for InternMentionedTypevars<'_, 'db> {
            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_bound_type_var_type(
                &self,
                db: &'db dyn Db,
                bound_typevar: BoundTypeVarInstance<'db>,
            ) {
                self.builder.intern_typevar(db, bound_typevar);
                walk_bound_type_var_type(db, bound_typevar, self);
            }

            fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
                for ty in alias.specialization(db).types(db) {
                    self.visit_type(db, *ty);
                }
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        InternMentionedTypevars {
            builder: self,
            recursion_guard: TypeCollector::default(),
        }
        .visit_type(db, ty);
    }

    /// Interns all of the typevars mentioned in a constraint in a stable order.
    fn intern_constraint_typevars(
        &self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) {
        self.intern_typevar(db, typevar);
        self.intern_mentioned_typevars_in_type(db, lower);
        self.intern_mentioned_typevars_in_type(db, upper);
    }

    fn intern_constraint(&self, db: &'db dyn Db, data: Constraint<'db>) -> ConstraintId {
        self.intern_constraint_typevars(db, data.typevar, data.lower, data.upper);

        let mut storage = self.storage.borrow_mut();
        if let Some(id) = storage.constraint_cache.get(&data) {
            return *id;
        }
        let id = storage.constraints.push(data);
        storage.constraint_cache.insert(data, id);
        id
    }

    fn intern_interior_node(&self, data: InteriorNodeData) -> NodeId {
        let mut storage = self.storage.borrow_mut();
        if let Some(id) = storage.node_cache.get(&data) {
            return *id;
        }
        let id = storage.nodes.push(data);
        storage.node_cache.insert(data, id);
        id
    }

    fn typevar_id(&self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarId {
        let identity = typevar.identity(db);
        self.storage
            .borrow()
            .typevar_cache
            .get(&identity)
            .copied()
            .expect("typevar should be interned before ordering")
    }

    fn constraint_data(&self, constraint: ConstraintId) -> Constraint<'db> {
        self.storage.borrow().constraints[constraint]
    }

    fn interior_node_data(&self, node: NodeId) -> InteriorNodeData {
        self.storage.borrow().nodes[node]
    }
}

impl<'db> BoundTypeVarInstance<'db> {
    /// Returns whether this typevar can be the lower or upper bound of another typevar in a
    /// constraint set.
    ///
    /// We enforce an (arbitrary) ordering on typevars, and ensure that the bounds of a constraint
    /// are "later" according to that order than the typevar being constrained. Having an order
    /// ensures that we can build up transitive relationships between constraints without incurring
    /// any cycles. This particular ordering plays nicely with how we are ordering constraints
    /// within a BDD — it means that if a typevar has another typevar as a bound, all of the
    /// constraints that apply to the bound will appear lower in the BDD.
    fn can_be_bound_for(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        typevar: Self,
    ) -> bool {
        builder.typevar_id(db, self).index() < builder.typevar_id(db, typevar).index()
    }
}

#[derive(Clone, Copy, Debug)]
enum IntersectionResult<'db> {
    Simplified(Constraint<'db>),
    CannotSimplify,
    Disjoint,
}

impl IntersectionResult<'_> {
    fn is_disjoint(self) -> bool {
        matches!(self, IntersectionResult::Disjoint)
    }
}

/// The index of a bound typevar within a [`ConstraintSetStorage`].
#[newtype_index]
#[derive(salsa::Update, get_size2::GetSize)]
pub struct TypeVarId;

/// The index of an individual constraint (i.e. a BDD variable) within a [`ConstraintSetStorage`].
#[newtype_index]
#[derive(salsa::Update, get_size2::GetSize)]
pub struct ConstraintId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
enum NestedSubstitutionSide {
    Lower,
    Upper,
}

/// Identifies one nested-typevar substitution that has been applied while saturating a single
/// BDD path.
///
/// We key this by the typevar of the constrained constraint (which stays the same across an
/// entire chain of derivations against a single root constraint), the typevar that we substitute
/// _for_, and the side. We deliberately do _not_ key by the constraint id we substitute into,
/// because each nested substitution produces a new derived constraint, and if we keyed by that
/// id the next derivation step would have a different id and the repeat-guard would never fire.
/// The pathological cases that matter for performance involve repeated wrapping (e.g.
/// `Iterable[...]` layers) that keeps producing ever-deeper replacement types while targeting
/// the same constrained typevar; keying by the constrained typevar plus the substituted typevar
/// lets [`PathAssignments`] apply each substitution shape at most once per path.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
struct NestedSubstitution {
    /// NOTE: Keying `NestedSubstitution` by `constrained_typevar` instead of the specific constrained `ConstraintId` makes
    /// deduplication apply across all constraints on that typevar.
    /// This provides a performance benefit, but may weaken sequent saturation and can miss contradictions (or other implications) that depend on keeping both substitutions.
    /// However, at present, there don't seem to be any cases where this is a problem (see ruff#24803 for details).
    constrained_typevar: TypeVarId,
    substituted_typevar: TypeVarId,
    side: NestedSubstitutionSide,
}

/// A constraint derived from the sequent map, optionally annotated with the nested substitution
/// step that produced it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
struct DerivedConstraint {
    constraint: ConstraintId,
    nested_substitution: Option<NestedSubstitution>,
}

fn nested_substitution<'db>(
    db: &'db dyn Db,
    builder: &ConstraintSetBuilder<'db>,
    constrained_typevar: BoundTypeVarInstance<'db>,
    substituted_typevar: BoundTypeVarInstance<'db>,
    side: NestedSubstitutionSide,
) -> NestedSubstitution {
    NestedSubstitution {
        constrained_typevar: builder.typevar_id(db, constrained_typevar),
        substituted_typevar: builder.typevar_id(db, substituted_typevar),
        side,
    }
}

/// An individual constraint in a constraint set. This restricts a single typevar to be within a
/// lower and upper bound.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct Constraint<'db> {
    pub(crate) typevar: BoundTypeVarInstance<'db>,
    pub(crate) lower: Type<'db>,
    pub(crate) upper: Type<'db>,
}

impl ConstraintId {
    fn new<'db>(
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> ConstraintId {
        builder.intern_constraint(
            db,
            Constraint {
                typevar,
                lower,
                upper,
            },
        )
    }
}

impl<'db> Constraint<'db> {
    /// Returns a new range constraint.
    ///
    /// Panics if `lower` and `upper` are not both fully static.
    fn new_node(
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        mut lower: Type<'db>,
        mut upper: Type<'db>,
    ) -> NodeId {
        // It's not useful for an upper bound to be an intersection type, or for a lower bound to
        // be a union type. Because the following equivalences hold, we can break these bounds
        // apart and create an equivalent BDD with more nodes but simpler constraints. (Fewer,
        // simpler constraints mean that our sequent maps won't grow pathologically large.)
        //
        //   T ≤ (α & β)   ⇔ (T ≤ α) ∧ (T ≤ β)
        //   T ≤ (¬α & ¬β) ⇔ (T ≤ ¬α) ∧ (T ≤ ¬β)
        //   (α | β) ≤ T   ⇔ (α ≤ T) ∧ (β ≤ T)
        if let Type::Union(lower_union) = lower {
            let mut result = ALWAYS_TRUE;
            for lower_element in lower_union.elements(db) {
                result = result.and_with_offset(
                    builder,
                    Constraint::new_node(db, builder, typevar, *lower_element, upper),
                );
            }
            return result;
        }
        // A negated type ¬α is represented as an intersection with no positive elements, and a
        // single negative element. We _don't_ want to treat that an "intersection" for the
        // purposes of simplifying upper bounds.
        if let Type::Intersection(upper_intersection) = upper
            && !upper_intersection.is_simple_negation(db)
        {
            let mut result = ALWAYS_TRUE;
            for upper_element in upper_intersection.iter_positive(db) {
                result = result.and_with_offset(
                    builder,
                    Constraint::new_node(db, builder, typevar, lower, upper_element),
                );
            }
            for upper_element in upper_intersection.iter_negative(db) {
                result = result.and_with_offset(
                    builder,
                    Constraint::new_node(db, builder, typevar, lower, upper_element.negate(db)),
                );
            }
            return result;
        }

        // Two identical typevars must always solve to the same type, so it is not useful to have
        // an upper or lower bound that is the typevar being constrained.
        match lower {
            Type::TypeVar(lower_bound_typevar)
                if typevar.is_same_typevar_as(db, lower_bound_typevar) =>
            {
                lower = Type::Never;
            }
            Type::Intersection(intersection)
                if intersection.positive(db).iter().any(|element| {
                    element.as_typevar().is_some_and(|element_bound_typevar| {
                        typevar.is_same_typevar_as(db, element_bound_typevar)
                    })
                }) =>
            {
                lower = Type::Never;
            }
            Type::Intersection(intersection)
                if intersection.negative(db).iter().any(|element| {
                    element.as_typevar().is_some_and(|element_bound_typevar| {
                        typevar.is_same_typevar_as(db, element_bound_typevar)
                    })
                }) =>
            {
                return Node::new_constraint(
                    builder,
                    ConstraintId::new(db, builder, typevar, Type::Never, Type::object()),
                    1,
                )
                .negate(builder);
            }
            _ => {}
        }
        match upper {
            Type::TypeVar(upper_bound_typevar)
                if typevar.is_same_typevar_as(db, upper_bound_typevar) =>
            {
                upper = Type::object();
            }
            Type::Union(union)
                if union.elements(db).iter().any(|element| {
                    element.as_typevar().is_some_and(|element_bound_typevar| {
                        typevar.is_same_typevar_as(db, element_bound_typevar)
                    })
                }) =>
            {
                upper = Type::object();
            }
            _ => {}
        }

        builder.intern_constraint_typevars(db, typevar, lower, upper);

        // If `lower ≰ upper` for every possible assignment of typevars, then the constraint cannot
        // be satisfied, since there is no type that is both greater than `lower`, and less than
        // `upper`. We use an existential check here ("is there *some* assignment where
        // `lower ≤ upper`?") rather than a universal check, because the bounds may mention
        // typevars — e.g., `Sequence[int] ≤ A ≤ Sequence[T]` is satisfiable when `int ≤ T`.
        if lower
            .when_constraint_set_assignable_to(db, upper, builder)
            .is_never_satisfied(db)
        {
            return ALWAYS_FALSE;
        }

        // We have an (arbitrary) ordering for typevars. If the upper and/or lower bounds are
        // typevars, we have to ensure that the bounds are "later" according to that order than the
        // typevar being constrained.
        //
        // In the comments below, we use brackets to indicate which typevar is "earlier", and
        // therefore the typevar that the constraint applies to.
        match (lower, upper) {
            // L ≤ T ≤ L == (T ≤ [L] ≤ T)
            (Type::TypeVar(lower), Type::TypeVar(upper)) if lower.is_same_typevar_as(db, upper) => {
                let (bound, typevar) = if lower.can_be_bound_for(db, builder, typevar) {
                    (lower, typevar)
                } else {
                    (typevar, lower)
                };
                Node::new_constraint(
                    builder,
                    ConstraintId::new(
                        db,
                        builder,
                        typevar,
                        Type::TypeVar(bound),
                        Type::TypeVar(bound),
                    ),
                    1,
                )
            }

            // L ≤ T ≤ U == ([L] ≤ T) && (T ≤ [U])
            (Type::TypeVar(lower), Type::TypeVar(upper))
                if typevar.can_be_bound_for(db, builder, lower)
                    && typevar.can_be_bound_for(db, builder, upper) =>
            {
                let lower = Node::new_constraint(
                    builder,
                    ConstraintId::new(db, builder, lower, Type::Never, Type::TypeVar(typevar)),
                    1,
                );
                let upper = Node::new_constraint(
                    builder,
                    ConstraintId::new(db, builder, upper, Type::TypeVar(typevar), Type::object()),
                    1,
                );
                lower.and(builder, upper)
            }

            // L ≤ T ≤ U == ([L] ≤ T) && ([T] ≤ U)
            (Type::TypeVar(lower), _) if typevar.can_be_bound_for(db, builder, lower) => {
                let lower = Node::new_constraint(
                    builder,
                    ConstraintId::new(db, builder, lower, Type::Never, Type::TypeVar(typevar)),
                    1,
                );
                let upper = if upper.is_object() {
                    ALWAYS_TRUE
                } else {
                    Constraint::new_node(db, builder, typevar, Type::Never, upper)
                };
                lower.and(builder, upper)
            }

            // L ≤ T ≤ U == (L ≤ [T]) && (T ≤ [U])
            (_, Type::TypeVar(upper)) if typevar.can_be_bound_for(db, builder, upper) => {
                let lower = if lower.is_never() {
                    ALWAYS_TRUE
                } else {
                    Constraint::new_node(db, builder, typevar, lower, Type::object())
                };
                let upper = Node::new_constraint(
                    builder,
                    ConstraintId::new(db, builder, upper, Type::TypeVar(typevar), Type::object()),
                    1,
                );
                lower.and(builder, upper)
            }

            _ => Node::new_constraint(
                builder,
                ConstraintId::new(db, builder, typevar, lower, upper),
                1,
            ),
        }
    }
}

impl ConstraintId {
    fn when_true(self) -> ConstraintAssignment {
        ConstraintAssignment::Positive(self)
    }

    fn when_false(self) -> ConstraintAssignment {
        ConstraintAssignment::Negative(self)
    }

    fn when_unconstrained(self) -> ConstraintAssignment {
        ConstraintAssignment::Unconstrained(self)
    }

    /// Defines the ordering of the variables in a constraint set BDD.
    ///
    /// If we only care about _correctness_, we can choose any ordering that we want, as long as
    /// it's consistent. However, different orderings can have very different _performance_
    /// characteristics. Many BDD libraries attempt to reorder variables on the fly while building
    /// and working with BDDs. We don't do that, but we have tried to make some simple choices that
    /// have clear wins.
    ///
    /// In particular, we use the order that constraints are added to this builder. This gives us
    /// an ordering that is stable across runs, and which is not influenced by when and how quickly
    /// we analyze the other files in the project.
    ///
    /// As an optimization, we also _reverse_ this ordering, so that constraints that appear
    /// earlier in the arena appear "lower" (closer to the terminal nodes) in the BDD. Since we
    /// build up BDDs by combining smaller BDDs (which will have been constructed from expressions
    /// earlier in the source), this tends to minimize the amount of "node shuffling" that we have
    /// to do when combining BDDs.
    ///
    /// Previously, we tried to be more clever — for instance, by comparing the typevars of each
    /// constraint first, in an attempt to keep all of the constraints for a single typevar
    /// adjacent in the BDD structure. However, this proved to be counterproductive; we've found
    /// empirically that we get smaller BDDs with an ordering that is more aligned with source
    /// order.
    fn ordering(self) -> impl Ord {
        std::cmp::Reverse(self.index())
    }

    /// Returns whether this constraint implies another — i.e., whether every type that
    /// satisfies this constraint also satisfies `other`.
    ///
    /// This is used to simplify how we display constraint sets, by removing redundant constraints
    /// from a clause.
    fn implies<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        other: Self,
    ) -> bool {
        let self_constraint = builder.constraint_data(self);
        let other_constraint = builder.constraint_data(other);
        if !self_constraint
            .typevar
            .is_same_typevar_as(db, other_constraint.typevar)
        {
            return false;
        }
        other_constraint
            .lower
            .is_constraint_set_assignable_to(db, self_constraint.lower)
            && self_constraint
                .upper
                .is_constraint_set_assignable_to(db, other_constraint.upper)
    }

    /// Returns the intersection of two range constraints, or `None` if the intersection is empty.
    fn intersect<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        other: Self,
    ) -> IntersectionResult<'db> {
        /// TODO: For now, we treat some upper bounds as unsimplifiable if they become "too big".
        /// When intersecting constraints, the upper bounds are also intersected together. If the
        /// lhs and rhs upper bounds are unions of intersections (e.g. `(a & b) | (c & d)`), then
        /// intersecting them together will require distributing across every pair of union
        /// elements. That can quickly balloon in size. We are looking at a better representation
        /// that would let us model this case more directly, but for now, we punt.
        const MAX_UPPER_BOUND_SIZE: usize = 4;

        let self_constraint = builder.constraint_data(self);
        let other_constraint = builder.constraint_data(other);
        let estimated_upper_bound_size = self_constraint
            .upper
            .union_size(db)
            .saturating_mul(other_constraint.upper.union_size(db))
            .saturating_mul(
                self_constraint
                    .upper
                    .intersection_size(db)
                    .saturating_add(other_constraint.upper.intersection_size(db)),
            );
        if estimated_upper_bound_size >= MAX_UPPER_BOUND_SIZE {
            return IntersectionResult::CannotSimplify;
        }

        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α ≤ (t₁ ∩ t₂))
        let lower = UnionType::from_two_elements(db, self_constraint.lower, other_constraint.lower);
        let upper =
            IntersectionType::from_two_elements(db, self_constraint.upper, other_constraint.upper);

        // If `lower ≰ upper` for every possible assignment of typevars, then the intersection is
        // empty, since there is no type that is both greater than `lower`, and less than `upper`.
        // We use an existential check here ("is there *some* assignment where `lower ≤ upper`?")
        // rather than a universal check ("is `lower ≤ upper` for *all* assignments?"), because the
        // bounds may mention typevars — e.g., `Sequence[int] ≤ A ≤ Sequence[T]` is satisfiable
        // when `int ≤ T`, even though it's not universally true for all `T`.
        if lower
            .when_constraint_set_assignable_to(db, upper, builder)
            .is_never_satisfied(db)
        {
            return IntersectionResult::Disjoint;
        }

        // We do not create lower bounds that are unions, or upper bounds that are intersections,
        // since those can be broken apart into BDDs over simpler constraints.
        if lower.is_union() || upper.is_nontrivial_intersection(db) {
            return IntersectionResult::CannotSimplify;
        }

        IntersectionResult::Simplified(Constraint {
            typevar: self_constraint.typevar,
            lower,
            upper,
        })
    }

    pub(crate) fn display<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
    ) -> impl Display {
        self.when_true().display(db, builder)
    }
}

/// The index of a BDD node within a [`ConstraintSetBuilder`].
///
/// The "variables" of a constraint set BDD are individual constraints, represented by an interned
/// [`Constraint`].
///
/// Terminal nodes (`false` and `true`) have hard-coded IDs. Interior nodes are stored in a
/// [`ConstraintSetBuilder`], and are represented by the index into the storage array. By
/// construction, interior nodes can only refer to nodes with smaller indexes (since the nodes that
/// outgoing edges point at must already exist).
///
/// BDD nodes are _quasi-reduced_, which means that there are no duplicate nodes (which we handle
/// via Salsa interning). Unlike the typical BDD representation, which is (fully) reduced, we do
/// allow redundant nodes, with `if_true` and `if_false` edges that point at the same node. That
/// means that our BDDs "remember" all of the individual constraints that they were created with.
///
/// BDD nodes are also _ordered_, meaning that every path from the root of a BDD to a terminal node
/// visits variables in the same order. [`ConstraintId::ordering`] defines the variable
/// ordering that we use for constraint set BDDs.
///
/// In addition to this BDD variable ordering, we also track a `source_order` for each individual
/// constraint. This records the order in which constraints are added to the constraint set, which
/// typically tracks when they appear in the underlying Python source code. This provides an
/// ordering that is stable across multiple runs, for consistent test and diagnostic output. (We
/// cannot use this ordering as our BDD variable ordering, since we calculate it from already
/// constructed BDDs, and we need the BDD variable ordering to be fixed and available before
/// construction starts.)
#[derive(Clone, Copy, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
struct NodeId(u32);

/// A special ID that is used for an "always true" / "always visible" constraint.
const ALWAYS_TRUE: NodeId = NodeId(0xffff_ffff);

/// A special ID that is used for an "always false" / "never visible" constraint.
const ALWAYS_FALSE: NodeId = NodeId(0xffff_fffe);

const SMALLEST_TERMINAL: NodeId = ALWAYS_FALSE;

enum Node {
    AlwaysTrue,
    AlwaysFalse,
    Interior(InteriorNode),
}

impl NodeId {
    /// Creates a new BDD node, ensuring that it is quasi-reduced.
    fn new(
        builder: &ConstraintSetBuilder<'_>,
        constraint: ConstraintId,
        if_true: NodeId,
        if_false: NodeId,
        source_order: usize,
    ) -> NodeId {
        Self::with_uncertain(
            builder,
            constraint,
            if_true,
            ALWAYS_FALSE,
            if_false,
            source_order,
        )
    }

    /// Creates a new TDD node with an explicit `if_uncertain` branch, ensuring that it is
    /// quasi-reduced.
    fn with_uncertain(
        builder: &ConstraintSetBuilder<'_>,
        constraint: ConstraintId,
        if_true: NodeId,
        if_uncertain: NodeId,
        if_false: NodeId,
        source_order: usize,
    ) -> NodeId {
        debug_assert!(
            if_true
                .root_constraint(builder)
                .is_none_or(|root_constraint| {
                    root_constraint.ordering() > constraint.ordering()
                })
        );
        debug_assert!(
            if_uncertain
                .root_constraint(builder)
                .is_none_or(|root_constraint| {
                    root_constraint.ordering() > constraint.ordering()
                })
        );
        debug_assert!(
            if_false
                .root_constraint(builder)
                .is_none_or(|root_constraint| {
                    root_constraint.ordering() > constraint.ordering()
                })
        );
        if if_true == ALWAYS_FALSE && if_uncertain == ALWAYS_FALSE && if_false == ALWAYS_FALSE {
            return ALWAYS_FALSE;
        }
        let max_source_order = source_order
            .max(if_true.max_source_order(builder))
            .max(if_uncertain.max_source_order(builder))
            .max(if_false.max_source_order(builder));
        builder.intern_interior_node(InteriorNodeData {
            constraint,
            if_true,
            if_uncertain,
            if_false,
            source_order,
            max_source_order,
        })
    }
}

impl Node {
    /// Creates a new BDD node for an individual constraint. (The BDD will evaluate to `true` when
    /// the constraint holds, and to `false` when it does not.)
    fn new_constraint(
        builder: &ConstraintSetBuilder<'_>,
        constraint: ConstraintId,
        source_order: usize,
    ) -> NodeId {
        builder.intern_interior_node(InteriorNodeData {
            constraint,
            if_true: ALWAYS_TRUE,
            if_uncertain: ALWAYS_FALSE,
            if_false: ALWAYS_FALSE,
            source_order,
            max_source_order: source_order,
        })
    }

    /// Creates a new BDD node for a positive, negative, or unconstrained individual constraint.
    /// (For a positive constraint, this returns the same BDD node as
    /// [`new_constraint`][Self::new_constraint]. For a negative constraint, it returns the
    /// negation of that BDD node. For an unconstrained constraint, the result holds regardless
    /// of the constraint's truth value.)
    fn new_satisfied_constraint(
        builder: &ConstraintSetBuilder<'_>,
        constraint: ConstraintAssignment,
        source_order: usize,
    ) -> NodeId {
        match constraint {
            ConstraintAssignment::Positive(constraint) => {
                builder.intern_interior_node(InteriorNodeData {
                    constraint,
                    if_true: ALWAYS_TRUE,
                    if_uncertain: ALWAYS_FALSE,
                    if_false: ALWAYS_FALSE,
                    source_order,
                    max_source_order: source_order,
                })
            }
            ConstraintAssignment::Negative(constraint) => {
                builder.intern_interior_node(InteriorNodeData {
                    constraint,
                    if_true: ALWAYS_FALSE,
                    if_uncertain: ALWAYS_FALSE,
                    if_false: ALWAYS_TRUE,
                    source_order,
                    max_source_order: source_order,
                })
            }
            ConstraintAssignment::Unconstrained(constraint) => {
                // The result holds regardless of the constraint's truth value, so only
                // `if_uncertain` needs to be `ALWAYS_TRUE` — `n? 0: 1: 0`. It would also be
                // correct to use `n? 1: 1: 1` (i.e., `ALWAYS_TRUE` for all outgoing edges), but
                // that would throw away some of the efficiency gains this representation gives us.
                builder.intern_interior_node(InteriorNodeData {
                    constraint,
                    if_true: ALWAYS_FALSE,
                    if_uncertain: ALWAYS_TRUE,
                    if_false: ALWAYS_FALSE,
                    source_order,
                    max_source_order: source_order,
                })
            }
        }
    }
}

impl NodeId {
    fn node(self) -> Node {
        match self {
            ALWAYS_TRUE => Node::AlwaysTrue,
            ALWAYS_FALSE => Node::AlwaysFalse,
            _ => Node::Interior(InteriorNode(self)),
        }
    }

    fn is_terminal(self) -> bool {
        self.0 >= SMALLEST_TERMINAL.0
    }

    /// Returns the BDD variable of the root node of this BDD, or `None` if this BDD is a terminal
    /// node.
    fn root_constraint(self, builder: &ConstraintSetBuilder<'_>) -> Option<ConstraintId> {
        if self.is_terminal() {
            return None;
        }
        let interior = builder.interior_node_data(self);
        Some(interior.constraint)
    }

    fn max_source_order(self, builder: &ConstraintSetBuilder<'_>) -> usize {
        if self.is_terminal() {
            return 0;
        }
        let interior = builder.interior_node_data(self);
        interior.max_source_order
    }

    /// Returns a copy of this BDD node with all `source_order`s adjusted by the given amount.
    fn with_adjusted_source_order(self, builder: &ConstraintSetBuilder<'_>, delta: usize) -> Self {
        if delta == 0 {
            return self;
        }
        match self.node() {
            Node::AlwaysTrue | Node::AlwaysFalse => self,
            Node::Interior(_) => {
                let interior = builder.interior_node_data(self);
                NodeId::with_uncertain(
                    builder,
                    interior.constraint,
                    interior.if_true.with_adjusted_source_order(builder, delta),
                    interior
                        .if_uncertain
                        .with_adjusted_source_order(builder, delta),
                    interior.if_false.with_adjusted_source_order(builder, delta),
                    interior.source_order + delta,
                )
            }
        }
    }

    /// Checks whether this BDD represents a single conjunction (of an arbitrary number of
    /// positive or negative constraints).
    fn is_single_conjunction(self, builder: &ConstraintSetBuilder<'_>) -> bool {
        // A BDD can be viewed as an encoding of the formula's DNF representation (OR of ANDs).
        // Each path from the root node to the `always` terminals represents one of the disjoints.
        // The constraints that we encounter on the path represent the conjoints. That means that a
        // BDD can only represent a single conjunction if there is precisely one path from the root
        // node to the `always` terminal.
        //
        // We can take advantage of quasi-reduction. We never create an interior node with both
        // outgoing edges leading to `never`; those are collapsed to `never`. That means that if we
        // ever encounter a node with both outgoing edges pointing to something other than `never`,
        // that node must have at least two paths to the `always` terminal.
        let mut current = self.node();
        loop {
            match current {
                Node::AlwaysTrue => return true,
                Node::AlwaysFalse => return false,
                Node::Interior(interior) => {
                    let data = builder.interior_node_data(interior.node());

                    // If both if_true and if_false point to non-never, there are multiple paths to
                    // `always`, so this cannot be a simple conjunction.
                    if data.if_true != ALWAYS_FALSE && data.if_false != ALWAYS_FALSE {
                        return false;
                    }

                    // The uncertain branch must also be never for a simple conjunction, since it
                    // contributes to all paths.
                    if data.if_uncertain != ALWAYS_FALSE {
                        return false;
                    }

                    // Follow the non-never branch.
                    current = if data.if_true != ALWAYS_FALSE {
                        data.if_true.node()
                    } else {
                        data.if_false.node()
                    };
                }
            }
        }
    }

    fn for_each_path<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        mut f: impl FnMut(&PathAssignments),
    ) {
        match self.node() {
            Node::AlwaysTrue => {}
            Node::AlwaysFalse => {}
            Node::Interior(interior) => {
                let mut path = interior.path_assignments(builder);
                self.for_each_path_inner(db, builder, &mut f, &mut path);
            }
        }
    }

    fn for_each_path_inner<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        f: &mut dyn FnMut(&PathAssignments),
        path: &mut PathAssignments,
    ) {
        match self.node() {
            Node::AlwaysTrue => f(path),
            Node::AlwaysFalse => {}
            Node::Interior(_) => {
                let interior = builder.interior_node_data(self);
                path.walk_edge(
                    db,
                    builder,
                    interior.constraint.when_true(),
                    interior.source_order,
                    |path, _| {
                        interior.if_true.for_each_path_inner(db, builder, f, path);
                        interior
                            .if_uncertain
                            .for_each_path_inner(db, builder, f, path);
                    },
                );
                path.walk_edge(
                    db,
                    builder,
                    interior.constraint.when_false(),
                    interior.source_order,
                    |path, _| {
                        interior.if_false.for_each_path_inner(db, builder, f, path);
                        interior
                            .if_uncertain
                            .for_each_path_inner(db, builder, f, path);
                    },
                );
            }
        }
    }

    /// Returns whether this BDD represent the constant function `true`.
    fn is_always_satisfied<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
    ) -> bool {
        match self.node() {
            Node::AlwaysTrue => true,
            Node::AlwaysFalse => false,
            Node::Interior(interior) => {
                let mut path = interior.path_assignments(builder);
                self.is_always_satisfied_inner(db, builder, &mut path)
            }
        }
    }

    fn is_always_satisfied_inner<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        path: &mut PathAssignments,
    ) -> bool {
        match self.node() {
            Node::AlwaysTrue => true,
            Node::AlwaysFalse => false,
            Node::Interior(_) => {
                // walk_edge will return None if this node's constraint (or anything we can derive
                // from it) causes the if_true edge to become impossible. We want to ignore
                // impossible paths, and so we treat them as passing the "always satisfied" check.
                //
                // Under TDD semantics, when the constraint holds the result is C ∨ U, and when it
                // doesn't the result is D ∨ U. We fold the uncertain branch into both before
                // checking, because "C ∨ U is always satisfied" cannot be decomposed into
                // independent checks on C and U (it's a disjunction). This is zero-cost for binary
                // BDDs since `C ∨ false = C`.
                let interior = builder.interior_node_data(self);
                let if_true_or_uncertain = interior.if_true.or(builder, interior.if_uncertain);
                let true_always_satisfied = path
                    .walk_edge(
                        db,
                        builder,
                        interior.constraint.when_true(),
                        interior.source_order,
                        |path, _| if_true_or_uncertain.is_always_satisfied_inner(db, builder, path),
                    )
                    .unwrap_or(true);
                if !true_always_satisfied {
                    return false;
                }

                // Ditto for the if_false branch
                let if_false_or_uncertain = interior.if_false.or(builder, interior.if_uncertain);
                path.walk_edge(
                    db,
                    builder,
                    interior.constraint.when_false(),
                    interior.source_order,
                    |path, _| if_false_or_uncertain.is_always_satisfied_inner(db, builder, path),
                )
                .unwrap_or(true)
            }
        }
    }

    /// Returns whether this BDD represent the constant function `false`.
    fn is_never_satisfied<'db>(self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> bool {
        match self.node() {
            Node::AlwaysTrue => false,
            Node::AlwaysFalse => true,
            Node::Interior(interior) => {
                let mut path = interior.path_assignments(builder);
                self.is_never_satisfied_inner(db, builder, &mut path)
            }
        }
    }

    fn is_never_satisfied_inner<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        path: &mut PathAssignments,
    ) -> bool {
        match self.node() {
            Node::AlwaysTrue => false,
            Node::AlwaysFalse => true,
            Node::Interior(_) => {
                // walk_edge will return None if this node's constraint (or anything we can derive
                // from it) causes the if_true edge to become impossible. We want to ignore
                // impossible paths, and so we treat them as passing the "never satisfied" check.
                //
                // Note that unlike `is_always_satisfied`, here we don't have to fold the uncertain
                // branch into the true and false branches, since C ∨ U is only false when C and U
                // are each independently false. That lets us check each branch in isolation.
                let interior = builder.interior_node_data(self);
                let true_never_satisfied = path
                    .walk_edge(
                        db,
                        builder,
                        interior.constraint.when_true(),
                        interior.source_order,
                        |path, _| interior.if_true.is_never_satisfied_inner(db, builder, path),
                    )
                    .unwrap_or(true);
                if !true_never_satisfied {
                    return false;
                }

                // Ditto for the if_uncertain branch
                let uncertain_never_satisfied = path
                    .walk_edge(
                        db,
                        builder,
                        interior.constraint.when_unconstrained(),
                        interior.source_order,
                        |path, _| {
                            interior
                                .if_uncertain
                                .is_never_satisfied_inner(db, builder, path)
                        },
                    )
                    .unwrap_or(true);
                if !uncertain_never_satisfied {
                    return false;
                }

                // Ditto for the if_false branch
                path.walk_edge(
                    db,
                    builder,
                    interior.constraint.when_false(),
                    interior.source_order,
                    |path, _| {
                        interior
                            .if_false
                            .is_never_satisfied_inner(db, builder, path)
                    },
                )
                .unwrap_or(true)
            }
        }
    }

    fn solutions<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Solutions<'db> {
        let path_bounds = PathBounds::compute(db, builder, self);
        path_bounds.solve(db, builder)
    }

    /// Returns the negation of this BDD.
    fn negate(self, builder: &ConstraintSetBuilder<'_>) -> Self {
        match self.node() {
            Node::AlwaysTrue => ALWAYS_FALSE,
            Node::AlwaysFalse => ALWAYS_TRUE,
            Node::Interior(interior) => interior.negate(builder),
        }
    }

    /// Returns the `or` or union of two BDDs.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    fn or_with_offset(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
        // To ensure that `self` appears before `other` in `source_order`, we add the maximum
        // `source_order` of the lhs to all of the `source_order`s in the rhs.
        //
        // TODO: If we store `other_offset` as a new field on InteriorNode, we might be able to
        // avoid all of the extra work in the calls to with_adjusted_source_order, and apply the
        // adjustment lazily when walking a BDD tree. (ditto below in the other _with_offset
        // methods)
        let other_offset = self.max_source_order(builder);
        self.or_inner(builder, other, other_offset)
    }

    fn or(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
        self.or_inner(builder, other, 0)
    }

    fn or_inner(
        self,
        builder: &ConstraintSetBuilder<'_>,
        other: Self,
        other_offset: usize,
    ) -> Self {
        match (self.node(), other.node()) {
            (Node::AlwaysTrue, Node::AlwaysTrue) => ALWAYS_TRUE,
            (Node::AlwaysTrue, Node::Interior(_)) => {
                let other_interior = builder.interior_node_data(other);
                // If lhs is always true, then the overall result is true for any assignment of
                // rhs.
                NodeId::with_uncertain(
                    builder,
                    other_interior.constraint,
                    ALWAYS_FALSE,
                    ALWAYS_TRUE,
                    ALWAYS_FALSE,
                    other_interior.source_order + other_offset,
                )
            }
            (Node::Interior(_), Node::AlwaysTrue) => {
                let self_interior = builder.interior_node_data(self);
                // If rhs is always true, then the overall result is true for any assignment of
                // lhs.
                NodeId::with_uncertain(
                    builder,
                    self_interior.constraint,
                    ALWAYS_FALSE,
                    ALWAYS_TRUE,
                    ALWAYS_FALSE,
                    self_interior.source_order,
                )
            }
            (Node::AlwaysFalse, _) => other.with_adjusted_source_order(builder, other_offset),
            (_, Node::AlwaysFalse) => self,
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.or(builder, other_interior, other_offset)
            }
        }
    }

    /// Combine an iterator of nodes into a single node using an associative operator.
    ///
    /// Because the operator is associative, we don't have to combine the nodes left to right; we
    /// can instead combine them in a "tree-like" way:
    ///
    /// ```text
    /// linear:  (((((a ∨ b) ∨ c) ∨ d) ∨ e) ∨ f) ∨ g
    /// tree:    ((a ∨ b) ∨ (c ∨ d)) ∨ ((e ∨ f) ∨ g)
    /// ```
    ///
    /// We have to invoke the operator the same number of times. But BDD operators are often much
    /// cheaper when the operands are small, and with the tree shape, many more of the invocations
    /// are performed on small BDDs.
    ///
    /// You must also provide the "zero" and "one" units of the operator. The "zero" is the value
    /// that has no effect (`0 ∨ a = a`). It is returned if the iterator is empty. The "one" is the
    /// value that saturates (`1 ∨ a = 1`). We use this to short-circuit; if any element BDD or any
    /// intermediate result evaluates to "one", we can return early.
    fn tree_fold<'db>(
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        nodes: impl Iterator<Item = Self>,
        zero: Self,
        is_one: impl Fn(Self, &'db dyn Db, &ConstraintSetBuilder<'db>) -> bool,
        mut combine: impl FnMut(Self, &ConstraintSetBuilder<'db>, Self) -> Self,
    ) -> Self {
        // To implement the "linear" shape described above, we could collect the iterator elements
        // into a vector, and then use the fold at the bottom of this method to combine the
        // elements using the operator.
        //
        // To implement the "tree" shape, we also maintain a "depth" for each element of the
        // vector, which indicates how many times the operator has been applied to the element.
        // As we collect elements into the vector, we keep it capped at a length `O(log n)` of the
        // number of elements seen so far. To do that, whenever the last two elements of the vector
        // have the same depth, we apply the operator once to combine those two elements, adding
        // the result back to the vector with an incremented depth. (That might let us combine the
        // result with the _next_ intermediate result in the vector, and so on.)
        //
        // Walking through the example above, our vector ends up looking like:
        //
        //                                a/0
        //                     a/0 b/0 => ab/1
        //                                ab/1 c/0
        //   ab/1 c/0 d/0 => ab/1 cd/1 => abcd/2
        //                                abcd/2 e/0
        //              abcd/2 e/0 f/0 => abcd/2 ef/1
        //                                abcd/2 ef/1 g/0
        //
        // We use a SmallVec for the accumulator so that we don't have to spill over to the heap
        // until the iterator passes 256 elements.
        let mut accumulator: SmallVec<[(NodeId, u8); 8]> = SmallVec::default();
        for node in nodes {
            if is_one(node, db, builder) {
                return node;
            }

            let (mut node, mut depth) = (node, 0);
            while accumulator
                .last()
                .is_some_and(|(_, existing)| *existing == depth)
            {
                let (existing, _) = accumulator.pop().expect("accumulator should not be empty");
                node = combine(existing, builder, node);
                if is_one(node, db, builder) {
                    return node;
                }
                depth += 1;
            }
            accumulator.push((node, depth));
        }

        // At this point, we've consumed all of the iterator. The length of the accumulator will be
        // the same as the number of 1 bits in the length of the iterator. We do a final fold to
        // produce the overall result.
        accumulator
            .into_iter()
            .fold(zero, |result, (node, _)| combine(result, builder, node))
    }

    fn distributed_or<'db>(
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        nodes: impl Iterator<Item = NodeId>,
    ) -> Self {
        Self::tree_fold(
            db,
            builder,
            nodes,
            ALWAYS_FALSE,
            Self::is_always_satisfied,
            Self::or_with_offset,
        )
    }

    fn distributed_and<'db>(
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        nodes: impl Iterator<Item = NodeId>,
    ) -> Self {
        Self::tree_fold(
            db,
            builder,
            nodes,
            ALWAYS_TRUE,
            Self::is_never_satisfied,
            Self::and_with_offset,
        )
    }

    /// Returns the `and` or intersection of two BDDs.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    fn and_with_offset(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
        // To ensure that `self` appears before `other` in `source_order`, we add the maximum
        // `source_order` of the lhs to all of the `source_order`s in the rhs.
        let other_offset = self.max_source_order(builder);
        self.and_inner(builder, other, other_offset)
    }

    fn and(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
        self.and_inner(builder, other, 0)
    }

    fn and_inner(
        self,
        builder: &ConstraintSetBuilder<'_>,
        other: Self,
        other_offset: usize,
    ) -> Self {
        match (self.node(), other.node()) {
            (Node::AlwaysFalse, Node::AlwaysFalse) => ALWAYS_FALSE,
            (Node::AlwaysFalse, Node::Interior(_)) => {
                let other_interior = builder.interior_node_data(other);
                NodeId::new(
                    builder,
                    other_interior.constraint,
                    ALWAYS_FALSE,
                    ALWAYS_FALSE,
                    other_interior.source_order + other_offset,
                )
            }
            (Node::Interior(_), Node::AlwaysFalse) => {
                let self_interior = builder.interior_node_data(self);
                NodeId::new(
                    builder,
                    self_interior.constraint,
                    ALWAYS_FALSE,
                    ALWAYS_FALSE,
                    self_interior.source_order,
                )
            }
            (Node::AlwaysTrue, _) => other.with_adjusted_source_order(builder, other_offset),
            (_, Node::AlwaysTrue) => self,
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.and(builder, other_interior, other_offset)
            }
        }
    }

    fn implies(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
        // p → q == ¬p ∨ q
        self.negate(builder).or(builder, other)
    }

    /// Returns a new BDD that evaluates to `true` when both input BDDs evaluate to the same
    /// result.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    fn iff_with_offset(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
        // To ensure that `self` appears before `other` in `source_order`, we add the maximum
        // `source_order` of the lhs to all of the `source_order`s in the rhs.
        let other_offset = self.max_source_order(builder);
        self.iff_inner(builder, other, other_offset)
    }

    fn iff(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
        self.iff_inner(builder, other, 0)
    }

    fn iff_inner(
        self,
        builder: &ConstraintSetBuilder<'_>,
        other: Self,
        other_offset: usize,
    ) -> Self {
        // iff(a, b) = (a ∧ b) ∨ (¬a ∧ ¬b)
        let a_and_b = self.and_inner(builder, other, other_offset);
        let not_a_and_not_b =
            self.negate(builder)
                .and_inner(builder, other.negate(builder), other_offset);
        a_and_b.or(builder, not_a_and_not_b)
    }

    /// Returns the `if-then-else` of three BDDs: when `self` evaluates to `true`, it returns what
    /// `then_node` evaluates to; otherwise it returns what `else_node` evaluates to.
    fn ite(self, builder: &ConstraintSetBuilder<'_>, then_node: Self, else_node: Self) -> Self {
        self.and(builder, then_node)
            .or(builder, self.negate(builder).and(builder, else_node))
    }

    /// Returns the TDD `if-then-else` of four BDDs: when `self` evaluates to `true`, it returns
    /// what `then_node` evaluates to; when `self` evaluates to `false`, it returns what
    /// `else_node` evaluates to; and `uncertain_node` is included regardless of `self`'s value.
    fn ite_uncertain(
        self,
        builder: &ConstraintSetBuilder<'_>,
        then_node: Self,
        uncertain_node: Self,
        else_node: Self,
    ) -> Self {
        if uncertain_node == ALWAYS_TRUE {
            return ALWAYS_TRUE;
        }

        match self.node() {
            Node::AlwaysTrue => then_node.or(builder, uncertain_node),
            Node::AlwaysFalse => else_node.or(builder, uncertain_node),
            Node::Interior(_) => {
                let interior = builder.interior_node_data(self);
                // Fast path for a bare positive constraint whose branches are still later in the
                // BDD variable ordering. This is the common case when loading an owned TDD into a
                // fresh builder, and lets us preserve an existing uncertain branch directly.
                if interior.if_true == ALWAYS_TRUE
                    && interior.if_uncertain == ALWAYS_FALSE
                    && interior.if_false == ALWAYS_FALSE
                    && then_node
                        .root_constraint(builder)
                        .is_none_or(|root| root.ordering() > interior.constraint.ordering())
                    && uncertain_node
                        .root_constraint(builder)
                        .is_none_or(|root| root.ordering() > interior.constraint.ordering())
                    && else_node
                        .root_constraint(builder)
                        .is_none_or(|root| root.ordering() > interior.constraint.ordering())
                {
                    return NodeId::with_uncertain(
                        builder,
                        interior.constraint,
                        then_node,
                        uncertain_node,
                        else_node,
                        interior.source_order,
                    );
                }

                // For compound conditions, or when the new builder's variable ordering requires
                // one of the branches to move above `self`, fall back to the semantic expansion:
                // `(self ∧ then_node) ∨ uncertain_node ∨ (¬self ∧ else_node)`.
                self.and(builder, then_node)
                    .or(builder, uncertain_node)
                    .or(builder, self.negate(builder).and(builder, else_node))
            }
        }
    }

    fn implies_subtype_of<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        lhs: Type<'db>,
        rhs: Type<'db>,
    ) -> Self {
        // When checking subtyping involving a typevar, we can turn the subtyping check into a
        // constraint (i.e, "is `T` a subtype of `int` becomes the constraint `T ≤ int`), and then
        // check when the BDD implies that constraint.
        //
        // Note that we are NOT guaranteed that `lhs` and `rhs` will always be fully static, since
        // these types are coming in from arbitrary subtyping checks that the caller might want to
        // perform. So we have to take the appropriate materialization when translating the check
        // into a constraint.
        let constraint = match (lhs, rhs) {
            (Type::TypeVar(bound_typevar), _) => Constraint::new_node(
                db,
                builder,
                bound_typevar,
                Type::Never,
                rhs.bottom_materialization(db),
            ),
            (_, Type::TypeVar(bound_typevar)) => Constraint::new_node(
                db,
                builder,
                bound_typevar,
                lhs.top_materialization(db),
                Type::object(),
            ),
            _ => panic!("at least one type should be a typevar"),
        };

        self.implies(builder, constraint)
    }

    fn satisfied_by_all_typevars<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> bool {
        match self.node() {
            Node::AlwaysTrue => return true,
            Node::AlwaysFalse => return false,
            Node::Interior(_) => {}
        }

        let mut typevars = FxHashSet::default();
        self.for_each_constraint(builder, &mut |constraint, _| {
            let constraint = builder.constraint_data(constraint);
            typevars.insert(constraint.typevar);
        });

        // Returns if some specialization satisfies this constraint set.
        let some_specialization_satisfies = move |specializations: NodeId| {
            let when_satisfied = specializations
                .implies(builder, self)
                .and(builder, specializations);
            !when_satisfied.is_never_satisfied(db, builder)
        };

        // Returns if all specializations satisfy this constraint set.
        let all_specializations_satisfy = move |specializations: NodeId| {
            let when_satisfied = specializations
                .implies(builder, self)
                .and(builder, specializations);
            when_satisfied
                .iff(builder, specializations)
                .is_always_satisfied(db, builder)
        };

        for typevar in typevars {
            if typevar.is_inferable(db, inferable) {
                // If the typevar is in inferable position, we need to verify that some valid
                // specialization satisfies the constraint set.
                let valid_specializations = typevar.valid_specializations(db, builder);
                if !some_specialization_satisfies(valid_specializations) {
                    return false;
                }
            } else {
                // If the typevar is in non-inferable position, we need to verify that all required
                // specializations satisfy the constraint set. Complicating things, the typevar
                // might have gradual constraints. For those, we need to know the range of valid
                // materializations, but we only need some materialization to satisfy the
                // constraint set.
                //
                // NB: We could also model this by introducing a synthetic typevar for the gradual
                // constraint, treating that synthetic typevar as always inferable (so that we only
                // need to verify for some materialization), and then update this typevar's
                // constraint to refer to the synthetic typevar instead of the original gradual
                // constraint.
                let (static_specializations, gradual_constraints) =
                    typevar.required_specializations(db, builder);
                if !all_specializations_satisfy(static_specializations) {
                    return false;
                }
                for gradual_constraint in gradual_constraints {
                    if !some_specialization_satisfies(gradual_constraint) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Returns a new BDD that is the _existential abstraction_ of `self` for a set of typevars.
    /// The result will return true whenever `self` returns true for _any_ assignment of those
    /// typevars. The result will not contain any constraints that mention those typevars.
    fn exists<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        bound_typevars: impl IntoIterator<Item = BoundTypeVarIdentity<'db>>,
    ) -> Self {
        bound_typevars
            .into_iter()
            .fold(self, |abstracted, bound_typevar| {
                abstracted.exists_one(db, builder, bound_typevar)
            })
    }

    fn exists_one<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarIdentity<'db>,
    ) -> Self {
        match self.node() {
            Node::AlwaysTrue => ALWAYS_TRUE,
            Node::AlwaysFalse => ALWAYS_FALSE,
            Node::Interior(interior) => interior.exists_one(db, builder, bound_typevar),
        }
    }

    fn remove_noninferable<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> Self {
        match self.node() {
            Node::AlwaysTrue => ALWAYS_TRUE,
            Node::AlwaysFalse => ALWAYS_FALSE,
            Node::Interior(interior) => interior.remove_noninferable(db, builder, inferable),
        }
    }

    fn abstract_one_inner<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        should_remove: &mut dyn FnMut(ConstraintId) -> bool,
        path: &mut PathAssignments,
    ) -> Self {
        match self.node() {
            Node::AlwaysTrue => ALWAYS_TRUE,
            Node::AlwaysFalse => ALWAYS_FALSE,
            Node::Interior(interior) => {
                interior.abstract_one_inner(db, builder, should_remove, path)
            }
        }
    }

    /// Returns a new BDD that returns the same results as `self`, but with some inputs fixed to
    /// particular values. (Those variables will not be checked when evaluating the result, and
    /// will not be present in the result.)
    ///
    /// Also returns whether _all_ of the restricted variables appeared in the BDD.
    fn restrict<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        assignment: impl IntoIterator<Item = ConstraintAssignment>,
    ) -> (Self, bool) {
        assignment
            .into_iter()
            .fold((self, true), |(restricted, found), assignment| {
                let (restricted, found_this) = restricted.restrict_one(db, builder, assignment);
                (restricted, found && found_this)
            })
    }

    /// Returns a new BDD that returns the same results as `self`, but with one input fixed to a
    /// particular value. (That variable will be not be checked when evaluating the result, and
    /// will not be present in the result.)
    ///
    /// Also returns whether the restricted variable appeared in the BDD.
    fn restrict_one<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        assignment: ConstraintAssignment,
    ) -> (Self, bool) {
        match self.node() {
            Node::AlwaysTrue | Node::AlwaysFalse => (self, false),
            Node::Interior(interior) => interior.restrict_one(db, builder, assignment),
        }
    }

    /// Returns a new BDD with any occurrence of `left ∧ right` replaced with `replacement`.
    #[expect(clippy::too_many_arguments)]
    fn substitute_intersection<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        left: ConstraintAssignment,
        left_source_order: usize,
        right: ConstraintAssignment,
        right_source_order: usize,
        replacement: NodeId,
    ) -> Self {
        // We perform a Shannon expansion to find out what the input BDD evaluates to when:
        //   - left and right are both true
        //   - left is false
        //   - left is true and right is false
        // This covers the entire truth table of `left ∧ right`.
        let (when_left_and_right, both_found) = self.restrict(db, builder, [left, right]);
        if !both_found {
            // If left and right are not both present in the input BDD, we should not even attempt
            // the substitution, since the Shannon expansion might introduce the missing variables!
            // That confuses us below when we try to detect whether the substitution is consistent
            // with the input.
            return self;
        }
        let (when_not_left, _) = self.restrict(db, builder, [left.negated()]);
        let (when_left_but_not_right, _) = self.restrict(db, builder, [left, right.negated()]);

        // The result should test `replacement`, and when it's true, it should produce the same
        // output that input would when `left ∧ right` is true. When replacement is false, it
        // should fall back on testing left and right individually to make sure we produce the
        // correct outputs in the `¬(left ∧ right)` case. So the result is
        //
        //   if replacement
        //     when_left_and_right
        //   else if not left
        //     when_not_left
        //   else if not right
        //     when_left_but_not_right
        //   else
        //     false
        //
        //  (Note that the `else` branch shouldn't be reachable, but we have to provide something!)
        let left_node = Node::new_satisfied_constraint(builder, left, left_source_order);
        let right_node = Node::new_satisfied_constraint(builder, right, right_source_order);
        let right_result = right_node.ite(builder, ALWAYS_FALSE, when_left_but_not_right);
        let left_result = left_node.ite(builder, right_result, when_not_left);
        let result = replacement.ite(builder, when_left_and_right, left_result);

        // Lastly, verify that the result is consistent with the input. (It must produce the same
        // results when `left ∧ right`.) If it doesn't, the substitution isn't valid, and we should
        // return the original BDD unmodified.
        let validity = replacement.iff(builder, left_node.and(builder, right_node));
        let constrained_original = self.and(builder, validity);
        let constrained_replacement = result.and(builder, validity);
        if constrained_original == constrained_replacement {
            result
        } else {
            self
        }
    }

    /// Returns a new BDD with any occurrence of `left ∨ right` replaced with `replacement`.
    #[expect(clippy::too_many_arguments)]
    fn substitute_union<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        left: ConstraintAssignment,
        left_source_order: usize,
        right: ConstraintAssignment,
        right_source_order: usize,
        replacement: NodeId,
    ) -> Self {
        // We perform a Shannon expansion to find out what the input BDD evaluates to when:
        //   - left and right are both true
        //   - left is true and right is false
        //   - left is false and right is true
        //   - left and right are both false
        // This covers the entire truth table of `left ∨ right`.
        let (when_l1_r1, both_found) = self.restrict(db, builder, [left, right]);
        if !both_found {
            // If left and right are not both present in the input BDD, we should not even attempt
            // the substitution, since the Shannon expansion might introduce the missing variables!
            // That confuses us below when we try to detect whether the substitution is consistent
            // with the input.
            return self;
        }
        let (when_l0_r0, _) = self.restrict(db, builder, [left.negated(), right.negated()]);
        let (when_l1_r0, _) = self.restrict(db, builder, [left, right.negated()]);
        let (when_l0_r1, _) = self.restrict(db, builder, [left.negated(), right]);

        // The result should test `replacement`, and when it's true, it should produce the same
        // output that input would when `left ∨ right` is true. For OR, this is the union of what
        // the input produces for the three cases that comprise `left ∨ right`. When `replacement`
        // is false, the result should produce the same output that input would when
        // `¬(left ∨ right)`, i.e. when `left ∧ right`. So the result is
        //
        //   if replacement
        //     or(when_l1_r1, when_l1_r0, when_r0_l1)
        //   else
        //     when_l0_r0
        let result = replacement.ite(
            builder,
            when_l1_r0.or(builder, when_l0_r1.or(builder, when_l1_r1)),
            when_l0_r0,
        );

        // Lastly, verify that the result is consistent with the input. (It must produce the same
        // results when `left ∨ right`.) If it doesn't, the substitution isn't valid, and we should
        // return the original BDD unmodified.
        let left_node = Node::new_satisfied_constraint(builder, left, left_source_order);
        let right_node = Node::new_satisfied_constraint(builder, right, right_source_order);
        let validity = replacement.iff(builder, left_node.or(builder, right_node));
        let constrained_original = self.and(builder, validity);
        let constrained_replacement = result.and(builder, validity);
        if constrained_original == constrained_replacement {
            result
        } else {
            self
        }
    }

    /// Invokes a closure for each constraint variable that appears anywhere in a BDD. (Any given
    /// constraint can appear multiple times in different paths from the root; we do not
    /// deduplicate those constraints, and will instead invoke the callback each time we encounter
    /// the constraint.)
    fn for_each_constraint(
        self,
        builder: &ConstraintSetBuilder<'_>,
        f: &mut dyn FnMut(ConstraintId, usize),
    ) {
        if self.is_terminal() {
            return;
        }
        let interior = builder.interior_node_data(self);
        f(interior.constraint, interior.source_order);
        interior.if_true.for_each_constraint(builder, f);
        interior.if_uncertain.for_each_constraint(builder, f);
        interior.if_false.for_each_constraint(builder, f);
    }

    /// Simplifies a BDD, replacing constraints with simpler or smaller constraints where possible.
    ///
    /// TODO: [Historical note] This is now used only for display purposes, but previously was also
    /// used to ensure that we added the "transitive closure" to each BDD. The constraints in a BDD
    /// are not independent; some combinations of constraints can imply other constraints. This
    /// affects us in two ways: First, it means that certain combinations are impossible. (If
    /// `a → b` then `a ∧ ¬b` can never happen.) Second, it means that certain constraints can be
    /// inferred even if they do not explicitly appear in the BDD. It is important to take this
    /// into account in several BDD operations (satisfiability, existential quantification, etc).
    /// Before, we used this method to _add_ the transitive closure to a BDD, in an attempt to make
    /// sure that it holds "all the facts" that would be needed to satisfy any query we might make.
    /// We also used this method to calculate the "domain" of the BDD to help rule out invalid
    /// inputs. However, this was at odds with using this method for display purposes, where our
    /// goal is to _remove_ redundant information, so as to not clutter up the display. To resolve
    /// this dilemma, all of the correctness uses have been refactored to use [`SequentMap`]
    /// instead. It tracks the same information in a more efficient and lazy way, and never tries
    /// to remove redundant information. For expediency, however, we did not make any changes to
    /// this method, other than to stop tracking the domain (which was never used for display
    /// purposes). That means we have some tech debt here, since there is a lot of duplicate logic
    /// between `simplify_for_display` and `SequentMap`. It would be nice to update our display
    /// logic to use the sequent map as much as possible. But that can happen later.
    fn simplify_for_display<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Self {
        match self.node() {
            Node::AlwaysTrue | Node::AlwaysFalse => self,
            Node::Interior(interior) => interior.simplify(db, builder),
        }
    }

    /// Returns clauses describing all of the variable assignments that cause this BDD to evaluate
    /// to `true`. (This translates the boolean function that this BDD represents into DNF form.)
    fn satisfied_clauses(self, builder: &ConstraintSetBuilder<'_>) -> SatisfiedClauses {
        struct Searcher {
            clauses: SatisfiedClauses,
            current_clause: SatisfiedClause,
        }

        impl Searcher {
            fn visit_node(&mut self, builder: &ConstraintSetBuilder<'_>, node: NodeId) {
                match node.node() {
                    Node::AlwaysFalse => {}
                    Node::AlwaysTrue => self.clauses.push(self.current_clause.clone()),
                    Node::Interior(_) => {
                        let interior = builder.interior_node_data(node);
                        self.current_clause.push(interior.constraint.when_true());
                        self.visit_node(builder, interior.if_true);
                        self.current_clause.pop();
                        self.current_clause
                            .push(interior.constraint.when_unconstrained());
                        self.visit_node(builder, interior.if_uncertain);
                        self.current_clause.pop();
                        self.current_clause.push(interior.constraint.when_false());
                        self.visit_node(builder, interior.if_false);
                        self.current_clause.pop();
                    }
                }
            }
        }

        let mut searcher = Searcher {
            clauses: SatisfiedClauses::default(),
            current_clause: SatisfiedClause::default(),
        };
        searcher.visit_node(builder, self);
        searcher.clauses
    }

    fn display<'db>(self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> impl Display {
        // To render a BDD in DNF form, you perform a depth-first search of the BDD tree, looking
        // for any path that leads to the AlwaysTrue terminal. Each such path represents one of the
        // intersection clauses in the DNF form. The path traverses zero or more interior nodes,
        // and takes either the true or false edge from each one. That gives you the positive or
        // negative individual constraints in the path's clause.
        struct DisplayNode<'db, 'c> {
            node: NodeId,
            db: &'db dyn Db,
            builder: &'c ConstraintSetBuilder<'db>,
        }

        impl Display for DisplayNode<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.node.node() {
                    Node::AlwaysTrue => f.write_str("always"),
                    Node::AlwaysFalse => f.write_str("never"),
                    Node::Interior(_) => {
                        let mut clauses = self.node.satisfied_clauses(self.builder);
                        clauses.simplify(self.db, self.builder);
                        Display::fmt(&clauses.display(self.db, self.builder), f)
                    }
                }
            }
        }

        DisplayNode {
            node: self,
            db,
            builder,
        }
    }

    /// Displays the full graph structure of this BDD. `prefix` will be output before each line
    /// other than the first. Produces output like the following:
    ///
    /// ```text
    /// (T@_ = str)
    /// ┡━₁ (U@_ = str)
    /// │   ┡━₁ always
    /// │   └─₀ (U@_ = bool)
    /// │       ┡━₁ always
    /// │       └─₀ never
    /// └─₀ (T@_ = bool)
    ///     ┡━₁ (U@_ = str)
    ///     │   ┡━₁ always
    ///     │   └─₀ (U@_ = bool)
    ///     │       ┡━₁ always
    ///     │       └─₀ never
    ///     └─₀ never
    /// ```
    fn display_graph<'db, 'a>(
        self,
        db: &'db dyn Db,
        builder: &'a ConstraintSetBuilder<'db>,
        prefix: &'a dyn Display,
    ) -> impl Display + 'a {
        struct DisplayNode<'a, 'db> {
            db: &'db dyn Db,
            builder: &'a ConstraintSetBuilder<'db>,
            node: NodeId,
            prefix: &'a dyn Display,
            seen: RefCell<FxIndexSet<NodeId>>,
        }

        fn format_node<'db>(
            db: &'db dyn Db,
            builder: &ConstraintSetBuilder<'db>,
            node: NodeId,
            prefix: &dyn Display,
            seen: &RefCell<FxIndexSet<NodeId>>,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            match node.node() {
                Node::AlwaysTrue => write!(f, "always"),
                Node::AlwaysFalse => write!(f, "never"),
                Node::Interior(_) => {
                    let (index, is_new) = seen.borrow_mut().insert_full(node);
                    if !is_new {
                        return write!(f, "<{index}> SHARED");
                    }
                    let interior = builder.interior_node_data(node);
                    write!(
                        f,
                        "<{index}> {} {}/{}",
                        interior.constraint.display(db, builder),
                        interior.source_order,
                        interior.max_source_order,
                    )?;
                    // Calling display_graph recursively here causes rustc to claim that the
                    // expect(unused) up above is unfulfilled!
                    write!(f, "\n{prefix}┡━₁ ")?;
                    format_node(
                        db,
                        builder,
                        interior.if_true,
                        &format_args!("{prefix}│   "),
                        seen,
                        f,
                    )?;
                    write!(f, "\n{prefix}├─? ")?;
                    format_node(
                        db,
                        builder,
                        interior.if_uncertain,
                        &format_args!("{prefix}│   "),
                        seen,
                        f,
                    )?;
                    write!(f, "\n{prefix}└─₀ ")?;
                    format_node(
                        db,
                        builder,
                        interior.if_false,
                        &format_args!("{prefix}    "),
                        seen,
                        f,
                    )?;
                    Ok(())
                }
            }
        }

        impl Display for DisplayNode<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                format_node(self.db, self.builder, self.node, self.prefix, &self.seen, f)
            }
        }

        DisplayNode {
            db,
            builder,
            node: self,
            prefix,
            seen: RefCell::default(),
        }
    }
}

impl Debug for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_tuple("Node");
        match self.node() {
            // We use format_args instead of rendering the strings directly so that we don't get
            // any quotes in the output: ScopedReachabilityConstraintId(AlwaysTrue) instead of
            // ScopedReachabilityConstraintId("AlwaysTrue").
            Node::AlwaysTrue => f.field(&format_args!("AlwaysTrue")),
            Node::AlwaysFalse => f.field(&format_args!("AlwaysFalse")),
            Node::Interior(_) => f.field(&self.0),
        };
        f.finish()
    }
}

impl Idx for NodeId {
    #[inline]
    fn new(value: usize) -> Self {
        assert!(value <= (SMALLEST_TERMINAL.0 as usize));
        #[expect(clippy::cast_possible_truncation)]
        Self(value as u32)
    }

    #[inline]
    fn index(self) -> usize {
        debug_assert!(!self.is_terminal());
        self.0 as usize
    }
}

/// The index of an interior node within a [`ConstraintSetStorage`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
struct InteriorNode(NodeId);

/// An interior node of a BDD
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
struct InteriorNodeData {
    constraint: ConstraintId,
    if_true: NodeId,
    if_uncertain: NodeId,
    if_false: NodeId,

    /// Represents the order in which this node's constraint was added to the containing constraint
    /// set, relative to all of the other constraints in the set. This starts off at 1 for a simple
    /// single-constraint set (e.g. created with [`Node::new_constraint`] or
    /// [`Node::new_satisfied_constraint`]). It will get incremented, if needed, as that simple BDD
    /// is combined into larger BDDs.
    source_order: usize,

    /// The maximum `source_order` across this node and all of its descendants.
    max_source_order: usize,
}

/// Accumulated lower and upper bounds for a single typevar on a single BDD path.
///
/// Lower bounds are collected into a union (they are alternatives for the minimum type the
/// typevar can specialize to). Upper bounds are collected into an intersection (the typevar
/// must satisfy all of them simultaneously).
#[derive(Default)]
struct Bounds<'db> {
    lower: FxIndexSet<Type<'db>>,
    upper: FxIndexSet<Type<'db>>,
}

impl<'db> Bounds<'db> {
    fn add_lower(&mut self, _db: &'db dyn Db, ty: Type<'db>) {
        // Lower bounds are unioned. Our type representation is in DNF, so unioning a new
        // element is typically cheap (in that it does not involve a combinatorial
        // explosion from distributing the clause through an existing disjunction). So we
        // don't need to be as clever here as in `add_upper`.
        self.lower.insert(ty);
    }

    fn add_upper(&mut self, db: &'db dyn Db, ty: Type<'db>) {
        // Upper bounds are intersectioned. If `ty` is a union, that involves distributing
        // the union elements through the existing type. That makes it worth checking first
        // whether any of the types in the upper bound are redundant.

        // First check if there's an existing upper bound clause that is a subtype of the
        // new type. If so, adding the new type does nothing to the intersection.
        if self
            .upper
            .iter()
            .any(|existing| existing.is_redundant_with(db, ty))
        {
            return;
        }

        // Otherwise remove any existing clauses that are a supertype of the new type,
        // since the intersection will clip them to the new type.
        self.upper
            .retain(|existing| !ty.is_redundant_with(db, *existing));
        self.upper.insert(ty);
    }
}

/// Materialized lower and upper bounds for a single typevar on a single BDD path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct TypeVarBounds<'db> {
    bound_typevar: BoundTypeVarInstance<'db>,
    /// The union of all lower bounds on this path.
    lower: Type<'db>,
    /// The intersection of all upper bounds on this path (NOT including the typevar's declared
    /// upper bound).
    upper: Type<'db>,
}

impl<'db> Type<'db> {
    /// Calculates the [`PathBounds`] that represent the valid solutions for when `self` is
    /// constraint-set assignable to `target`.
    pub(crate) fn assignable_solutions_with_inferable(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> &'db PathBounds<'db> {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _, _, _| PathBounds::Unsatisfiable,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn assignable_solutions_impl<'db>(
            db: &'db dyn Db,
            source: Type<'db>,
            target: Type<'db>,
            inferable: InferableTypeVars<'db>,
        ) -> PathBounds<'db> {
            let when = source.when_constraint_set_assignable_to_owned(db, target);
            when.query(|builder, when| {
                let when = when.remove_noninferable(db, builder, inferable);
                PathBounds::compute(db, builder, when.node)
            })
        }

        assignable_solutions_impl(db, self, target, inferable)
    }
}

/// Per-path bounds for all typevars. Each element is the set of typevar bounds for one BDD path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) enum PathBounds<'db> {
    Unsatisfiable,
    Unconstrained,
    Constrained(Vec<Vec<TypeVarBounds<'db>>>),
}

impl<'db> PathBounds<'db> {
    /// Computes sorted BDD paths and accumulates per-typevar lower/upper bounds for each path.
    ///
    /// Returns a list of paths, where each path contains the materialized lower/upper bounds for
    /// each typevar that appears in the path's constraints.
    fn compute(db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>, node: NodeId) -> Self {
        match node.node() {
            Node::AlwaysTrue => return PathBounds::Unconstrained,
            Node::AlwaysFalse => return PathBounds::Unsatisfiable,
            Node::Interior(_) => {}
        }

        // Sort the constraints in each path by their `source_order`s, to ensure that we construct
        // any unions or intersections in our type mappings in a stable order. Constraints might
        // come out of `PathAssignment`s with identical `source_order`s, but if they do, those
        // "tied" constraints will still be ordered in a stable way. So we need a stable sort to
        // retain that stable per-tie ordering.
        let mut sorted_paths = Vec::new();
        node.for_each_path(db, builder, |path| {
            let mut path: Vec<_> = path.positive_constraints().collect();
            path.sort_by_key(|(_, source_order)| *source_order);
            sorted_paths.push(path);
        });
        sorted_paths.sort_by(|path1, path2| {
            let source_orders1 = path1.iter().map(|(_, source_order)| *source_order);
            let source_orders2 = path2.iter().map(|(_, source_order)| *source_order);
            source_orders1.cmp(source_orders2)
        });

        let mut result = Vec::with_capacity(sorted_paths.len());
        let mut mappings: FxHashMap<BoundTypeVarInstance<'db>, Bounds<'db>> = FxHashMap::default();

        for path in sorted_paths {
            mappings.clear();
            for (constraint, _) in path {
                let constraint = builder.constraint_data(constraint);
                let typevar = constraint.typevar;
                let lower = constraint.lower;
                let upper = constraint.upper;
                let bounds = mappings.entry(typevar).or_default();
                bounds.add_lower(db, lower);
                bounds.add_upper(db, upper);

                if let Type::TypeVar(lower_bound_typevar) = lower {
                    let bounds = mappings.entry(lower_bound_typevar).or_default();
                    bounds.add_upper(db, Type::TypeVar(typevar));
                }

                if let Type::TypeVar(upper_bound_typevar) = upper {
                    let bounds = mappings.entry(upper_bound_typevar).or_default();
                    bounds.add_lower(db, Type::TypeVar(typevar));
                }
            }

            let path_bounds = mappings
                .drain()
                .map(|(bound_typevar, bounds)| TypeVarBounds {
                    bound_typevar,
                    lower: UnionType::from_elements(db, bounds.lower),
                    upper: IntersectionType::from_elements(db, bounds.upper),
                })
                .collect();
            result.push(path_bounds);
        }

        PathBounds::Constrained(result)
    }

    pub(crate) fn solve(
        &self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Solutions<'db> {
        self.solve_with(|bound_typevar, _variance, lower, upper| {
            PathBounds::default_solve(db, builder, bound_typevar, lower, upper)
        })
    }

    /// Solves each path by applying a per-typevar solver function, collecting valid solutions.
    ///
    /// The solver receives the typevar and its materialized lower/upper bounds, and returns:
    /// - `Ok(Some(solution))` to add a solution for this typevar on this path
    /// - `Ok(None)` to leave this typevar unsolved on this path
    /// - `Err(())` to invalidate the entire path
    pub(crate) fn solve_with(
        &self,
        mut choose: impl FnMut(
            BoundTypeVarInstance<'db>,
            TypeVarVariance,
            Type<'db>,
            Type<'db>,
        ) -> Result<Option<Type<'db>>, ()>,
    ) -> Solutions<'db> {
        let paths = match self {
            PathBounds::Unsatisfiable => return Solutions::Unsatisfiable,
            PathBounds::Unconstrained => return Solutions::Unconstrained,
            PathBounds::Constrained(paths) => paths,
        };

        let mut solutions = Vec::with_capacity(paths.len());
        'paths: for path in paths {
            let mut solution = Vec::with_capacity(path.len());
            for bounds in path {
                let TypeVarBounds {
                    bound_typevar,
                    lower,
                    upper,
                } = *bounds;

                // Determine variance from the constraint bounds:
                // - Only upper bound (lower = Never) → covariant position
                // - Only lower bound (upper = object) → contravariant position
                // - Both bounds set → invariant position
                let variance = if lower.is_never() {
                    TypeVarVariance::Covariant
                } else if upper == Type::object() {
                    TypeVarVariance::Contravariant
                } else {
                    TypeVarVariance::Invariant
                };

                match choose(bound_typevar, variance, lower, upper) {
                    Ok(Some(ty)) => solution.push(TypeVarSolution {
                        bound_typevar,
                        solution: ty,
                    }),
                    Ok(None) => {}
                    Err(()) => continue 'paths,
                }
            }
            solutions.push(solution);
        }

        if solutions.is_empty() {
            return Solutions::Unsatisfiable;
        }
        Solutions::Constrained(solutions)
    }

    /// The default solution selection logic for a single typevar on a single BDD path.
    ///
    /// Given the materialized lower and upper bounds for a typevar, selects the solution type.
    /// Returns:
    /// - `Ok(Some(solution))` if the typevar is solved on this path
    /// - `Ok(None)` if the typevar is unsolved (no solution added)
    /// - `Err(())` if the path is invalid (bounds violate the typevar's declared constraints)
    pub(crate) fn default_solve(
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Result<Option<Type<'db>>, ()> {
        match bound_typevar.typevar(db).require_bound_or_constraints(db) {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                let bound = bound.top_materialization(db);
                let when = lower.when_constraint_set_assignable_to(db, bound, builder);
                if when.is_never_satisfied(db) {
                    // This path does not satisfy the typevar's upper bound, and is
                    // therefore not a valid specialization.
                    return Err(());
                }

                // Prefer the lower bound (often the concrete actual type seen) over the
                // upper bound (which may include TypeVar bounds/constraints). The upper bound
                // should only be used as a fallback when no concrete type was inferred.
                if !lower.is_never() {
                    return Ok(Some(lower));
                }

                let upper = IntersectionType::from_elements(
                    db,
                    std::iter::once(upper).chain(std::iter::once(bound)),
                );
                if upper != bound {
                    Ok(Some(upper))
                } else {
                    Ok(None)
                }
            }

            TypeVarBoundOrConstraints::Constraints(constraints) => {
                // Filter out the typevar constraints that aren't satisfied by this path.
                let compatible_constraints = constraints.elements(db).iter().filter(|constraint| {
                    let constraint_lower = constraint.bottom_materialization(db);
                    let constraint_upper = constraint.top_materialization(db);
                    let when = lower
                        .when_constraint_set_assignable_to(db, constraint_lower, builder)
                        .and(db, builder, || {
                            constraint_upper.when_constraint_set_assignable_to(db, upper, builder)
                        });
                    !when.is_never_satisfied(db)
                });

                // If only one constraint remains, that's our specialization for this path.
                match compatible_constraints.at_most_one() {
                    Ok(None) => {
                        // This path does not satisfy any of the constraints, and is
                        // therefore not a valid specialization.
                        Err(())
                    }

                    Ok(Some(compatible_constraint)) => Ok(Some(*compatible_constraint)),

                    Err(_) => {
                        // This path satisfies multiple constraints. For now, don't
                        // prefer any of them, and fall back on the default
                        // specialization for this typevar.
                        Ok(None)
                    }
                }
            }
        }
    }
}

impl InteriorNode {
    fn node(self) -> NodeId {
        self.0
    }

    fn negate(self, builder: &ConstraintSetBuilder<'_>) -> NodeId {
        let key = self.node();
        let storage = builder.storage.borrow();
        if let Some(result) = storage.negate_cache.get(&key) {
            return *result;
        }
        drop(storage);

        // negate(n ? C : U : D) = n ? negate(or(C, U)) : 0 : negate(or(D, U))
        //
        // The uncertain branch U is absorbed into C and D via union before negation. The result's
        // uncertain branch is always zero. When U = 0 (the common case), this degenerates to the
        // standard binary BDD leaf-swap: n ? negate(C) : 0 : negate(D).
        let interior = builder.interior_node_data(self.node());
        let not_true = interior.if_true.negate(builder);
        let not_uncertain = interior.if_uncertain.negate(builder);
        let not_false = interior.if_false.negate(builder);
        let result = NodeId::new(
            builder,
            interior.constraint,
            not_true.and(builder, not_uncertain),
            not_false.and(builder, not_uncertain),
            interior.source_order,
        );

        let mut storage = builder.storage.borrow_mut();
        storage.negate_cache.insert(key, result);
        result
    }

    fn or(self, builder: &ConstraintSetBuilder<'_>, other: Self, other_offset: usize) -> NodeId {
        let key = (self.node(), other.node(), other_offset);
        let storage = builder.storage.borrow();
        if let Some(result) = storage.or_cache.get(&key) {
            return *result;
        }
        drop(storage);

        let self_interior = builder.interior_node_data(self.node());
        let self_ordering = self_interior.constraint.ordering();
        let other_interior = builder.interior_node_data(other.node());
        let other_ordering = other_interior.constraint.ordering();
        let result = match self_ordering.cmp(&other_ordering) {
            Ordering::Equal => NodeId::with_uncertain(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .or_inner(builder, other_interior.if_true, other_offset),
                self_interior.if_uncertain.or_inner(
                    builder,
                    other_interior.if_uncertain,
                    other_offset,
                ),
                self_interior
                    .if_false
                    .or_inner(builder, other_interior.if_false, other_offset),
                self_interior.source_order,
            ),
            // This is from Frisch's original description of TDDs. If self < other, we check self
            // first. Instead of distributing other into the if_true and if_false branches, we
            // "park" it in the if_uncertain branch. That causes us to only evaluate other "lazily"
            // when needed.
            Ordering::Less => NodeId::with_uncertain(
                builder,
                self_interior.constraint,
                self_interior.if_true,
                self_interior
                    .if_uncertain
                    .or_inner(builder, other.node(), other_offset),
                self_interior.if_false,
                self_interior.source_order,
            ),
            // Ditto above but for the other variable ordering
            Ordering::Greater => NodeId::with_uncertain(
                builder,
                other_interior.constraint,
                other_interior.if_true,
                self.node()
                    .or_inner(builder, other_interior.if_uncertain, other_offset),
                other_interior.if_false,
                other_interior.source_order + other_offset,
            ),
        };

        let mut storage = builder.storage.borrow_mut();
        storage.or_cache.insert(key, result);
        result
    }

    fn and(self, builder: &ConstraintSetBuilder<'_>, other: Self, other_offset: usize) -> NodeId {
        let key = (self.node(), other.node(), other_offset);
        let storage = builder.storage.borrow();
        if let Some(result) = storage.and_cache.get(&key) {
            return *result;
        }
        drop(storage);

        let self_interior = builder.interior_node_data(self.node());
        let self_ordering = self_interior.constraint.ordering();
        let other_interior = builder.interior_node_data(other.node());
        let other_ordering = other_interior.constraint.ordering();
        let result = match self_ordering.cmp(&other_ordering) {
            // This is one of Duboc's optimizations over Frisch's original TDD operators. Frisch
            // always sets the if_uncertain branch to ALWAYS_FALSE, and always distributes both
            // input if_uncertain branches into the corresponding if_true and if_false branches.
            // Duboc propagates the input if_uncertain branches into the result's if_uncertain
            // branch.
            //
            //     n ? (C1 ∧ (C2 ∨ U2)) ∨ (U1 ∧ C2) : U1 ∧ U2 : (D1 ∧ (U2 ∨ D2)) ∨ (U1 ∧ D2)
            //
            // See [Duboc2026], §11.2 for more details.
            Ordering::Equal => {
                let if_true = self_interior
                    .if_true
                    .and_inner(
                        builder,
                        other_interior.if_true.or_inner(
                            builder,
                            other_interior.if_uncertain,
                            other_offset,
                        ),
                        other_offset,
                    )
                    .or_inner(
                        builder,
                        self_interior.if_uncertain.and_inner(
                            builder,
                            other_interior.if_true,
                            other_offset,
                        ),
                        0,
                    );
                let if_uncertain = self_interior.if_uncertain.and_inner(
                    builder,
                    other_interior.if_uncertain,
                    other_offset,
                );
                let if_false = self_interior
                    .if_false
                    .and_inner(
                        builder,
                        other_interior.if_uncertain.or_inner(
                            builder,
                            other_interior.if_false,
                            other_offset,
                        ),
                        other_offset,
                    )
                    .or_inner(
                        builder,
                        self_interior.if_uncertain.and_inner(
                            builder,
                            other_interior.if_false,
                            other_offset,
                        ),
                        0,
                    );
                NodeId::with_uncertain(
                    builder,
                    self_interior.constraint,
                    if_true,
                    if_uncertain,
                    if_false,
                    self_interior.source_order,
                )
            }
            Ordering::Less => NodeId::with_uncertain(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .and_inner(builder, other.node(), other_offset),
                self_interior
                    .if_uncertain
                    .and_inner(builder, other.node(), other_offset),
                self_interior
                    .if_false
                    .and_inner(builder, other.node(), other_offset),
                self_interior.source_order,
            ),
            Ordering::Greater => NodeId::with_uncertain(
                builder,
                other_interior.constraint,
                self.node()
                    .and_inner(builder, other_interior.if_true, other_offset),
                self.node()
                    .and_inner(builder, other_interior.if_uncertain, other_offset),
                self.node()
                    .and_inner(builder, other_interior.if_false, other_offset),
                other_interior.source_order + other_offset,
            ),
        };

        let mut storage = builder.storage.borrow_mut();
        storage.and_cache.insert(key, result);
        result
    }

    fn exists_one<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarIdentity<'db>,
    ) -> NodeId {
        let key = (self.node(), bound_typevar);
        let storage = builder.storage.borrow();
        if let Some(result) = storage.exists_one_cache.get(&key) {
            return *result;
        }
        drop(storage);

        let mut path = self.path_assignments(builder);
        let mentions_typevar = |ty: Type<'db>| match ty {
            Type::TypeVar(haystack) => haystack.identity(db) == bound_typevar,
            _ => false,
        };
        let result = self.abstract_one_inner(
            db,
            builder,
            // Remove any node that constrains `bound_typevar`, or that has a lower/upper bound
            // that mentions `bound_typevar`. The sequent map ensures that derived facts are
            // propagated for nested typevar references, using the variance of the typevar's
            // position to determine the correct substitution.
            &mut |constraint| {
                let constraint = builder.constraint_data(constraint);
                if constraint.typevar.identity(db) == bound_typevar {
                    return true;
                }
                if any_over_type(db, constraint.lower, false, mentions_typevar) {
                    return true;
                }
                if any_over_type(db, constraint.upper, false, mentions_typevar) {
                    return true;
                }
                false
            },
            &mut path,
        );

        let mut storage = builder.storage.borrow_mut();
        storage.exists_one_cache.insert(key, result);
        result
    }

    fn remove_noninferable<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> NodeId {
        let mut path = self.path_assignments(builder);
        let is_bare_inferable_typevar = |ty: Type<'db>| {
            ty.as_typevar()
                .is_some_and(|bound_typevar| bound_typevar.is_inferable(db, inferable))
        };
        self.abstract_one_inner(
            db,
            builder,
            // We only want to keep constraints on inferable typevars. If the constraint's typevar
            // is itself inferable, we keep it. We also need to keep some constraints in
            // non-inferable typevars, if their lower or upper bound is a bare inferable typevar.
            // This ensure that our quantification logic does not depend on typevar ordering.
            //
            // For example, `I ≤ N` (where I is inferable and N is non-inferable) could be encoded
            // either as `Never ≤ I ≤ N` or `I ≤ N ≤ object`, depending on typevar ordering. If we
            // only checked the inferability of the constrained typevar, we would keep the first
            // encoding but remove the second.
            &mut |constraint| {
                let constraint = builder.constraint_data(constraint);
                !constraint.typevar.is_inferable(db, inferable)
                    && !is_bare_inferable_typevar(constraint.lower)
                    && !is_bare_inferable_typevar(constraint.upper)
            },
            &mut path,
        )
    }

    fn abstract_one_inner<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        should_remove: &mut dyn FnMut(ConstraintId) -> bool,
        path: &mut PathAssignments,
    ) -> NodeId {
        let self_interior = builder.interior_node_data(self.node());
        if should_remove(self_interior.constraint) {
            // If we should remove constraints involving this typevar, then we replace this node
            // with the OR of all of its outgoing edges. That is, the result is true if there's
            // any assignment of this node's constraint that is true.
            //
            // We also have to check if there are any derived facts that depend on the constraint
            // we're about to remove. If so, we need to "remember" them by AND-ing them in with the
            // corresponding branch. We currently reuse the `source_order` of the constraint being
            // removed when we add these derived facts.
            //
            // TODO: This might not be stable enough, if we add more than one derived fact for this
            // constraint. If we still see inconsistent test output, we might need a more complex
            // way of tracking source order for derived facts.
            let if_true = path
                .walk_edge(
                    db,
                    builder,
                    self_interior.constraint.when_true(),
                    self_interior.source_order,
                    |path, new_range| {
                        let branch = self_interior.if_true.abstract_one_inner(
                            db,
                            builder,
                            should_remove,
                            path,
                        );
                        path.assignments[new_range]
                            .iter()
                            .filter(|(assignment, _)| {
                                // Don't add back any derived facts if they are ones that we would have
                                // removed!
                                !should_remove(assignment.constraint())
                            })
                            .fold(branch, |branch, (assignment, source_order)| {
                                branch.and(
                                    builder,
                                    Node::new_satisfied_constraint(
                                        builder,
                                        *assignment,
                                        *source_order,
                                    ),
                                )
                            })
                    },
                )
                .unwrap_or(ALWAYS_FALSE);
            let if_false = path
                .walk_edge(
                    db,
                    builder,
                    self_interior.constraint.when_false(),
                    self_interior.source_order,
                    |path, new_range| {
                        let branch = self_interior.if_false.abstract_one_inner(
                            db,
                            builder,
                            should_remove,
                            path,
                        );
                        path.assignments[new_range]
                            .iter()
                            .filter(|(assignment, _)| {
                                // Don't add back any derived facts if they are ones that we would have
                                // removed!
                                !should_remove(assignment.constraint())
                            })
                            .fold(branch, |branch, (assignment, source_order)| {
                                branch.and(
                                    builder,
                                    Node::new_satisfied_constraint(
                                        builder,
                                        *assignment,
                                        *source_order,
                                    ),
                                )
                            })
                    },
                )
                .unwrap_or(ALWAYS_FALSE);
            let if_uncertain = path
                .walk_edge(
                    db,
                    builder,
                    self_interior.constraint.when_unconstrained(),
                    self_interior.source_order,
                    |path, new_range| {
                        let branch = self_interior.if_uncertain.abstract_one_inner(
                            db,
                            builder,
                            should_remove,
                            path,
                        );
                        path.assignments[new_range]
                            .iter()
                            .filter(|(assignment, _)| !should_remove(assignment.constraint()))
                            .fold(branch, |branch, (assignment, source_order)| {
                                branch.and(
                                    builder,
                                    Node::new_satisfied_constraint(
                                        builder,
                                        *assignment,
                                        *source_order,
                                    ),
                                )
                            })
                    },
                )
                .unwrap_or(ALWAYS_FALSE);
            if_true.or(builder, if_uncertain).or(builder, if_false)
        } else {
            // Otherwise, we abstract the if_false/if_true edges recursively.
            let if_true = path
                .walk_edge(
                    db,
                    builder,
                    self_interior.constraint.when_true(),
                    self_interior.source_order,
                    |path, _| {
                        self_interior
                            .if_true
                            .abstract_one_inner(db, builder, should_remove, path)
                    },
                )
                .unwrap_or(ALWAYS_FALSE);
            let if_uncertain = path
                .walk_edge(
                    db,
                    builder,
                    self_interior.constraint.when_unconstrained(),
                    self_interior.source_order,
                    |path, _| {
                        self_interior.if_uncertain.abstract_one_inner(
                            db,
                            builder,
                            should_remove,
                            path,
                        )
                    },
                )
                .unwrap_or(ALWAYS_FALSE);
            let if_false = path
                .walk_edge(
                    db,
                    builder,
                    self_interior.constraint.when_false(),
                    self_interior.source_order,
                    |path, _| {
                        self_interior
                            .if_false
                            .abstract_one_inner(db, builder, should_remove, path)
                    },
                )
                .unwrap_or(ALWAYS_FALSE);
            // Absorb the uncertain branch into both the true and false branches before
            // constructing the ITE, matching TDD semantics: when the constraint holds the result
            // is C ∨ U, and when it doesn't the result is D ∨ U.
            //
            // NB: We cannot use `Node::new` here, because the recursive calls might introduce new
            // derived constraints into the result, and those constraints might appear before this
            // one in the BDD ordering.
            Node::new_constraint(
                builder,
                self_interior.constraint,
                self_interior.source_order,
            )
            .ite(
                builder,
                if_true.or(builder, if_uncertain),
                if_false.or(builder, if_uncertain),
            )
        }
    }

    fn restrict_one<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        assignment: ConstraintAssignment,
    ) -> (NodeId, bool) {
        let key = (self.node(), assignment);
        let storage = builder.storage.borrow();
        if let Some(result) = storage.restrict_one_cache.get(&key) {
            return *result;
        }
        drop(storage);

        let self_interior = builder.interior_node_data(self.node());
        let self_ordering = self_interior.constraint.ordering();
        let result = if assignment.constraint().ordering() < self_ordering {
            // If this node's variable is larger than the assignment's variable, then we have reached a
            // point in the BDD where the assignment can no longer affect the result,
            // and we can return early.
            (self.node(), false)
        } else {
            // Otherwise, check if this node's variable is in the assignment. If so, substitute the
            // variable by replacing this node with the appropriate edge(s). When restricting a
            // TDD, the uncertain branch is folded in.
            if assignment == self_interior.constraint.when_true() {
                // restrict(n? C: U: D, n == true) = C ∨ U
                (
                    self_interior
                        .if_true
                        .or(builder, self_interior.if_uncertain),
                    true,
                )
            } else if assignment == self_interior.constraint.when_false() {
                // restrict(n? C: U: D, n == false) = D ∨ U
                (
                    self_interior
                        .if_false
                        .or(builder, self_interior.if_uncertain),
                    true,
                )
            } else if assignment == self_interior.constraint.when_unconstrained() {
                // restrict(n? C: U: D, n is unconstrained) = C ∨ U ∨ D
                (
                    self_interior
                        .if_true
                        .or(builder, self_interior.if_uncertain)
                        .or(builder, self_interior.if_false),
                    true,
                )
            } else {
                let (if_true, found_in_true) =
                    self_interior.if_true.restrict_one(db, builder, assignment);
                let (if_uncertain, found_in_uncertain) = self_interior
                    .if_uncertain
                    .restrict_one(db, builder, assignment);
                let (if_false, found_in_false) =
                    self_interior.if_false.restrict_one(db, builder, assignment);
                (
                    NodeId::with_uncertain(
                        builder,
                        self_interior.constraint,
                        if_true,
                        if_uncertain,
                        if_false,
                        self_interior.source_order,
                    ),
                    found_in_true || found_in_uncertain || found_in_false,
                )
            }
        };

        let mut storage = builder.storage.borrow_mut();
        storage.restrict_one_cache.insert(key, result);
        result
    }

    fn path_assignments(self, builder: &ConstraintSetBuilder<'_>) -> PathAssignments {
        // Sort the constraints in this BDD by their `source_order`s before adding them to the
        // sequent map. This ensures that constraints appear in the sequent map in a stable order.
        // The constraints mentioned in a BDD should all have distinct `source_order`s, so an
        // unstable sort is fine.
        let mut constraints: SmallVec<[_; 8]> = SmallVec::new();
        self.node()
            .for_each_constraint(builder, &mut |constraint, source_order| {
                constraints.push((constraint, source_order));
            });
        constraints.sort_unstable_by_key(|(_, source_order)| *source_order);

        PathAssignments::new(constraints.into_iter().map(|(constraint, _)| constraint))
    }

    /// Returns a simplified version of a BDD.
    ///
    /// This is calculated by looking at the relationships that exist between the constraints that
    /// are mentioned in the BDD. For instance, if one constraint implies another (`x → y`), then
    /// `x ∧ ¬y` is not a valid input, and we can rewrite any occurrences of `x ∨ y` into `y`.
    fn simplify<'db>(self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> NodeId {
        let key = self.node();
        let storage = builder.storage.borrow();
        if let Some(result) = storage.simplify_cache.get(&key) {
            return *result;
        }
        drop(storage);

        // To simplify a non-terminal BDD, we find all pairs of constraints that are mentioned in
        // the BDD. If any of those pairs can be simplified to some other BDD, we perform a
        // substitution to replace the pair with the simplification.
        //
        // Some of the simplifications create _new_ constraints that weren't originally present in
        // the BDD. If we encounter one of those cases, we need to check if we can simplify things
        // further relative to that new constraint.
        //
        // To handle this, we keep track of the individual constraints that we have already
        // discovered (`seen_constraints`), and a queue of constraint pairs that we still need to
        // check (`to_visit`).

        // Seed the seen set with all of the constraints that are present in the input BDD, and the
        // visit queue with all pairs of those constraints. (We use "combinations" because we don't
        // need to compare a constraint against itself, and because ordering doesn't matter.)
        let mut seen_constraints = FxHashSet::default();
        let mut source_orders = FxHashMap::default();
        self.node()
            .for_each_constraint(builder, &mut |constraint, source_order| {
                seen_constraints.insert(constraint);
                source_orders.insert(constraint, source_order);
            });
        let mut to_visit: Vec<(_, _)> = (seen_constraints.iter().copied())
            .tuple_combinations()
            .collect();

        // Repeatedly pop constraint pairs off of the visit queue, checking whether each pair can
        // be simplified. If we add any derived constraints, we will place them at the end in
        // source order. (We do not have any test cases that depend on constraint sets being
        // displayed in a consistent ordering, so we don't need to be clever in assigning these
        // `source_order`s.)
        let mut simplified = self.node();
        let self_interior = builder.interior_node_data(self.node());
        let mut next_source_order = self_interior.max_source_order + 1;
        while let Some((left_constraint, right_constraint)) = to_visit.pop() {
            let left_source_order = source_orders[&left_constraint];
            let right_source_order = source_orders[&right_constraint];

            // If the constraints refer to different typevars, the only simplifications we can make
            // are of the form `S ≤ T ∧ T ≤ int → S ≤ int`.
            let left_constraint_data = builder.constraint_data(left_constraint);
            let left_typevar = left_constraint_data.typevar;
            let right_constraint_data = builder.constraint_data(right_constraint);
            let right_typevar = right_constraint_data.typevar;
            if !left_typevar.is_same_typevar_as(db, right_typevar) {
                // We've structured our constraints so that a typevar's upper/lower bound can only
                // be another typevar if the bound is "later" in our arbitrary ordering. That means
                // we only have to check this pair of constraints in one direction — though we do
                // have to figure out which of the two typevars is constrained, and which one is
                // the upper/lower bound.
                let (bound_constraint, constrained_constraint) =
                    if left_typevar.can_be_bound_for(db, builder, right_typevar) {
                        (left_constraint, right_constraint)
                    } else {
                        (right_constraint, left_constraint)
                    };
                let bound_constraint_data = builder.constraint_data(bound_constraint);
                let bound_typevar = bound_constraint_data.typevar;
                let constrained_constraint_data = builder.constraint_data(constrained_constraint);
                let constrained_typevar = constrained_constraint_data.typevar;

                // We then look for cases where the "constrained" typevar's upper and/or lower
                // bound matches the "bound" typevar. If so, we're going to add an implication to
                // the constraint set that replaces the upper/lower bound that matched with the
                // bound constraint's corresponding bound.
                let (new_lower, new_upper) = match (
                    constrained_constraint_data.lower,
                    constrained_constraint_data.upper,
                ) {
                    // (B ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ BU)
                    (Type::TypeVar(constrained_lower), Type::TypeVar(constrained_upper))
                        if constrained_lower.is_same_typevar_as(db, bound_typevar)
                            && constrained_upper.is_same_typevar_as(db, bound_typevar) =>
                    {
                        (bound_constraint_data.lower, bound_constraint_data.upper)
                    }

                    // (CL ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (CL ≤ C ≤ BU)
                    (constrained_lower, Type::TypeVar(constrained_upper))
                        if constrained_upper.is_same_typevar_as(db, bound_typevar) =>
                    {
                        (constrained_lower, bound_constraint_data.upper)
                    }

                    // (B ≤ C ≤ CU) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ CU)
                    (Type::TypeVar(constrained_lower), constrained_upper)
                        if constrained_lower.is_same_typevar_as(db, bound_typevar) =>
                    {
                        (bound_constraint_data.lower, constrained_upper)
                    }

                    _ => continue,
                };

                let new_constraint =
                    ConstraintId::new(db, builder, constrained_typevar, new_lower, new_upper);
                if seen_constraints.contains(&new_constraint) {
                    continue;
                }
                let new_node = Node::new_constraint(builder, new_constraint, next_source_order);
                next_source_order += 1;
                let positive_left_node = Node::new_satisfied_constraint(
                    builder,
                    left_constraint.when_true(),
                    left_source_order,
                );
                let positive_right_node = Node::new_satisfied_constraint(
                    builder,
                    right_constraint.when_true(),
                    right_source_order,
                );
                let lhs = positive_left_node.and(builder, positive_right_node);
                let intersection = new_node.ite(builder, lhs, ALWAYS_FALSE);
                simplified = simplified.and(builder, intersection);
                continue;
            }

            // From here on out we know that both constraints constrain the same typevar. The
            // clause above will propagate all that we know about the current typevar relative to
            // other typevars, producing constraints on this typevar that have concrete lower/upper
            // bounds. That means we can skip the simplifications below if any bound is another
            // typevar.
            if left_constraint_data.lower.is_type_var()
                || left_constraint_data.upper.is_type_var()
                || right_constraint_data.lower.is_type_var()
                || right_constraint_data.upper.is_type_var()
            {
                continue;
            }

            // Containment: The range of one constraint might completely contain the range of the
            // other. If so, there are several potential simplifications.
            let larger_smaller = if left_constraint.implies(db, builder, right_constraint) {
                Some((
                    right_constraint,
                    right_source_order,
                    left_constraint,
                    left_source_order,
                ))
            } else if right_constraint.implies(db, builder, left_constraint) {
                Some((
                    left_constraint,
                    left_source_order,
                    right_constraint,
                    right_source_order,
                ))
            } else {
                None
            };
            if let Some((
                larger_constraint,
                larger_source_order,
                smaller_constraint,
                smaller_source_order,
            )) = larger_smaller
            {
                let positive_larger_node = Node::new_satisfied_constraint(
                    builder,
                    larger_constraint.when_true(),
                    larger_source_order,
                );
                let negative_larger_node = Node::new_satisfied_constraint(
                    builder,
                    larger_constraint.when_false(),
                    larger_source_order,
                );

                // larger ∨ smaller = larger
                simplified = simplified.substitute_union(
                    db,
                    builder,
                    larger_constraint.when_true(),
                    larger_source_order,
                    smaller_constraint.when_true(),
                    smaller_source_order,
                    positive_larger_node,
                );

                // ¬larger ∧ ¬smaller = ¬larger
                simplified = simplified.substitute_intersection(
                    db,
                    builder,
                    larger_constraint.when_false(),
                    larger_source_order,
                    smaller_constraint.when_false(),
                    smaller_source_order,
                    negative_larger_node,
                );

                // smaller ∧ ¬larger = false
                // (¬larger removes everything that's present in smaller)
                simplified = simplified.substitute_intersection(
                    db,
                    builder,
                    larger_constraint.when_false(),
                    larger_source_order,
                    smaller_constraint.when_true(),
                    smaller_source_order,
                    ALWAYS_FALSE,
                );

                // larger ∨ ¬smaller = true
                // (larger fills in everything that's missing in ¬smaller)
                simplified = simplified.substitute_union(
                    db,
                    builder,
                    larger_constraint.when_true(),
                    larger_source_order,
                    smaller_constraint.when_false(),
                    smaller_source_order,
                    ALWAYS_TRUE,
                );
            }

            // There are some simplifications we can make when the intersection of the two
            // constraints is empty, and others that we can make when the intersection is
            // non-empty.
            match left_constraint.intersect(db, builder, right_constraint) {
                IntersectionResult::Simplified(intersection_constraint_data) => {
                    let intersection_constraint =
                        builder.intern_constraint(db, intersection_constraint_data);

                    // If the intersection is non-empty, we need to create a new constraint to
                    // represent that intersection. We also need to add the new constraint to our
                    // seen set and (if we haven't already seen it) to the to-visit queue.
                    if seen_constraints.insert(intersection_constraint) {
                        source_orders.insert(intersection_constraint, next_source_order);
                        to_visit.extend(
                            (seen_constraints.iter().copied())
                                .filter(|seen| *seen != intersection_constraint)
                                .map(|seen| (seen, intersection_constraint)),
                        );
                    }
                    let positive_intersection_node = Node::new_satisfied_constraint(
                        builder,
                        intersection_constraint.when_true(),
                        next_source_order,
                    );
                    let negative_intersection_node = Node::new_satisfied_constraint(
                        builder,
                        intersection_constraint.when_false(),
                        next_source_order,
                    );
                    next_source_order += 1;

                    let positive_left_node = Node::new_satisfied_constraint(
                        builder,
                        left_constraint.when_true(),
                        left_source_order,
                    );
                    let negative_left_node = Node::new_satisfied_constraint(
                        builder,
                        left_constraint.when_false(),
                        left_source_order,
                    );

                    let positive_right_node = Node::new_satisfied_constraint(
                        builder,
                        right_constraint.when_true(),
                        right_source_order,
                    );
                    let negative_right_node = Node::new_satisfied_constraint(
                        builder,
                        right_constraint.when_false(),
                        right_source_order,
                    );

                    // left ∧ right = intersection
                    simplified = simplified.substitute_intersection(
                        db,
                        builder,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        positive_intersection_node,
                    );

                    // ¬left ∨ ¬right = ¬intersection
                    simplified = simplified.substitute_union(
                        db,
                        builder,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        negative_intersection_node,
                    );

                    // left ∧ ¬right = left ∧ ¬intersection
                    // (clip the negative constraint to the smallest range that actually removes
                    // something from positive constraint)
                    simplified = simplified.substitute_intersection(
                        db,
                        builder,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        positive_left_node.and(builder, negative_intersection_node),
                    );

                    // ¬left ∧ right = ¬intersection ∧ right
                    // (save as above but reversed)
                    simplified = simplified.substitute_intersection(
                        db,
                        builder,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        positive_right_node.and(builder, negative_intersection_node),
                    );

                    // left ∨ ¬right = intersection ∨ ¬right
                    // (clip the positive constraint to the smallest range that actually adds
                    // something to the negative constraint)
                    simplified = simplified.substitute_union(
                        db,
                        builder,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        negative_right_node.or(builder, positive_intersection_node),
                    );

                    // ¬left ∨ right = ¬left ∨ intersection
                    // (save as above but reversed)
                    simplified = simplified.substitute_union(
                        db,
                        builder,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        negative_left_node.or(builder, positive_intersection_node),
                    );
                }

                // If the intersection doesn't simplify to a single clause, we shouldn't update the
                // BDD.
                IntersectionResult::CannotSimplify => {}

                IntersectionResult::Disjoint => {
                    // All of the below hold because we just proved that the intersection of left
                    // and right is empty.

                    let positive_left_node = Node::new_satisfied_constraint(
                        builder,
                        left_constraint.when_true(),
                        left_source_order,
                    );
                    let positive_right_node = Node::new_satisfied_constraint(
                        builder,
                        right_constraint.when_true(),
                        right_source_order,
                    );

                    // left ∧ right = false
                    simplified = simplified.substitute_intersection(
                        db,
                        builder,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        ALWAYS_FALSE,
                    );

                    // ¬left ∨ ¬right = true
                    simplified = simplified.substitute_union(
                        db,
                        builder,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        ALWAYS_TRUE,
                    );

                    // left ∧ ¬right = left
                    // (there is nothing in the hole of ¬right that overlaps with left)
                    simplified = simplified.substitute_intersection(
                        db,
                        builder,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        positive_left_node,
                    );

                    // ¬left ∧ right = right
                    // (save as above but reversed)
                    simplified = simplified.substitute_intersection(
                        db,
                        builder,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        positive_right_node,
                    );
                }
            }
        }

        let mut storage = builder.storage.borrow_mut();
        storage.simplify_cache.insert(key, simplified);
        simplified
    }
}

/// The result of solving a constraint set for per-typevar specializations.
#[derive(Debug)]
pub(crate) enum Solutions<'db> {
    Unsatisfiable,
    Unconstrained,
    Constrained(Vec<Solution<'db>>),
}

pub(crate) type Solution<'db> = Vec<TypeVarSolution<'db>>;

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub(crate) struct TypeVarSolution<'db> {
    pub(crate) bound_typevar: BoundTypeVarInstance<'db>,
    pub(crate) solution: Type<'db>,
}

/// An assignment of one BDD variable to either `true` or `false`. (When evaluating a BDD, we
/// must provide an assignment for each variable present in the BDD.)
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub(crate) enum ConstraintAssignment {
    Positive(ConstraintId),
    Negative(ConstraintId),
    Unconstrained(ConstraintId),
}

impl ConstraintAssignment {
    fn constraint(self) -> ConstraintId {
        match self {
            ConstraintAssignment::Positive(constraint) => constraint,
            ConstraintAssignment::Negative(constraint) => constraint,
            ConstraintAssignment::Unconstrained(constraint) => constraint,
        }
    }

    fn negated(self) -> Self {
        match self {
            ConstraintAssignment::Positive(constraint) => {
                ConstraintAssignment::Negative(constraint)
            }
            ConstraintAssignment::Negative(constraint) => {
                ConstraintAssignment::Positive(constraint)
            }
            // "This constraint can go either way" is symmetric under negation.
            ConstraintAssignment::Unconstrained(constraint) => {
                ConstraintAssignment::Unconstrained(constraint)
            }
        }
    }

    fn negate(&mut self) {
        *self = self.negated();
    }

    /// Returns whether this constraint implies another — i.e., whether every type that
    /// satisfies this constraint also satisfies `other`.
    ///
    /// This is used to simplify how we display constraint sets, by removing redundant constraints
    /// from a clause.
    fn implies<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        other: Self,
    ) -> bool {
        match (self, other) {
            // For two positive constraints, one range has to fully contain the other; the smaller
            // constraint implies the larger.
            //
            //     ....|----other-----|....
            //     ......|---self---|......
            (
                ConstraintAssignment::Positive(self_constraint),
                ConstraintAssignment::Positive(other_constraint),
            ) => self_constraint.implies(db, builder, other_constraint),

            // For two negative constraints, one range has to fully contain the other; the ranges
            // represent "holes", though, so the constraint with the larger range implies the one
            // with the smaller.
            //
            //     |-----|...other...|-----|
            //     |---|.....self......|---|
            (
                ConstraintAssignment::Negative(self_constraint),
                ConstraintAssignment::Negative(other_constraint),
            ) => other_constraint.implies(db, builder, self_constraint),

            // For a positive and negative constraint, the ranges have to be disjoint, and the
            // positive range implies the negative range.
            //
            //     |---------------|...self...|---|
            //     ..|---other---|................|
            (
                ConstraintAssignment::Positive(self_constraint),
                ConstraintAssignment::Negative(other_constraint),
            ) => self_constraint
                .intersect(db, builder, other_constraint)
                .is_disjoint(),

            // It's theoretically possible for a negative constraint to imply a positive constraint
            // if the positive constraint is always satisfied (`Never ≤ T ≤ object`). But we never
            // create constraints of that form, so with our representation, a negative constraint
            // can never imply a positive constraint.
            //
            //     |------other-------|
            //     |---|...self...|---|
            (ConstraintAssignment::Negative(_), ConstraintAssignment::Positive(_)) => false,

            // An `Unconstrained` assignment means "this constraint can go either way." It does
            // not imply any positive or negative assignment, and no positive or negative
            // assignment implies it. The only trivially true case is Unconstrained => Unconstrained
            // for the same constraint.
            (
                ConstraintAssignment::Unconstrained(self_constraint),
                ConstraintAssignment::Unconstrained(other_constraint),
            ) => self_constraint == other_constraint,
            (ConstraintAssignment::Unconstrained(_), _)
            | (_, ConstraintAssignment::Unconstrained(_)) => false,
        }
    }

    fn display<'db>(self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> impl Display {
        struct DisplayConstraintAssignment<'db, 'c> {
            assignment: ConstraintAssignment,
            db: &'db dyn Db,
            builder: &'c ConstraintSetBuilder<'db>,
        }

        impl DisplayConstraintAssignment<'_, '_> {
            fn equality_sign(&self) -> &'static str {
                match self.assignment {
                    ConstraintAssignment::Positive(_) => "=",
                    ConstraintAssignment::Negative(_) => "≠",
                    ConstraintAssignment::Unconstrained(_) => "=?",
                }
            }

            fn range_prefix(&self) -> &'static str {
                match self.assignment {
                    ConstraintAssignment::Positive(_) => "",
                    ConstraintAssignment::Negative(_) => "¬",
                    ConstraintAssignment::Unconstrained(_) => "?",
                }
            }
        }

        impl Display for DisplayConstraintAssignment<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let constraint_data = self.builder.constraint_data(self.assignment.constraint());
                let lower = constraint_data.lower;
                let upper = constraint_data.upper;
                let typevar = constraint_data.typevar;
                if lower.is_equivalent_to(self.db, upper) {
                    // If this typevar is equivalent to another, output the constraint in a
                    // consistent alphabetical order, regardless of the salsa ordering that we are
                    // using the in BDD.
                    if let Type::TypeVar(bound) = lower {
                        let bound = bound.identity(self.db).display(self.db).to_string();
                        let typevar = typevar.identity(self.db).display(self.db).to_string();
                        let (smaller, larger) = if bound < typevar {
                            (bound, typevar)
                        } else {
                            (typevar, bound)
                        };
                        return write!(f, "({} {} {})", smaller, self.equality_sign(), larger);
                    }

                    return write!(
                        f,
                        "({} {} {})",
                        typevar.identity(self.db).display(self.db),
                        self.equality_sign(),
                        lower.display(self.db)
                    );
                }

                if lower.is_never() && upper.is_object() {
                    return write!(
                        f,
                        "({} {} *)",
                        typevar.identity(self.db).display(self.db),
                        self.equality_sign()
                    );
                }

                f.write_str(self.range_prefix())?;
                f.write_str("(")?;
                if !lower.is_never() {
                    write!(f, "{} ≤ ", lower.display(self.db))?;
                }
                typevar.identity(self.db).display(self.db).fmt(f)?;
                if !upper.is_object() {
                    write!(f, " ≤ {}", upper.display(self.db))?;
                }
                f.write_str(")")
            }
        }

        DisplayConstraintAssignment {
            assignment: self,
            db,
            builder,
        }
    }
}

/// A collection of _sequents_ that describe how the constraints mentioned in a BDD relate to each
/// other. These are used in several BDD operations that need to know about "derived facts" even if
/// they are not mentioned in the BDD directly. These operations involve walking one or more paths
/// from the root node to a terminal node. Each sequent describes paths that are invalid (which are
/// pruned from the search), and new constraints that we can assume to be true even if we haven't
/// seen them directly.
///
/// We support several kinds of sequent:
///
/// - `¬C₁ → false`: This indicates that `C₁` is always true. Any path that assumes it is false is
///   impossible and can be pruned.
///
/// - `C₁ ∧ C₂ → false`: This indicates that `C₁` and `C₂` are disjoint: it is not possible for
///   both to hold. Any path that assumes both is impossible and can be pruned.
///
/// - `C₁ ∧ C₂ → D`: This indicates that the intersection of `C₁` and `C₂` can be simplified to
///   `D`. Any path that assumes both `C₁` and `C₂` hold, but assumes `D` does _not_, is impossible
///   and can be pruned.
///
/// - `C → D`: This indicates that `C` on its own is enough to imply `D`. Any path that assumes `C`
///   holds but `D` does _not_ is impossible and can be pruned.
///
/// Sequent maps are primarily used when walking a BDD path with a [`PathAssignments`]. The
/// `PathAssignments` will hold a sequent map containing all of the constraints that are
/// encountered during the walk. It builds up its sequent map lazily, so that it only has to
/// include sequents for the constraints that are actually encountered. However, we also don't want
/// to perform duplicate work if we perform multiple BDD walks on the same constraint set. The
/// [`for_constraint`][Self::for_constraint] and [`for_constraint_pair`][Self::for_constraint_pair]
/// methods are salsa-tracked, to ensure that we only perform them once for any particular
/// constraint or pair of constraints. `PathAssignments` invokes these methods when it encounters a
/// new constraint, and then merges those cached sequents into its own sequent map. (That means we
/// also share the work of calculating the sequent map across `PathAssignments` for _different_
/// constraint sets.)
#[derive(Clone, Debug, Default, Eq, PartialEq, get_size2::GetSize)]
struct SequentMap {
    /// Sequents of the form `¬C₁ → false`
    single_tautologies: FxHashSet<ConstraintId>,
    /// Sequents of the form `C₁ ∧ C₂ → false`
    pair_impossibilities: FxHashSet<(ConstraintId, ConstraintId)>,
    /// Sequents of the form `C₁ ∧ C₂ → D`
    pair_implications: FxIndexMap<(ConstraintId, ConstraintId), FxIndexSet<DerivedConstraint>>,
    /// Sequents of the form `C → D`
    single_implications: FxIndexMap<ConstraintId, FxIndexSet<ConstraintId>>,
}

impl SequentMap {
    /// Returns a sequent map containing the sequents that we can infer from a single constraint in
    /// isolation. This method is salsa-tracked so that we only perform this work once per
    /// constraint.
    fn for_constraint<'db, 'c>(
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        constraint: ConstraintId,
    ) -> Ref<'c, Self> {
        let key = constraint;
        let storage = builder.storage.borrow();
        if let Ok(map) = Ref::filter_map(storage, |storage| storage.single_sequent_cache.get(&key))
        {
            return map;
        }

        tracing::trace!(
            target: "ty_python_semantic::types::constraints::SequentMap",
            constraint = %constraint.display(db, builder),
            "add sequents for constraint",
        );
        let mut map = SequentMap::default();
        map.add_sequents_for_single(db, builder, constraint);

        let mut storage = builder.storage.borrow_mut();
        storage.single_sequent_cache.insert(key, map);
        drop(storage);

        let storage = builder.storage.borrow();
        Ref::map(storage, |storage| &storage.single_sequent_cache[&key])
    }

    /// Returns a sequent map containing the sequents that we can infer from a pair of constraints.
    /// This method is salsa-tracked so that we only perform this work once per constraint pair.
    ///
    /// (Note that this method is _not_ commutative; you should provide `left` and `right` in the
    /// order that they appear in the source code, so that we can construct derived constraints
    /// that retain that ordering.)
    fn for_constraint_pair<'db, 'c>(
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        left: ConstraintId,
        right: ConstraintId,
    ) -> Ref<'c, Self> {
        let key = (left, right);
        let storage = builder.storage.borrow();
        if let Ok(map) = Ref::filter_map(storage, |storage| storage.pair_sequent_cache.get(&key)) {
            return map;
        }

        tracing::trace!(
            target: "ty_python_semantic::types::constraints::SequentMap",
            left = %left.display(db, builder),
            right = %right.display(db, builder),
            "add sequents for constraint pair",
        );
        let mut map = SequentMap::default();
        map.add_sequents_for_pair(db, builder, left, right);

        let mut storage = builder.storage.borrow_mut();
        storage.pair_sequent_cache.insert(key, map);
        drop(storage);

        let storage = builder.storage.borrow();
        Ref::map(storage, |storage| &storage.pair_sequent_cache[&key])
    }

    /// Merges the sequents from another sequent map into this one.
    fn merge(&mut self, other: &Self) {
        self.single_tautologies.extend(&other.single_tautologies);
        self.pair_impossibilities
            .extend(&other.pair_impossibilities);
        for ((ante1, ante2), post) in &other.pair_implications {
            self.pair_implications
                .entry(Self::pair_key(*ante1, *ante2))
                .or_default()
                .extend(post);
        }
        for (ante, post) in &other.single_implications {
            self.single_implications
                .entry(*ante)
                .or_default()
                .extend(post);
        }
    }

    fn pair_key(ante1: ConstraintId, ante2: ConstraintId) -> (ConstraintId, ConstraintId) {
        if ante1.ordering() < ante2.ordering() {
            (ante1, ante2)
        } else {
            (ante2, ante1)
        }
    }

    fn add_single_tautology<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        ante: ConstraintId,
    ) {
        if self.single_tautologies.insert(ante) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                sequent = %format_args!("¬{} → false", ante.display(db, builder)),
                "add sequent",
            );
        }
    }

    fn add_pair_impossibility<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        ante1: ConstraintId,
        ante2: ConstraintId,
    ) {
        if self
            .pair_impossibilities
            .insert(Self::pair_key(ante1, ante2))
        {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                sequent = %format_args!(
                    "{} ∧ {} → false",
                    ante1.display(db, builder),
                    ante2.display(db, builder),
                ),
                "add sequent",
            );
        }
    }

    fn add_pair_implication<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        ante1: ConstraintId,
        ante2: ConstraintId,
        post: ConstraintId,
    ) {
        self.add_pair_implication_with_provenance(db, builder, ante1, ante2, post, None);
    }

    fn add_pair_implication_with_provenance<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        ante1: ConstraintId,
        ante2: ConstraintId,
        post: ConstraintId,
        nested_substitution: Option<NestedSubstitution>,
    ) {
        // If the post constraint is unsatisfiable, then the antecedents contradict each other.
        let post_data = builder.constraint_data(post);
        let when = post_data
            .lower
            .when_constraint_set_assignable_to(db, post_data.upper, builder);
        if when.is_never_satisfied(db) {
            self.add_pair_impossibility(db, builder, ante1, ante2);
            return;
        }

        // If either antecedent implies the consequent on its own, this new sequent is redundant.
        if ante1.implies(db, builder, post) || ante2.implies(db, builder, post) {
            return;
        }
        let derived = DerivedConstraint {
            constraint: post,
            nested_substitution,
        };
        if self
            .pair_implications
            .entry(Self::pair_key(ante1, ante2))
            .or_default()
            .insert(derived)
        {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                sequent = %format_args!(
                    "{} ∧ {} → {}",
                    ante1.display(db, builder),
                    ante2.display(db, builder),
                    post.display(db, builder),
                ),
                "add sequent",
            );
        }
    }

    fn add_single_implication<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        ante: ConstraintId,
        post: ConstraintId,
    ) {
        if ante == post {
            return;
        }
        if self
            .single_implications
            .entry(ante)
            .or_default()
            .insert(post)
        {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                sequent = %format_args!(
                    "{} → {}",
                    ante.display(db, builder),
                    post.display(db, builder),
                ),
                "add sequent",
            );
        }
    }

    fn add_sequents_for_single<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        constraint: ConstraintId,
    ) {
        // If this constraint binds its typevar to `Never ≤ T ≤ object`, then the typevar can take
        // on any type, and the constraint is always satisfied.
        let constraint_data = builder.constraint_data(constraint);
        let lower = constraint_data.lower;
        let upper = constraint_data.upper;
        if lower.is_never() && upper.is_object() {
            self.add_single_tautology(db, builder, constraint);
            return;
        }

        // Given a constraint `L ≤ T ≤ U`, `L ≤ U` must also hold. If those bounds contain other
        // typevars, we can infer additional constraints. This is easiest to see when the bounds
        // _are_ typevars:
        //
        //   1. `(S ≤ T ≤ U) → (S ≤ U)`
        //   2. `(S ≤ T ≤ τ) → (S ≤ τ)`
        //   3. `(τ ≤ T ≤ U) → (τ ≤ U)`
        //
        // but it also holds when the bounds _contain_ typevars:
        //
        //   4. `(Covariant[S] ≤ T ≤ Covariant[U]) → (S ≤ U)`
        //      `(Covariant[S] ≤ T ≤ Covariant[τ]) → (S ≤ τ)`
        //      `(Covariant[τ] ≤ T ≤ Covariant[U]) → (τ ≤ U)`
        //
        //   5. `(Contravariant[S] ≤ T ≤ Contravariant[U]) → (U ≤ S)`
        //      `(Contravariant[S] ≤ T ≤ Contravariant[τ]) → (τ ≤ S)`
        //      `(Contravariant[τ] ≤ T ≤ Contravariant[U]) → (U ≤ τ)`
        //
        //   6. `(Invariant[S] ≤ T ≤ Invariant[U]) → (S = U)`
        //      `(Invariant[S] ≤ T ≤ Invariant[τ]) → (S = τ)`
        //      `(Invariant[τ] ≤ T ≤ Invariant[U]) → (τ = U)`
        //
        // and whenever the bounds are assignable, even if they don't mention exactly the same
        // types:
        //
        //   class Sub(Covariant[int]): ...
        //
        //   7. `(Covariant[S] ≤ T ≤ Sub) → (S ≤ int)`
        //      `(Sub ≤ T ≤ Covariant[U]) → (int ≤ U)`
        //
        // To handle all of these cases, we perform a constraint set assignability check to see
        // when `L ≤ U`. This gives us a constraint set, which should be the rhs of the sequent
        // implication. (That is, this check directly encodes `(L ≤ T ≤ U) → (L ≤ U)` as an
        // implication.)

        // Skip trivial cases where the assignability check won't produce useful results.
        if lower.is_never() || upper.is_object() {
            return;
        }

        let when = lower.when_constraint_set_assignable_to(db, upper, builder);

        // If L is _never_ assignable to U, this constraint would violate transitivity, and should
        // never have been added.
        debug_assert!(!when.is_never_satisfied(db));

        // Fast path: If L is trivially always assignable to U, there are no derived constraints
        // that we can infer. This would be handled correctly by the logic below, but this is a
        // useful early return. Since we only use this check as an early return happy path, we can
        // accept false negatives. That lets us use the simpler and cheaper check against
        // ALWAYS_TRUE, rather than a more expensive is_always_satisfiable call.
        if when.node == ALWAYS_TRUE {
            return;
        }

        // Technically, we've just calculated a _constraint set_ as the rhs of this implication.
        // Unfortunately, our sequent map can currently only store implications where the rhs is a
        // single constraint.
        //
        // If the constraint set that we get represents a single conjunction, we can still shoehorn
        // it into this shape, since we can "break apart" a conjunction on the rhs of an
        // implication:
        //
        //   a → b ∧ c ∧ d
        //
        // becomes
        //
        //   a → b
        //   a → c
        //   a → d
        //
        // That takes care of breaking apart the rhs conjunction: we can add each positive
        // constraint as a separate single_implication.
        //
        // We can also handle _negative_ constraints, because those turn into impossibilities:
        //
        //   a → ¬b
        //
        // becomes
        //
        //   a ∧ b → false
        //
        // TODO: This should handle the most common cases. In the future, we could handle arbitrary
        // rhs constraint sets by moving this logic into PathAssignments::walk_path, and performing
        // it once for _every_ root→always path in the BDD. (That would require resetting the
        // PathAssignments state for each of those paths, which is why the logic would have to
        // move.)
        let mut node = when.node;
        if !node.is_single_conjunction(builder) {
            return;
        }

        loop {
            match node.node() {
                Node::AlwaysTrue | Node::AlwaysFalse => break,
                Node::Interior(interior) => {
                    let interior = builder.interior_node_data(interior.node());
                    if interior.if_true != ALWAYS_FALSE {
                        self.add_single_implication(db, builder, constraint, interior.constraint);
                        node = interior.if_true;
                    } else {
                        self.add_pair_impossibility(db, builder, constraint, interior.constraint);
                        node = interior.if_false;
                    }
                }
            }
        }
    }

    fn add_sequents_for_pair<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        // If either of the constraints has another typevar as a lower/upper bound, the only
        // sequents we can add are for the transitive closure. For instance, if we have
        // `(S ≤ T) ∧ (T ≤ int)`, then `(S ≤ int)` will also hold, and we should add a sequent for
        // this implication. These are the `mutual_sequents` mentioned below — sequents that come
        // about because two typevars are mutually constrained.
        //
        // Complicating things is that `(S ≤ T)` will be encoded differently depending on how `S`
        // and `T` compare in our arbitrary BDD variable ordering.
        //
        // When `S` comes before `T`, `(S ≤ T)` will be encoded as `(Never ≤ S ≤ T)`, and the
        // overall antecedent will be `(Never ≤ S ≤ T) ∧ (T ≤ int)`. Those two individual
        // constraints constrain different typevars (`S` and `T`, respectively), and are handled by
        // `add_mutual_sequents_for_different_typevars`.
        //
        // When `T` comes before `S`, `(S ≤ T)` will be encoded as `(S ≤ T ≤ object)`, and the
        // overall antecedent will be `(S ≤ T ≤ object) ∧ (T ≤ int)`. Those two individual
        // constraints both constrain `T`, and are handled by
        // `add_mutual_sequents_for_same_typevars`.
        //
        // If all of the lower and upper bounds are concrete (i.e., not typevars), then there
        // several _other_ sequents that we can add, as handled by `add_concrete_sequents`.
        let left_constraint_data = builder.constraint_data(left_constraint);
        let left_typevar = left_constraint_data.typevar;
        let right_constraint_data = builder.constraint_data(right_constraint);
        let right_typevar = right_constraint_data.typevar;
        if !left_typevar.is_same_typevar_as(db, right_typevar) {
            self.add_mutual_sequents_for_different_typevars(
                db,
                builder,
                left_constraint,
                right_constraint,
            );
            self.add_nested_typevar_sequents(db, builder, left_constraint, right_constraint);
        } else if left_constraint_data.lower.is_type_var()
            || left_constraint_data.upper.is_type_var()
            || right_constraint_data.lower.is_type_var()
            || right_constraint_data.upper.is_type_var()
        {
            self.add_mutual_sequents_for_same_typevars(
                db,
                builder,
                left_constraint,
                right_constraint,
            );
        } else {
            self.add_concrete_sequents(db, builder, left_constraint, right_constraint);
        }
    }

    fn add_mutual_sequents_for_different_typevars<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        // We've structured our constraints so that a typevar's upper/lower bound can only
        // be another typevar if the bound is "later" in our arbitrary ordering. That means
        // we only have to check this pair of constraints in one direction — though we do
        // have to figure out which of the two typevars is constrained, and which one is
        // the upper/lower bound.
        let left_constraint_data = builder.constraint_data(left_constraint);
        let left_typevar = left_constraint_data.typevar;
        let right_constraint_data = builder.constraint_data(right_constraint);
        let right_typevar = right_constraint_data.typevar;
        let (bound_constraint, constrained_constraint) =
            if left_typevar.can_be_bound_for(db, builder, right_typevar) {
                (left_constraint, right_constraint)
            } else {
                (right_constraint, left_constraint)
            };

        // We then look for cases where the "constrained" typevar's upper and/or lower bound
        // matches the "bound" typevar. If so, we're going to add an implication sequent that
        // replaces the upper/lower bound that matched with the bound constraint's corresponding
        // bound.
        let bound_constraint_data = builder.constraint_data(bound_constraint);
        let bound_typevar = bound_constraint_data.typevar;
        let constrained_constraint_data = builder.constraint_data(constrained_constraint);
        let constrained_typevar = constrained_constraint_data.typevar;
        let (new_lower, new_upper) = match (
            constrained_constraint_data.lower,
            constrained_constraint_data.upper,
        ) {
            // (B ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ BU)
            (Type::TypeVar(constrained_lower), Type::TypeVar(constrained_upper))
                if constrained_lower.is_same_typevar_as(db, bound_typevar)
                    && constrained_upper.is_same_typevar_as(db, bound_typevar) =>
            {
                (bound_constraint_data.lower, bound_constraint_data.upper)
            }

            // (CL ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (CL ≤ C ≤ BU)
            (constrained_lower, Type::TypeVar(constrained_upper))
                if constrained_upper.is_same_typevar_as(db, bound_typevar) =>
            {
                (constrained_lower, bound_constraint_data.upper)
            }

            // (B ≤ C ≤ CU) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ CU)
            (Type::TypeVar(constrained_lower), constrained_upper)
                if constrained_lower.is_same_typevar_as(db, bound_typevar) =>
            {
                (bound_constraint_data.lower, constrained_upper)
            }

            // (CL ≤ C ≤ pivot) ∧ (pivot ≤ B ≤ BU) → (CL ≤ C ≤ B)
            (constrained_lower, constrained_upper)
                if !constrained_upper.is_never()
                    && !constrained_upper.is_object()
                    && constrained_upper
                        .top_materialization(db)
                        .is_constraint_set_assignable_to(
                            db,
                            bound_constraint_data.lower.bottom_materialization(db),
                        ) =>
            {
                (constrained_lower, Type::TypeVar(bound_typevar))
            }

            // (pivot ≤ C ≤ CU) ∧ (BL ≤ B ≤ pivot) → (B ≤ C ≤ CU)
            (constrained_lower, constrained_upper)
                if !constrained_lower.is_never()
                    && !constrained_lower.is_object()
                    && bound_constraint_data
                        .upper
                        .top_materialization(db)
                        .is_constraint_set_assignable_to(
                            db,
                            constrained_lower.bottom_materialization(db),
                        ) =>
            {
                (Type::TypeVar(bound_typevar), constrained_upper)
            }

            _ => return,
        };

        let mut post_constraints: SmallVec<[ConstraintId; 3]> = SmallVec::new();
        let mut constrained_lower = new_lower;
        let mut constrained_upper = new_upper;

        // The transitive rule above gives us an intended post-condition
        // `new_lower ≤ [constrained] ≤ new_upper`.
        //
        // If a top-level bound typevar is "earlier" than `constrained`, we cannot represent that
        // directly as a bound on `constrained` without violating our canonical ordering.
        // Instead, split it into equivalent canonical constraints by "moving" that bound onto the
        // other typevar:
        //
        //   invalid lower  `L ≤ [C]`  ->  `(Never ≤ [L] ≤ C)` and drop `L` from C's lower bound
        //   invalid upper  `[C] ≤ U`  ->  `(C ≤ [U] ≤ object)` and drop `U` from C's upper bound
        //
        // Example: if we derive `[A] ≤ T ≤ [B]` but `A`/`B` are not valid top-level bounds for
        // `T` in this ordering, we emit two pair implications:
        //   `(Never ≤ [A] ≤ T)` and `(T ≤ [B] ≤ object)`.
        // This preserves the relationship while keeping all derived constraints canonical.
        if let Type::TypeVar(lower_bound_typevar) = new_lower
            && !lower_bound_typevar.can_be_bound_for(db, builder, constrained_typevar)
        {
            post_constraints.push(ConstraintId::new(
                db,
                builder,
                lower_bound_typevar,
                Type::Never,
                Type::TypeVar(constrained_typevar),
            ));
            constrained_lower = Type::Never;
        }

        if let Type::TypeVar(upper_bound_typevar) = new_upper
            && !upper_bound_typevar.can_be_bound_for(db, builder, constrained_typevar)
        {
            post_constraints.push(ConstraintId::new(
                db,
                builder,
                upper_bound_typevar,
                Type::TypeVar(constrained_typevar),
                Type::object(),
            ));
            constrained_upper = Type::object();
        }

        if !(constrained_lower.is_never() && constrained_upper.is_object()) {
            post_constraints.push(ConstraintId::new(
                db,
                builder,
                constrained_typevar,
                constrained_lower,
                constrained_upper,
            ));
        }

        for post_constraint in post_constraints {
            self.add_pair_implication(
                db,
                builder,
                left_constraint,
                right_constraint,
                post_constraint,
            );
        }
    }

    /// Adds sequents for the case where one constraint's lower or upper bound contains another
    /// constraint's typevar nested inside a parameterized type (e.g., `U ≤ Covariant[T]`).
    ///
    /// This is distinct from `add_mutual_sequents_for_different_typevars`, which handles the case
    /// where a typevar appears _directly_ as a top-level lower/upper bound (e.g., `U ≤ T`). A
    /// bare `Type::TypeVar` is technically a special case of covariant nesting (since the variance
    /// of `T` in `T` itself is covariant), but the existing direct-typevar logic handles it
    /// separately because it requires careful canonical ordering of typevar-to-typevar constraints
    /// that the generic nested-typevar logic here does not need to worry about.
    fn add_nested_typevar_sequents<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        let mut try_tightening =
            |bound_constraint: ConstraintId, constrained_constraint: ConstraintId| {
                let bound_data = builder.constraint_data(bound_constraint);
                let bound_typevar = bound_data.typevar;
                let constrained_data = builder.constraint_data(constrained_constraint);
                let constrained_typevar = constrained_data.typevar;

                // If the replacement contains the bound typevar itself (e.g., the bound
                // constraint is `_V ≤ G[_V]`), or the constrained typevar (e.g., the bound
                // constraint is `_T ≤ G[_V]` and we're about to substitute into `_V ≤ G[_T]`),
                // substituting would create a deeper nesting of the same recursive pattern
                // that triggers the same substitution again ad infinitum. Skip in both cases.
                //
                // Fast-path bare typevar replacements (`Type::TypeVar`) using equality checks
                // instead of calling `variance_of` on them. This avoids a large number of tiny
                // tracked `variance_of` queries in hot paths.
                let replacement_mentions_bound_or_constrained = |replacement: Type<'db>| {
                    replacement.variance_of(db, bound_typevar) != TypeVarVariance::Bivariant
                        || replacement.variance_of(db, constrained_typevar)
                            != TypeVarVariance::Bivariant
                };

                // Check the upper bound of the constrained constraint for nested occurrences of
                // the bound typevar. We use `variance_of` as our combined presence + variance
                // check: `Bivariant` means the typevar doesn't appear in the type (or is genuinely
                // bivariant, which is semantically equivalent — no implication is needed in either
                // case).
                //
                // Note: if `Bivariant` is ever removed from the `TypeVarVariance` enum, we would
                // need an alternative representation for "typevar not present"
                // (e.g., `Option<TypeVarVariance>`).
                let upper_replacement = match constrained_data.upper.variance_of(db, bound_typevar)
                {
                    TypeVarVariance::Bivariant => None,
                    // Skip bare typevars — those are handled by
                    // `add_mutual_sequents_for_different_typevars`.
                    _ if constrained_data.upper.is_type_var() => None,
                    // Covariance preserves direction: upper bound on T substitutes into upper
                    // bound. A ≤ B → G[A] ≤ G[B], so (T ≤ u_B) gives G[T] ≤ G[u_B].
                    TypeVarVariance::Covariant if !bound_data.upper.is_object() => {
                        Some(bound_data.upper)
                    }
                    // Contravariance flips direction: lower bound on T substitutes into upper
                    // bound. A ≤ B → G[B] ≤ G[A], so (l_B ≤ T) gives G[T] ≤ G[l_B].
                    TypeVarVariance::Contravariant if !bound_data.lower.is_never() => {
                        Some(bound_data.lower)
                    }
                    // Invariance requires equality: only substitute if l_B = u_B.
                    TypeVarVariance::Invariant
                        if bound_data.lower == bound_data.upper && !bound_data.lower.is_never() =>
                    {
                        Some(bound_data.lower)
                    }
                    _ => None,
                };
                let upper_replacement = upper_replacement.filter(|replacement| {
                    // Substituting one typevar for another into large unions can generate many
                    // very-weak derived constraints and cause severe performance regressions.
                    // Keep the common/non-union case enabled; skip union upper bounds for this
                    // specific typevar-to-typevar replacement shape.
                    if replacement.is_type_var() && constrained_data.upper.is_union() {
                        return false;
                    }
                    !replacement_mentions_bound_or_constrained(*replacement)
                });
                if let Some(replacement) = upper_replacement {
                    let new_upper = constrained_data.upper.substitute_one_typevar(
                        db,
                        bound_typevar,
                        replacement,
                    );
                    if new_upper != constrained_data.upper {
                        let post = ConstraintId::new(
                            db,
                            builder,
                            constrained_typevar,
                            constrained_data.lower,
                            new_upper,
                        );
                        self.add_pair_implication_with_provenance(
                            db,
                            builder,
                            bound_constraint,
                            constrained_constraint,
                            post,
                            Some(nested_substitution(
                                db,
                                builder,
                                constrained_typevar,
                                bound_typevar,
                                NestedSubstitutionSide::Upper,
                            )),
                        );
                    }
                }

                // Check the lower bound of the constrained constraint for nested occurrences.
                let lower_replacement = match constrained_data.lower.variance_of(db, bound_typevar)
                {
                    TypeVarVariance::Bivariant => None,
                    _ if constrained_data.lower.is_type_var() => None,
                    // Covariance preserves direction: lower bound on T substitutes into lower
                    // bound. A ≤ B → G[A] ≤ G[B], so (l_B ≤ T) gives G[l_B] ≤ G[T].
                    TypeVarVariance::Covariant if !bound_data.lower.is_never() => {
                        Some(bound_data.lower)
                    }
                    // Contravariance flips direction: upper bound on T substitutes into lower
                    // bound. A ≤ B → G[B] ≤ G[A], so (T ≤ u_B) gives G[u_B] ≤ G[T].
                    TypeVarVariance::Contravariant if !bound_data.upper.is_object() => {
                        Some(bound_data.upper)
                    }
                    // Invariance requires equality: only substitute if l_B = u_B.
                    TypeVarVariance::Invariant
                        if bound_data.lower == bound_data.upper && !bound_data.lower.is_never() =>
                    {
                        Some(bound_data.lower)
                    }
                    _ => None,
                };
                let lower_replacement = lower_replacement.filter(|replacement| {
                    // Substituting one typevar for another into large intersections can generate
                    // many very-weak derived constraints and cause severe performance regressions.
                    // Keep the common/non-intersection case enabled; skip intersection lower
                    // bounds for this specific typevar-to-typevar replacement shape.
                    if replacement.is_type_var() && constrained_data.lower.is_intersection() {
                        return false;
                    }
                    !replacement_mentions_bound_or_constrained(*replacement)
                });
                if let Some(replacement) = lower_replacement {
                    let new_lower = constrained_data.lower.substitute_one_typevar(
                        db,
                        bound_typevar,
                        replacement,
                    );
                    if new_lower != constrained_data.lower {
                        let post = ConstraintId::new(
                            db,
                            builder,
                            constrained_typevar,
                            new_lower,
                            constrained_data.upper,
                        );
                        self.add_pair_implication_with_provenance(
                            db,
                            builder,
                            bound_constraint,
                            constrained_constraint,
                            post,
                            Some(nested_substitution(
                                db,
                                builder,
                                constrained_typevar,
                                bound_typevar,
                                NestedSubstitutionSide::Lower,
                            )),
                        );
                    }
                }
            };

        try_tightening(left_constraint, right_constraint);
        try_tightening(right_constraint, left_constraint);

        // Additionally, check if one constraint's bare typevar *bound* appears nested in the other
        // constraint's bounds. This handles the "dual" direction: instead of substituting a
        // typevar's concrete bounds into another constraint (tightening), we substitute the
        // typevar itself for one of its bare typevar bounds (weakening), creating a cross-typevar
        // link.
        //
        // For example, given `(Covariant[S] ≤ C) ∧ (Never ≤ B ≤ S)`, S is B's upper bound and
        // appears covariantly in C's lower bound. Since `B ≤ S`, covariance tells us that
        // `Covariant[B] ≤ Covariant[S]`. Transitivity then lets us derive `Covariant[B] ≤ C`.
        //
        // The derived constraint is weaker than the original, but it introduces a relationship
        // between B and C that we need to remember and propagate if we ever existentially quantify
        // away S.
        //
        // TODO: This only handles the case where the bound (in this case, S) is a bare typevar. A
        // future extension could handle arbitrary types by pattern-matching on generic alias
        // structure.
        //
        // This is defined as a separate closure because it iterates over the bound constraint's
        // bare typevar bounds, which is a different axis than `try_tightening`'s check on the
        // bound constraint's typevar.
        let mut try_weakening =
            |bound_constraint: ConstraintId, constrained_constraint: ConstraintId| {
                let bound_data = builder.constraint_data(bound_constraint);
                let bound_typevar = bound_data.typevar;
                let constrained_data = builder.constraint_data(constrained_constraint);
                let constrained_typevar = constrained_data.typevar;

                let mut try_one_bound = |bound: Type<'db>, is_upper_bound: bool| {
                    let Some(nested_typevar) = bound.as_typevar() else {
                        return;
                    };

                    // Skip if the nested typevar is the same as the constrained typevar — that
                    // case is handled by `add_mutual_sequents_for_different_typevars`.
                    if nested_typevar.is_same_typevar_as(db, constrained_typevar)
                        || nested_typevar.is_same_typevar_as(db, bound_typevar)
                    {
                        return;
                    }

                    let replacement = Type::TypeVar(bound_typevar);

                    // Check the constrained constraint's upper bound for nested occurrences of
                    // nested_typevar (S). We want to *weaken* (relax) the upper bound by making it
                    // larger:
                    //   - Covariant + S is B's lower bound (S ≤ B): G[S] ≤ G[B] → weaker. Emit.
                    //   - Contravariant + S is B's upper bound (B ≤ S): G[S] ≤ G[B] → weaker. Emit.
                    //   - Other combinations tighten rather than weaken. Skip.
                    let should_weaken_upper = !constrained_data.upper.is_type_var()
                        && !constrained_data.upper.is_never()
                        && !constrained_data.upper.is_object()
                        && !constrained_data.upper.is_dynamic()
                        && match constrained_data.upper.variance_of(db, nested_typevar) {
                            TypeVarVariance::Bivariant => false,
                            TypeVarVariance::Covariant => !is_upper_bound,
                            TypeVarVariance::Contravariant => is_upper_bound,
                            TypeVarVariance::Invariant => {
                                bound_data.lower == bound_data.upper && !bound_data.lower.is_never()
                            }
                        };
                    if should_weaken_upper {
                        let new_upper = constrained_data.upper.substitute_one_typevar(
                            db,
                            nested_typevar,
                            replacement,
                        );
                        if new_upper != constrained_data.upper {
                            let post = ConstraintId::new(
                                db,
                                builder,
                                constrained_typevar,
                                constrained_data.lower,
                                new_upper,
                            );
                            self.add_pair_implication_with_provenance(
                                db,
                                builder,
                                bound_constraint,
                                constrained_constraint,
                                post,
                                Some(nested_substitution(
                                    db,
                                    builder,
                                    constrained_typevar,
                                    nested_typevar,
                                    NestedSubstitutionSide::Upper,
                                )),
                            );
                        }
                    }

                    // Ditto for the lower bound.
                    let should_weaken_lower = !constrained_data.lower.is_type_var()
                        && !constrained_data.lower.is_never()
                        && !constrained_data.lower.is_object()
                        && !constrained_data.lower.is_dynamic()
                        && match constrained_data.lower.variance_of(db, nested_typevar) {
                            TypeVarVariance::Bivariant => false,
                            TypeVarVariance::Covariant => is_upper_bound,
                            TypeVarVariance::Contravariant => !is_upper_bound,
                            TypeVarVariance::Invariant => {
                                bound_data.lower == bound_data.upper && !bound_data.lower.is_never()
                            }
                        };
                    if should_weaken_lower {
                        let new_lower = constrained_data.lower.substitute_one_typevar(
                            db,
                            nested_typevar,
                            replacement,
                        );
                        if new_lower != constrained_data.lower {
                            let post = ConstraintId::new(
                                db,
                                builder,
                                constrained_typevar,
                                new_lower,
                                constrained_data.upper,
                            );
                            self.add_pair_implication_with_provenance(
                                db,
                                builder,
                                bound_constraint,
                                constrained_constraint,
                                post,
                                Some(nested_substitution(
                                    db,
                                    builder,
                                    constrained_typevar,
                                    nested_typevar,
                                    NestedSubstitutionSide::Lower,
                                )),
                            );
                        }
                    }
                };

                // For each bare typevar bound S of the bound constraint, check if S appears
                // nested in the constrained constraint's bounds. If so, we can substitute B
                // (the bound constraint's typevar) for S, producing a weaker but useful
                // constraint.
                try_one_bound(bound_data.upper, true);
                try_one_bound(bound_data.lower, false);
            };

        try_weakening(left_constraint, right_constraint);
        try_weakening(right_constraint, left_constraint);
    }

    fn add_mutual_sequents_for_same_typevars<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        let mut try_one_direction =
            |left_constraint: ConstraintId, right_constraint: ConstraintId| {
                let left_constraint_data = builder.constraint_data(left_constraint);
                let left_lower = left_constraint_data.lower;
                let left_upper = left_constraint_data.upper;
                let right_constraint_data = builder.constraint_data(right_constraint);
                let right_lower = right_constraint_data.lower;
                let right_upper = right_constraint_data.upper;
                let new_constraints =
                    |bound_typevar: BoundTypeVarInstance<'db>,
                     right_lower: Type<'db>,
                     right_upper: Type<'db>| {
                        let right_lower = if let Type::TypeVar(other_bound_typevar) = right_lower
                            && bound_typevar.is_same_typevar_as(db, other_bound_typevar)
                        {
                            Type::Never
                        } else {
                            right_lower
                        };
                        let right_upper = if let Type::TypeVar(other_bound_typevar) = right_upper
                            && bound_typevar.is_same_typevar_as(db, other_bound_typevar)
                        {
                            Type::object()
                        } else {
                            right_upper
                        };

                        // Same idea as `add_mutual_sequents_for_different_typevars`: if a derived
                        // post-condition for `[bound]` has top-level typevar bounds in the wrong
                        // orientation, split it into equivalent canonical constraints instead of
                        // dropping it.
                        let mut post_constraints: SmallVec<[ConstraintId; 3]> = SmallVec::new();
                        let mut constrained_lower = right_lower;
                        let mut constrained_upper = right_upper;

                        if let Type::TypeVar(lower_bound_typevar) = right_lower
                            && !lower_bound_typevar.can_be_bound_for(db, builder, bound_typevar)
                        {
                            post_constraints.push(ConstraintId::new(
                                db,
                                builder,
                                lower_bound_typevar,
                                Type::Never,
                                Type::TypeVar(bound_typevar),
                            ));
                            constrained_lower = Type::Never;
                        }

                        if let Type::TypeVar(upper_bound_typevar) = right_upper
                            && !upper_bound_typevar.can_be_bound_for(db, builder, bound_typevar)
                        {
                            post_constraints.push(ConstraintId::new(
                                db,
                                builder,
                                upper_bound_typevar,
                                Type::TypeVar(bound_typevar),
                                Type::object(),
                            ));
                            constrained_upper = Type::object();
                        }

                        if !(constrained_lower.is_never() && constrained_upper.is_object()) {
                            post_constraints.push(ConstraintId::new(
                                db,
                                builder,
                                bound_typevar,
                                constrained_lower,
                                constrained_upper,
                            ));
                        }

                        post_constraints
                    };
                let post_constraints = match (left_lower, left_upper) {
                    (Type::TypeVar(bound_typevar), Type::TypeVar(other_bound_typevar))
                        if bound_typevar.is_same_typevar_as(db, other_bound_typevar) =>
                    {
                        new_constraints(bound_typevar, right_lower, right_upper)
                    }
                    (Type::TypeVar(bound_typevar), _) => {
                        new_constraints(bound_typevar, Type::Never, right_upper)
                    }
                    (_, Type::TypeVar(bound_typevar)) => {
                        new_constraints(bound_typevar, right_lower, Type::object())
                    }
                    _ => return,
                };
                for post_constraint in post_constraints {
                    self.add_pair_implication(
                        db,
                        builder,
                        left_constraint,
                        right_constraint,
                        post_constraint,
                    );
                }
            };

        try_one_direction(left_constraint, right_constraint);
        try_one_direction(right_constraint, left_constraint);
    }

    fn add_concrete_sequents<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        // These might seem redundant with the intersection check below, since `a → b` means that
        // `a ∧ b = a`. But we are not normalizing constraint bounds, and these clauses help us
        // identify constraints that are identical besides e.g. ordering of union/intersection
        // elements. (For instance, when processing `T ≤ τ₁ & τ₂` and `T ≤ τ₂ & τ₁`, these clauses
        // would add sequents for `(T ≤ τ₁ & τ₂) → (T ≤ τ₂ & τ₁)` and vice versa.)
        if left_constraint.implies(db, builder, right_constraint) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left_constraint.display(db, builder),
                right = %right_constraint.display(db, builder),
                "left implies right",
            );
            self.add_single_implication(db, builder, left_constraint, right_constraint);
        }
        if right_constraint.implies(db, builder, left_constraint) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left_constraint.display(db, builder),
                right = %right_constraint.display(db, builder),
                "right implies left",
            );
            self.add_single_implication(db, builder, right_constraint, left_constraint);
        }

        match left_constraint.intersect(db, builder, right_constraint) {
            IntersectionResult::Simplified(intersection_constraint_data) => {
                let intersection_constraint =
                    builder.intern_constraint(db, intersection_constraint_data);
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::SequentMap",
                    left = %left_constraint.display(db, builder),
                    right = %right_constraint.display(db, builder),
                    intersection = %intersection_constraint.display(db, builder),
                    "left and right overlap",
                );
                self.add_pair_implication(
                    db,
                    builder,
                    left_constraint,
                    right_constraint,
                    intersection_constraint,
                );
                self.add_single_implication(db, builder, intersection_constraint, left_constraint);
                self.add_single_implication(db, builder, intersection_constraint, right_constraint);
            }

            // The sequent map only needs to include constraints that might appear in a BDD. If the
            // intersection does not collapse to a single constraint, then there's no new
            // constraint that we need to add to the sequent map.
            IntersectionResult::CannotSimplify => {}

            IntersectionResult::Disjoint => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::SequentMap",
                    left = %left_constraint.display(db, builder),
                    right = %right_constraint.display(db, builder),
                    "left and right are disjoint",
                );
                self.add_pair_impossibility(db, builder, left_constraint, right_constraint);
            }
        }
    }

    #[expect(dead_code)] // Keep this around for debugging purposes
    fn display<'db, 'a>(
        &'a self,
        db: &'db dyn Db,
        builder: &'a ConstraintSetBuilder<'db>,
        prefix: &'a dyn Display,
    ) -> impl Display + 'a {
        struct DisplaySequentMap<'a, 'db> {
            map: &'a SequentMap,
            prefix: &'a dyn Display,
            db: &'db dyn Db,
            builder: &'a ConstraintSetBuilder<'db>,
        }

        impl Display for DisplaySequentMap<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut first = true;
                let mut maybe_write_prefix = |f: &mut std::fmt::Formatter<'_>| {
                    if first {
                        first = false;
                        Ok(())
                    } else {
                        write!(f, "\n{}", self.prefix)
                    }
                };

                for (ante1, ante2) in &self.map.pair_impossibilities {
                    maybe_write_prefix(f)?;
                    write!(
                        f,
                        "{} ∧ {} → false",
                        ante1.display(self.db, self.builder),
                        ante2.display(self.db, self.builder),
                    )?;
                }

                for ((ante1, ante2), posts) in &self.map.pair_implications {
                    for post in posts {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} ∧ {} → {}",
                            ante1.display(self.db, self.builder),
                            ante2.display(self.db, self.builder),
                            post.constraint.display(self.db, self.builder),
                        )?;
                    }
                }

                for (ante, posts) in &self.map.single_implications {
                    for post in posts {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} → {}",
                            ante.display(self.db, self.builder),
                            post.display(self.db, self.builder)
                        )?;
                    }
                }

                if first {
                    f.write_str("[no sequents]")?;
                }
                Ok(())
            }
        }

        DisplaySequentMap {
            map: self,
            prefix,
            db,
            builder,
        }
    }
}

/// The collection of constraints that we know to be true or false at a certain point when
/// traversing a BDD.
#[derive(Debug)]
pub(crate) struct PathAssignments {
    map: SequentMap,
    assignments: FxIndexMap<ConstraintAssignment, usize>,
    /// Nested substitutions that we have already applied on the current root→terminal path.
    nested_substitutions: FxIndexSet<NestedSubstitution>,
    /// Constraints that we have discovered, mapped to whether we have processed them yet. (This
    /// ensures a stable order for all of the derived constraints that we create, while still
    /// letting us create them lazily.)
    discovered: FxIndexMap<ConstraintId, bool>,
}

impl PathAssignments {
    fn new(constraints: impl IntoIterator<Item = ConstraintId>) -> Self {
        let discovered = constraints
            .into_iter()
            .map(|constraint| (constraint, false))
            .collect();
        Self {
            map: SequentMap::default(),
            assignments: FxIndexMap::default(),
            nested_substitutions: FxIndexSet::default(),
            discovered,
        }
    }

    /// Walks one of the outgoing edges of an internal BDD node. `assignment` describes the
    /// constraint that the BDD node checks, and whether we are following the `if_true` or
    /// `if_false` edge.
    ///
    /// This new assignment might cause this path to become impossible — for instance, if we were
    /// already assuming (from an earlier edge in the path) a constraint that is disjoint with this
    /// one. We might also be able to infer _other_ assignments that do not appear in the BDD
    /// directly, but which are implied from a combination of constraints that we _have_ seen.
    ///
    /// To handle all of this, you provide a callback. If the path has become impossible, we will
    /// return `None` _without invoking the callback_. If the path does not contain any
    /// contradictions, we will invoke the callback and return its result (wrapped in `Some`).
    ///
    /// Your callback will also be provided a slice of all of the constraints that we were able to
    /// infer from `assignment` combined with the information we already knew. (For borrow-check
    /// reasons, we provide this as a [`Range`]; use that range to index into `self.assignments` to
    /// get the list of all of the assignments that we learned from this edge.)
    ///
    /// You will presumably end up making a recursive call of some kind to keep progressing through
    /// the BDD. You should make this call from inside of your callback, so that as you get further
    /// down into the BDD structure, we remember all of the information that we have learned from
    /// the path we're on.
    fn walk_edge<'db, R>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        assignment: ConstraintAssignment,
        source_order: usize,
        f: impl FnOnce(&mut Self, Range<usize>) -> R,
    ) -> Option<R> {
        // Record a snapshot of the assignments that we already knew held — both so that we can
        // pass along the range of which assignments are new, and so that we can reset back to this
        // point before returning.
        let start = self.assignments.len();
        let nested_substitutions_start = self.nested_substitutions.len();

        // Add the new assignment and anything we can derive from it.
        tracing::trace!(
            target: "ty_python_semantic::types::constraints::PathAssignment",
            before = %format_args!(
                "[{}]",
                self.assignments[..start].iter().map(|(assignment, _)| {
                    assignment.display(db, builder)
                }).format(", "),
            ),
            edge = %assignment.display(db, builder),
            "walk edge",
        );
        let found_conflict = self.add_assignment(db, builder, assignment, source_order, false);
        let result = if found_conflict.is_err() {
            // If that results in the path now being impossible due to a contradiction, return
            // without invoking the callback.
            None
        } else {
            // Otherwise invoke the callback to keep traversing the BDD. The callback will likely
            // traverse additional edges, which might add more to our `assignments` set. But even
            // if that happens, `start..end` will mark the assignments that were added by the
            // `add_assignment` call above — that is, the new assignment for this edge along with
            // the derived information we inferred from it.
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::PathAssignment",
                new = %format_args!(
                    "[{}]",
                    self.assignments[start..].iter().map(|(assignment, _)| {
                        assignment.display(db, builder)
                    }).format(", "),
                ),
                "new assignments",
            );
            let end = self.assignments.len();
            Some(f(self, start..end))
        };

        // Reset back to where we were before following this edge, so that the caller can reuse a
        // single instance for the entire BDD traversal.
        self.assignments.truncate(start);
        self.nested_substitutions
            .truncate(nested_substitutions_start);
        result
    }

    pub(crate) fn positive_constraints(&self) -> impl Iterator<Item = (ConstraintId, usize)> + '_ {
        self.assignments
            .iter()
            .filter_map(|(assignment, source_order)| match assignment {
                ConstraintAssignment::Positive(constraint) => Some((*constraint, *source_order)),
                ConstraintAssignment::Negative(_) | ConstraintAssignment::Unconstrained(_) => None,
            })
    }

    fn assignment_holds(&self, assignment: ConstraintAssignment) -> bool {
        self.assignments.contains_key(&assignment)
    }

    fn contains_constraint(&self, constraint: ConstraintId) -> bool {
        self.assignment_holds(constraint.when_true())
            || self.assignment_holds(constraint.when_false())
            || self.assignment_holds(constraint.when_unconstrained())
    }

    /// Update our sequent map to ensure that it holds all of the sequents that involve the given
    /// constraint. We do not calculate the new sequents directly. Instead, we call
    /// [`SequentMap::for_constraint`] and [`for_constraint_pair`][SequentMap::for_constraint_pair]
    /// to calculate _and cache_ the constraints, so that if we walk another constraint set
    /// containing this constraint, we reuse the work to calculate its sequents.
    fn discover_constraint<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        constraint: ConstraintId,
    ) {
        // If we've already processed this constraint, we can skip it.
        let existing = self.discovered.insert(constraint, true);
        let already_processed = existing.is_some_and(|existing| existing);
        if already_processed {
            return;
        }

        let single_map = SequentMap::for_constraint(db, builder, constraint);
        self.map.merge(&single_map);
        drop(single_map);

        for existing in self.discovered.keys().dropping_back(1) {
            let pair_map = SequentMap::for_constraint_pair(db, builder, *existing, constraint);
            self.map.merge(&pair_map);
        }
    }

    /// Adds a new assignment, along with any derived information that we can infer from the new
    /// assignment combined with the assignments we've already seen. If any of this causes the path
    /// to become invalid, due to a contradiction, returns a [`PathAssignmentConflict`] error.
    fn add_assignment<'db>(
        &mut self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        assignment: ConstraintAssignment,
        source_order: usize,
        derived: bool,
    ) -> Result<(), PathAssignmentConflict> {
        if matches!(assignment, ConstraintAssignment::Unconstrained(_)) {
            // An `Unconstrained` assignment means "this constraint can go either way". If there is
            // already any assignment for this constraint (positive, negative, or unconstrained),
            // the existing assignment is at least as informative, and we skip.
            if self.contains_constraint(assignment.constraint()) {
                return Ok(());
            }

            // Since we don't know whether the assignment's constraint holds or not, we cannot
            // derive any additional information from the sequent map. We still want to record the
            // assignment, but as an optimization we can return early without actually querying the
            // sequent map.
            self.assignments.insert(assignment, source_order);
            return Ok(());
        }

        // First add this assignment. If it causes a conflict, return that as an error. If we've
        // already know this assignment holds, just return.
        if self.assignments.contains_key(&assignment.negated()) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::PathAssignment",
                assignment = %assignment.display(db, builder),
                facts = %format_args!(
                    "[{}]",
                    self.assignments.iter().map(|(assignment, _)| {
                        assignment.display(db, builder)
                    }).format(", "),
                ),
                "found contradiction",
            );
            return Err(PathAssignmentConflict);
        }

        match self.assignments.entry(assignment) {
            Entry::Vacant(entry) => entry.insert(source_order),
            Entry::Occupied(mut entry) => {
                // If a constraint appears both as an "origin" constraint (it actually appears in
                // the BDD structure) and as a "derived" constraint (we infer it from other
                // constraints), we should prefer the origin source_order, regardless of which
                // order we encounter the various constraints in the BDD.
                if !derived {
                    *entry.get_mut() = source_order;
                }
                return Ok(());
            }
        };

        // Then use our sequents to add additional facts that we know to be true. We currently
        // reuse the `source_order` of the "real" constraint passed into `walk_edge` when we add
        // these derived facts.
        //
        // TODO: This might not be stable enough, if we add more than one derived fact for this
        // constraint. If we still see inconsistent test output, we might need a more complex
        // way of tracking source order for derived facts.
        //
        // TODO: This is very naive at the moment, partly for expediency, and partly because we
        // don't anticipate the sequent maps to be very large. We might consider avoiding the
        // brute-force search.

        self.discover_constraint(db, builder, assignment.constraint());

        for ante in &self.map.single_tautologies {
            if self.assignment_holds(ante.when_false()) {
                // The sequent map says (ante1) is always true, and the current path asserts that
                // it's false.
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    ante = %ante.display(db, builder),
                    facts = %format_args!(
                        "[{}]",
                        self.assignments.iter().map(|(assignment, _)| {
                            assignment.display(db, builder)
                        }).format(", "),
                    ),
                    "found contradiction",
                );
                return Err(PathAssignmentConflict);
            }
        }

        for (ante1, ante2) in &self.map.pair_impossibilities {
            if self.assignment_holds(ante1.when_true()) && self.assignment_holds(ante2.when_true())
            {
                // The sequent map says (ante1 ∧ ante2) is an impossible combination, and the
                // current path asserts that both are true.
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    ante1 = %ante1.display(db, builder),
                    ante2 = %ante2.display(db, builder),
                    facts = %format_args!(
                        "[{}]",
                        self.assignments.iter().map(|(assignment, _)| {
                            assignment.display(db, builder)
                        }).format(", "),
                    ),
                    "found contradiction",
                );
                return Err(PathAssignmentConflict);
            }
        }

        let mut new_constraints = Vec::new();
        for ((ante1, ante2), posts) in &self.map.pair_implications {
            if !self.assignment_holds(ante1.when_true())
                || !self.assignment_holds(ante2.when_true())
            {
                continue;
            }

            for post in posts {
                // Nested-typevar sequents are the mechanism that preserves cross-typevar facts when
                // we later existentially quantify away one of the typevars. However, once we've
                // applied a particular substitution site on the current path, reapplying it with a
                // newly derived replacement type does not add fundamentally new information — it
                // just keeps unfolding the same pattern one layer deeper. Skipping repeated
                // applications here prevents those infinite-looking expansion chains while still
                // keeping the first derived relationship.
                if let Some(nested_substitution) = post.nested_substitution
                    && !self.nested_substitutions.insert(nested_substitution)
                {
                    continue;
                }

                new_constraints.push(post.constraint);
            }
        }

        for (ante, posts) in &self.map.single_implications {
            if self.assignment_holds(ante.when_true()) {
                new_constraints.extend(posts.iter().copied());
            }
        }

        for new_constraint in new_constraints {
            self.add_assignment(db, builder, new_constraint.when_true(), source_order, true)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
struct PathAssignmentConflict;

/// A single clause in the DNF representation of a BDD
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SatisfiedClause {
    constraints: Vec<ConstraintAssignment>,
}

impl SatisfiedClause {
    fn push(&mut self, constraint: ConstraintAssignment) {
        self.constraints.push(constraint);
    }

    fn pop(&mut self) {
        self.constraints
            .pop()
            .expect("clause vector should not be empty");
    }

    /// Invokes a closure with the last constraint in this clause negated. Returns the clause back
    /// to its original state after invoking the closure.
    fn with_negated_last_constraint(&mut self, f: impl for<'a> FnOnce(&'a Self)) {
        if self.constraints.is_empty() {
            return;
        }
        let last_index = self.constraints.len() - 1;
        self.constraints[last_index].negate();
        f(self);
        self.constraints[last_index].negate();
    }

    /// Removes another clause from this clause, if it appears as a prefix of this clause. Returns
    /// whether the prefix was removed.
    fn remove_prefix(&mut self, prefix: &SatisfiedClause) -> bool {
        if self.constraints.starts_with(&prefix.constraints) {
            self.constraints.drain(0..prefix.constraints.len());
            return true;
        }
        false
    }

    /// Simplifies this clause by removing constraints that are implied by other constraints in the
    /// clause. (Clauses are the intersection of constraints, so if two clauses are redundant, we
    /// want to remove the larger one and keep the smaller one.)
    ///
    /// Returns a boolean that indicates whether any simplifications were made.
    fn simplify<'db>(&mut self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> bool {
        let mut changes_made = false;
        let mut i = 0;
        // Loop through each constraint, comparing it with any constraints that appear later in the
        // list.
        'outer: while i < self.constraints.len() {
            let mut j = i + 1;
            while j < self.constraints.len() {
                if self.constraints[j].implies(db, builder, self.constraints[i]) {
                    // If constraint `i` is removed, then we don't need to compare it with any
                    // later constraints in the list. Note that we continue the outer loop, instead
                    // of breaking from the inner loop, so that we don't bump index `i` below.
                    // (We'll have swapped another element into place at that index, and want to
                    // make sure that we process it.)
                    self.constraints.swap_remove(i);
                    changes_made = true;
                    continue 'outer;
                } else if self.constraints[i].implies(db, builder, self.constraints[j]) {
                    // If constraint `j` is removed, then we can continue the inner loop. We will
                    // swap a new element into place at index `j`, and will continue comparing the
                    // constraint at index `i` with later constraints.
                    self.constraints.swap_remove(j);
                    changes_made = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
        changes_made
    }

    fn display<'db>(&self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> String {
        if self.constraints.is_empty() {
            return String::from("always");
        }

        // This is a bit heavy-handed, but we need to output the constraints in a consistent order
        // even though Salsa IDs are assigned non-deterministically. This Display output is only
        // used in test cases, so we don't need to over-optimize it.
        let mut constraints: Vec<_> = self
            .constraints
            .iter()
            .map(|constraint| constraint.display(db, builder).to_string())
            .collect();
        constraints.sort();

        let mut result = String::new();
        if constraints.len() > 1 {
            result.push('(');
        }
        for (i, constraint) in constraints.iter().enumerate() {
            if i > 0 {
                result.push_str(" ∧ ");
            }
            result.push_str(constraint);
        }
        if constraints.len() > 1 {
            result.push(')');
        }
        result
    }
}

/// A list of the clauses that satisfy a BDD. This is a DNF representation of the boolean function
/// that the BDD represents.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SatisfiedClauses {
    clauses: Vec<SatisfiedClause>,
}

impl SatisfiedClauses {
    fn push(&mut self, clause: SatisfiedClause) {
        self.clauses.push(clause);
    }

    /// Simplifies the DNF representation, removing redundancies that do not change the underlying
    /// function. (This is used when displaying a BDD, to make sure that the representation that we
    /// show is as simple as possible while still producing the same results.)
    fn simplify<'db>(&mut self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) {
        // First simplify each clause individually, by removing constraints that are implied by
        // other constraints in the clause.
        for clause in &mut self.clauses {
            clause.simplify(db, builder);
        }

        while self.simplify_one_round() {
            // Keep going
        }

        // We can remove any clauses that have been simplified to the point where they are empty.
        // (Clauses are intersections, so an empty clause is `false`, which does not contribute
        // anything to the outer union.)
        self.clauses.retain(|clause| !clause.constraints.is_empty());
    }

    fn simplify_one_round(&mut self) -> bool {
        let mut changes_made = false;

        // First remove any duplicate clauses. (The clause list will start out with no duplicates
        // in the first round of simplification, because of the guarantees provided by the BDD
        // structure. But earlier rounds of simplification might have made some clauses redundant.)
        // Note that we have to loop through the vector element indexes manually, since we might
        // remove elements in each iteration.
        let mut i = 0;
        while i < self.clauses.len() {
            let mut j = i + 1;
            while j < self.clauses.len() {
                if self.clauses[i] == self.clauses[j] {
                    self.clauses.swap_remove(j);
                    changes_made = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
        if changes_made {
            return true;
        }

        // Then look for "prefix simplifications". That is, looks for patterns
        //
        //   (A ∧ B) ∨ (A ∧ ¬B ∧ ...)
        //
        // and replaces them with
        //
        //   (A ∧ B) ∨ (...)
        for i in 0..self.clauses.len() {
            let (clause, rest) = self.clauses[..=i]
                .split_last_mut()
                .expect("index should be in range");
            clause.with_negated_last_constraint(|clause| {
                for existing in rest {
                    changes_made |= existing.remove_prefix(clause);
                }
            });

            let (clause, rest) = self.clauses[i..]
                .split_first_mut()
                .expect("index should be in range");
            clause.with_negated_last_constraint(|clause| {
                for existing in rest {
                    changes_made |= existing.remove_prefix(clause);
                }
            });

            if changes_made {
                return true;
            }
        }

        false
    }

    fn display<'db>(&self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> String {
        // This is a bit heavy-handed, but we need to output the clauses in a consistent order
        // even though Salsa IDs are assigned non-deterministically. This Display output is only
        // used in test cases, so we don't need to over-optimize it.

        if self.clauses.is_empty() {
            return String::from("never");
        }
        let mut clauses: Vec<_> = self
            .clauses
            .iter()
            .map(|clause| clause.display(db, builder))
            .collect();
        clauses.sort();
        clauses.join(" ∨ ")
    }
}

impl<'db> BoundTypeVarInstance<'db> {
    /// Returns the valid specializations of a typevar. This is used when checking a constraint set
    /// when this typevar is in inferable position, where we only need _some_ specialization to
    /// satisfy the constraint set.
    fn valid_specializations(self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> NodeId {
        if self.paramspec_attr(db).is_some() {
            // P.args and P.kwargs are variadic, and do not have an upper bound or constraints.
            return ALWAYS_TRUE;
        }

        // For gradual upper bounds and constraints, we are free to choose any materialization that
        // makes the check succeed. In inferable positions, it is most helpful to choose a
        // materialization that is as permissive as possible, since that maximizes the number of
        // valid specializations that might satisfy the check. We therefore take the top
        // materialization of the bound or constraints.
        //
        // Moreover, for a gradual constraint, we don't need to worry that typevar constraints are
        // _equality_ comparisons, not _subtyping_ comparisons — since we are only going to check
        // that _some_ valid specialization satisfies the constraint set, it's correct for us to
        // return the range of valid materializations that we can choose from.
        match self.typevar(db).bound_or_constraints(db) {
            None => ALWAYS_TRUE,
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                let bound = bound.top_materialization(db);
                Constraint::new_node(db, builder, self, Type::Never, bound)
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                let mut specializations = ALWAYS_FALSE;
                for constraint in constraints.elements(db) {
                    let constraint_lower = constraint.bottom_materialization(db);
                    let constraint_upper = constraint.top_materialization(db);
                    specializations = specializations.or_with_offset(
                        builder,
                        Constraint::new_node(db, builder, self, constraint_lower, constraint_upper),
                    );
                }
                specializations
            }
        }
    }

    /// Returns the required specializations of a typevar. This is used when checking a constraint
    /// set when this typevar is in non-inferable position, where we need _all_ specializations to
    /// satisfy the constraint set.
    ///
    /// That causes complications if this is a constrained typevar, where one of the constraints is
    /// gradual. In that case, we need to return the range of valid materializations, but we don't
    /// want to require that all of those materializations satisfy the constraint set.
    ///
    /// To handle this, we return a "primary" result, and an iterator of any gradual constraints.
    /// For an unbounded/unconstrained typevar or a bounded typevar, the primary result fully
    /// specifies the required specializations, and the iterator will be empty. For a constrained
    /// typevar, the primary result will include the fully static constraints, and the iterator
    /// will include an entry for each non-fully-static constraint.
    fn required_specializations(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
    ) -> (NodeId, Vec<NodeId>) {
        // For upper bounds and constraints, we are free to choose any materialization that makes
        // the check succeed. In non-inferable positions, it is most helpful to choose a
        // materialization that is as restrictive as possible, since that minimizes the number of
        // valid specializations that must satisfy the check. We therefore take the bottom
        // materialization of the bound or constraints.
        match self.typevar(db).bound_or_constraints(db) {
            None => (ALWAYS_TRUE, Vec::new()),
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                let bound = bound.bottom_materialization(db);
                (
                    Constraint::new_node(db, builder, self, Type::Never, bound),
                    Vec::new(),
                )
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                let mut non_gradual_constraints = ALWAYS_FALSE;
                let mut gradual_constraints = Vec::new();
                for constraint in constraints.elements(db) {
                    let constraint_lower = constraint.bottom_materialization(db);
                    let constraint_upper = constraint.top_materialization(db);
                    let constraint =
                        Constraint::new_node(db, builder, self, constraint_lower, constraint_upper);
                    if constraint_lower == constraint_upper {
                        non_gradual_constraints =
                            non_gradual_constraints.or_with_offset(builder, constraint);
                    } else {
                        gradual_constraints.push(constraint);
                    }
                }
                (non_gradual_constraints, gradual_constraints)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;
    use pretty_assertions::assert_eq;

    use crate::db::tests::setup_db;
    use crate::types::{BoundTypeVarInstance, KnownClass, TypeVarVariance};
    use ruff_python_ast::name::Name;

    fn create_typevar<'db>(db: &'db dyn Db, name: &'static str) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::synthetic(db, Name::new_static(name), TypeVarVariance::Invariant)
    }

    fn create_constraint<'db, 'c>(
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
        bound: KnownClass,
    ) -> ConstraintSet<'db, 'c> {
        let ty = bound.to_instance(db);
        ConstraintSet::constrain_typevar(db, builder, bound_typevar, ty, ty)
    }

    #[track_caller]
    fn check_display_graph<'db, 'c>(
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        set: ConstraintSet<'db, 'c>,
        expected: &str,
    ) {
        let expected = expected.trim_end();
        let actual = set.node.display_graph(db, builder, &"").to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_display_graph_output() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");
        let constraints = ConstraintSetBuilder::new();
        let t_str = create_constraint(&db, &constraints, t, KnownClass::Str);
        let t_bool = create_constraint(&db, &constraints, t, KnownClass::Bool);
        let u_str = create_constraint(&db, &constraints, u, KnownClass::Str);
        let u_bool = create_constraint(&db, &constraints, u, KnownClass::Bool);
        // Construct this in a different order than above to make the source_orders more
        // interesting.
        let set = (u_str.or(&db, &constraints, || u_bool))
            .and(&db, &constraints, || t_str.or(&db, &constraints, || t_bool));
        check_display_graph(
            &db,
            &constraints,
            set,
            indoc! {r#"
                <0> (U = bool) 2/4
                ┡━₁ <1> (T = bool) 4/4
                │   ┡━₁ always
                │   ├─? <2> (T = str) 3/3
                │   │   ┡━₁ always
                │   │   ├─? never
                │   │   └─₀ never
                │   └─₀ never
                ├─? <3> (U = str) 1/4
                │   ┡━₁ <1> SHARED
                │   ├─? never
                │   └─₀ never
                └─₀ never
            "#},
        );
    }

    // TODO: Many of the tests below should hold for _all_ constraint sets. They should really be
    // promoted to full-fledged property tests.

    #[test]
    fn tdd_bare_constraints_have_no_uncertain_branches() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(&db, &builder, t, KnownClass::Int);
        check_display_graph(
            &db,
            &builder,
            t_int,
            indoc! {r#"
                <0> (T = int) 1/1
                ┡━₁ always
                ├─? never
                └─₀ never
            "#},
        );
    }

    /// The Duboc union algorithm parks the second operand in the uncertain branch when the two
    /// TDDs have different root constraints, instead of duplicating it into both branches.
    #[test]
    fn tdd_union_creates_uncertain_branches() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");
        let builder = ConstraintSetBuilder::new();

        // Neither lhs nor rhs have uncertain branches (checked above). The operand with the
        // "lower" BDD variable (in this case, the lhs) is parked into a new uncertain branch in
        // the union result.
        let t_int = create_constraint(&db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(&db, &builder, u, KnownClass::Str);
        let union = t_int.or(&db, &builder, || u_str);
        check_display_graph(
            &db,
            &builder,
            union,
            indoc! {r#"
                <0> (U = str) 2/2
                ┡━₁ always
                ├─? <1> (T = int) 1/1
                │   ┡━₁ always
                │   ├─? never
                │   └─₀ never
                └─₀ never
            "#},
        );
    }

    /// The Duboc intersection algorithm preserves uncertain branches: when both operands have
    /// uncertain branches, the result's uncertain branch is `U1 ∧ U2`.
    #[test]
    fn tdd_intersection_preserves_uncertain() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(&db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(&db, &builder, u, KnownClass::Str);
        let t_bool = create_constraint(&db, &builder, t, KnownClass::Bool);
        let u_int = create_constraint(&db, &builder, u, KnownClass::Int);

        // lhs and rhs both have uncertain branches (checked above). These uncertain branches are
        // carried through to the intersection result.
        let lhs = t_int.or(&db, &builder, || u_str);
        let rhs = t_bool.or(&db, &builder, || u_int);
        let intersection = lhs.and(&db, &builder, || rhs);
        check_display_graph(
            &db,
            &builder,
            intersection,
            indoc! {r#"
                <0> (U = int) 4/4
                ┡━₁ <1> (U = str) 2/2
                │   ┡━₁ always
                │   ├─? <2> (T = int) 1/1
                │   │   ┡━₁ always
                │   │   ├─? never
                │   │   └─₀ never
                │   └─₀ never
                ├─? <3> (T = bool) 3/3
                │   ┡━₁ <1> SHARED
                │   ├─? never
                │   └─₀ never
                └─₀ never
            "#},
        );
    }

    /// Negation always produces flat TDDs (all uncertain branches are `ALWAYS_FALSE`).
    #[test]
    fn tdd_negation_produces_flat_tdd() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(&db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(&db, &builder, u, KnownClass::Str);
        let union = t_int.or(&db, &builder, || u_str);
        let negated = union.negate(&db, &builder);
        check_display_graph(
            &db,
            &builder,
            negated,
            indoc! {r#"
                <0> (U = str) 2/2
                ┡━₁ never
                ├─? never
                └─₀ <1> (T = int) 1/1
                    ┡━₁ never
                    ├─? never
                    └─₀ always
            "#},
        );
    }

    #[test]
    fn tdd_negation_correctness() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");
        let builder = ConstraintSetBuilder::new();

        let t_int = create_constraint(&db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(&db, &builder, u, KnownClass::Str);
        let tdd = t_int.or(&db, &builder, || u_str);
        let negated = tdd.negate(&db, &builder);

        // T ∧ ¬T == false
        assert!(tdd.and(&db, &builder, || negated).is_never_satisfied(&db));

        // T ∨ ¬T == true
        assert!(tdd.or(&db, &builder, || negated).is_always_satisfied(&db));
    }

    /// Double negation of a TDD with uncertain branches is semantically equivalent to the
    /// original (though the structure may differ since negation produces flat TDDs).
    #[test]
    fn tdd_double_negation() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(&db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(&db, &builder, u, KnownClass::Str);
        let tdd = t_int.or(&db, &builder, || u_str);
        let negated = tdd.negate(&db, &builder);
        let double_negated = negated.negate(&db, &builder);
        let equivalent = tdd.iff(&db, &builder, double_negated);
        assert!(equivalent.is_always_satisfied(&db));
    }

    /// `iff(T, T)` is always satisfied for TDDs with uncertain branches.
    #[test]
    fn tdd_iff_self() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(&db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(&db, &builder, u, KnownClass::Str);
        let tdd = t_int.or(&db, &builder, || u_str);

        // iff(T, T) == true
        assert!(tdd.iff(&db, &builder, tdd).is_always_satisfied(&db));

        // iff(T, ¬T) == false
        let negated = tdd.negate(&db, &builder);
        assert!(tdd.iff(&db, &builder, negated).is_never_satisfied(&db));
    }

    /// Round-trip through `OwnedConstraintSet`: build a TDD with uncertain branches, convert to
    /// owned, load into a new builder, and verify that we preserve the uncertain branch.
    #[test]
    fn tdd_owned_round_trip() {
        let db = setup_db();
        let t = create_typevar(&db, "T");
        let u = create_typevar(&db, "U");

        // Build a TDD with uncertain branches and convert to owned
        let builder = ConstraintSetBuilder::new();
        let owned = builder.into_owned(|builder| {
            let t_int = create_constraint(&db, builder, t, KnownClass::Int);
            let u_str = create_constraint(&db, builder, u, KnownClass::Str);
            let result = t_int.or(&db, builder, || u_str);
            check_display_graph(
                &db,
                builder,
                result,
                indoc! {r#"
                    <0> (U = str) 2/2
                    ┡━₁ always
                    ├─? <1> (T = int) 1/1
                    │   ┡━₁ always
                    │   ├─? never
                    │   └─₀ never
                    └─₀ never
                "#},
            );
            result
        });

        // Load into a new builder
        let builder = ConstraintSetBuilder::new();
        let loaded = builder.load(&db, &owned);
        check_display_graph(
            &db,
            &builder,
            loaded,
            indoc! {r#"
                <0> (U = str) 2/2
                ┡━₁ always
                ├─? <1> (T = int) 1/1
                │   ┡━₁ always
                │   ├─? never
                │   └─₀ never
                └─₀ never
            "#},
        );
    }
}
