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
//! The typevar `T` has an upper bound of `B`, which would translate into the constraint `T ≤ B`.
//! (A missing lower bound is logically materialized as `Never`, since every type is a supertype of
//! `Never`. Similarly, a missing upper bound is logically materialized as `object`.) The `T ≤ B`
//! part expresses that the type can specialize to any type that is a subtype of B.
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

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::convert::Infallible;
use std::fmt::{Debug, Display};
use std::iter;
use std::marker::PhantomData;
use std::ops::{ControlFlow, Range};
use std::sync::{Arc, LazyLock};

use itertools::Itertools;
use ruff_index::{Idx, IndexVec, newtype_index};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::Program;
use ty_python_core::rank::RankBitBox;
use ty_static::EnvVars;

use crate::types::class::GenericAlias;
use crate::types::constraints::projection::{ProjectionError, SolutionBudget};
use crate::types::constraints::support::{Support, SupportId};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarInstance, TypeVarSet};
use crate::types::visitor::{
    TypeCollector, TypeKind, TypeVisitor, walk_non_atomic_type, walk_type_with_recursion_guard,
};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, IntersectionType, Type, TypeContext,
    TypeMapping, TypePair, TypeVarBoundOrConstraints, TypeVarVariance, UnionType,
};
use crate::{Db, FxIndexMap, FxIndexSet, FxOrderSet, ProgramEnvironment};

pub(crate) mod paths;
pub(crate) mod projection;
mod sequents;
mod solutions;
mod support;

use paths::PathAssignments;
use sequents::SequentMap;
use solutions::SolutionWalker;

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
    /// [`is_trivially_always_satisfied`][ConstraintSet::is_trivially_always_satisfied], then the
    /// overall result must be as well, and we stop consuming elements from the iterator.
    fn when_any<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c>;

    /// Returns the constraints under which every element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_trivially_never_satisfied`][ConstraintSet::is_trivially_never_satisfied], then the
    /// overall result must be as well, and we stop consuming elements from the iterator.
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
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        mut f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let (node, source_order) = NodeId::distributed_or(
            builder,
            self.map(|element| {
                let constraint = f(element);
                constraint.verify_builder(builder);
                (constraint.node, constraint.source_order)
            }),
        );
        ConstraintSet::from_node(builder, node, source_order)
    }

    fn when_all<'db, 'c>(
        self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        mut f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let (node, source_order) = NodeId::distributed_and(
            builder,
            self.map(|element| {
                let constraint = f(element);
                constraint.verify_builder(builder);
                (constraint.node, constraint.source_order)
            }),
        );
        ConstraintSet::from_node(builder, node, source_order)
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
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct OwnedConstraintSet<'db> {
    node: NodeId,
    source_order: Option<SourceOrderId>,
    inner: Option<Arc<OwnedConstraintSetInner<'db>>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
struct OwnedConstraintSetInner<'db> {
    constraints: Box<[Constraint<'db>]>,
    constraint_supports: Box<[SupportId]>,
    constraint_indices: RankBitBox,
    typevars: IndexVec<TypeVarId, BoundTypeVarInstance<'db>>,
    nodes: Box<[InteriorNodeData]>,
    node_supports: Box<[SupportId]>,
    node_indices: RankBitBox,
    supports: Box<[Support]>,
    support_indices: RankBitBox,
    /// A dense, canonical source-order tree whose IDs are independent of sidecar construction
    /// history.
    source_orders: Box<[SourceOrder]>,
}

impl Default for OwnedConstraintSet<'_> {
    fn default() -> Self {
        Self {
            node: ALWAYS_FALSE,
            source_order: None,
            inner: None,
        }
    }
}

impl<'db> OwnedConstraintSet<'db> {
    pub(crate) fn always() -> Self {
        Self {
            node: ALWAYS_TRUE,
            source_order: None,
            inner: None,
        }
    }

    /// Returns `true` if this constraint set's root is the `always` terminal.
    ///
    /// This is only a cheap sufficient check. A nonterminal constraint set can also be always
    /// satisfied, so `false` does not prove that the set is not always satisfied. Call
    /// [`ConstraintSet::is_always_satisfied`] through [`Self::query`] when false negatives are not
    /// acceptable.
    pub(crate) fn is_trivially_always_satisfied(&self) -> bool {
        self.node == ALWAYS_TRUE
    }

    /// Loads this constraint set into a new builder, invokes a callback with that builder, and
    /// returns the result.
    ///
    /// This is more efficient than [`ConstraintSetBuilder::load`] when this is the only set you
    /// need to load into the new builder.
    pub(crate) fn query<F, R>(&self, f: F) -> R
    where
        F: for<'c> FnOnce(&'c ConstraintSetBuilder<'db>, ConstraintSet<'db, 'c>) -> R,
    {
        let storage = ConstraintSetStorage {
            compacted: self.inner.clone(),
            ..ConstraintSetStorage::default()
        };
        let builder = ConstraintSetBuilder {
            storage: RefCell::new(storage),
        };
        let set = ConstraintSet::from_node(&builder, self.node, self.source_order);
        f(&builder, set)
    }

    /// Returns the types in constraints that are still reachable from the decision diagram.
    ///
    /// Source ordering can retain constraints that are no longer in the diagram, but their type
    /// variables must not participate in semantic walks or callable freshening.
    pub(crate) fn types(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        self.inner.iter().flat_map(|inner| {
            inner
                .nodes
                .iter()
                .map(|node| node.constraint)
                .unique()
                .map(|constraint| inner.constraints[inner.retained_constraint_index(constraint)])
                .flat_map(|constraint| {
                    [
                        Type::TypeVar(constraint.typevar),
                        constraint.bounds.lower_bound().ty(),
                        constraint.bounds.upper_bound().ty(),
                    ]
                })
        })
    }
}

impl OwnedConstraintSetInner<'_> {
    fn retained_node_index(&self, id: NodeId) -> usize {
        let index = id.index();
        debug_assert_eq!(
            self.node_indices.get_bit(index),
            Some(true),
            "should not access constraint set node that was marked unused",
        );
        self.node_indices.rank(index) as usize
    }

    fn retained_constraint_index(&self, id: ConstraintId) -> usize {
        let index = id.index();
        debug_assert_eq!(
            self.constraint_indices.get_bit(index),
            Some(true),
            "should not access constraint set constraint that was marked unused",
        );
        self.constraint_indices.rank(index) as usize
    }

    fn retained_support_index(&self, id: SupportId) -> usize {
        let index = id.index();
        debug_assert_eq!(
            self.support_indices.get_bit(index),
            Some(true),
            "should not access constraint set support that was marked unused",
        );
        self.support_indices.rank(index) as usize
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

    /// The source ordering of the constraints in this constraint set. Will be `None` for terminal
    /// nodes.
    source_order: Option<SourceOrderId>,

    /// A reference to the builder that holds the storage for this constraint set's BDD
    builder: &'c ConstraintSetBuilder<'db>,

    /// Ensures that the `'c` lifetime is invariant
    _invariant: PhantomData<fn(&'c ()) -> &'c ()>,
}

impl<'db, 'c> ConstraintSet<'db, 'c> {
    fn from_node(
        builder: &'c ConstraintSetBuilder<'db>,
        node: NodeId,
        source_order: Option<SourceOrderId>,
    ) -> Self {
        Self {
            node,
            source_order,
            builder,
            _invariant: PhantomData,
        }
    }

    fn never(builder: &'c ConstraintSetBuilder<'db>) -> Self {
        Self::from_node(builder, ALWAYS_FALSE, None)
    }

    fn always(builder: &'c ConstraintSetBuilder<'db>) -> Self {
        Self::from_node(builder, ALWAYS_TRUE, None)
    }

    pub(crate) fn from_bool(builder: &'c ConstraintSetBuilder<'db>, b: bool) -> Self {
        if b {
            Self::always(builder)
        } else {
            Self::never(builder)
        }
    }

    /// Returns a constraint set that constrains a typevar to an explicit range of types.
    pub(crate) fn constrain_typevar(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Self {
        Self::constrain_typevar_with_bounds(
            db,
            env,
            builder,
            typevar,
            Some(ConstraintBound::Evidence(lower)),
            Some(ConstraintBound::Evidence(upper)),
        )
    }

    /// Returns a constraint set that constrains a typevar with explicit lower and/or upper bounds.
    pub(crate) fn constrain_typevar_with_bounds(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Option<ConstraintBound<'db>>,
        upper: Option<ConstraintBound<'db>>,
    ) -> Self {
        let mut storage = builder.storage.borrow_mut();
        let (node, source_order) =
            Constraint::new_node_with_bounds(db, env, &mut storage, typevar, lower, upper);
        Self::from_node(builder, node, source_order)
    }

    /// Returns a constraint set that constrains a typevar to be a supertype of `lower`.
    pub(crate) fn constrain_typevar_lower_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
    ) -> Self {
        Self::constrain_typevar_with_bounds(
            db,
            env,
            builder,
            typevar,
            Some(ConstraintBound::Evidence(lower)),
            None,
        )
    }

    /// Returns a constraint set that constrains a typevar to be a subtype of `upper`.
    pub(crate) fn constrain_typevar_upper_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        Self::constrain_typevar_with_bounds(
            db,
            env,
            builder,
            typevar,
            None,
            Some(ConstraintBound::Evidence(upper)),
        )
    }

    /// Verifies that this constraint set was created by `builder`
    #[track_caller]
    fn verify_builder(self, builder: &'c ConstraintSetBuilder<'db>) {
        debug_assert!(std::ptr::eq(self.builder, builder));
    }

    /// Returns whether this constraint set never holds, without checking the type variables'
    /// declared bounds or constraints. Use [`Self::has_no_valid_solutions`] to include those.
    pub(crate) fn is_never_satisfied(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        let mut storage = self.builder.storage.borrow_mut();
        self.node
            .is_never_satisfied(db, env, &mut storage, self.source_order)
    }

    /// Returns whether no specialization satisfying the type variables' upper bounds and
    /// constraints can satisfy this constraint set.
    ///
    /// Unlike [`Self::is_never_satisfied`], this validates solutions against the type variables'
    /// upper bounds and constraints. For example, `T = int` is not contradictory by itself, but has
    /// no valid solution if `T` has an upper bound of `str`.
    ///
    /// If the solver reaches its computation limit, we do not know whether a valid solution exists.
    /// This returns `false` in that case: stopping the search is not proof that there is no solution.
    pub(crate) fn has_no_valid_solutions(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        if self.is_never_satisfied(db, env) {
            return true;
        }

        let inferable = {
            let storage = self.builder.storage.borrow();
            let Some(support) = storage.node_support(self.node) else {
                return false;
            };
            // For overlap, every mentioned type variable can choose a valid specialization.
            TypeVarSet::from_typevars(db, support.iter().map(|id| storage.typevar_data(id)))
        };

        matches!(
            self.solutions(db, env, inferable),
            Ok(Solutions::Unsatisfiable)
        )
    }

    /// Returns whether this constraint set is the `never` terminal.
    ///
    /// A nonterminal constraint set can also never be satisfied, so `false` does not prove that
    /// the set is satisfiable. Use [`Self::is_never_satisfied`] when false negatives are not
    /// acceptable.
    pub(crate) fn is_trivially_never_satisfied(self) -> bool {
        self.node == ALWAYS_FALSE
    }

    /// Returns whether this constraint set always holds.
    #[inline]
    pub(crate) fn is_always_satisfied(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        let mut storage = self.builder.storage.borrow_mut();
        self.node
            .is_always_satisfied(db, env, &mut storage, self.source_order)
    }

    /// Returns whether this constraint set is the `always` terminal.
    ///
    /// A nonterminal constraint set can also always be satisfied, so `false` does not prove that
    /// the set is not always satisfied. Use [`Self::is_always_satisfied`] when false negatives are
    /// not acceptable.
    pub(crate) fn is_trivially_always_satisfied(self) -> bool {
        self.node == ALWAYS_TRUE
    }

    /// Returns whether this constraint set mentions the given type-variable identity.
    pub(super) fn mentions_typevar(self, typevar: BoundTypeVarInstance<'db>) -> bool {
        let storage = self.builder.storage.borrow();
        storage
            .node_support(self.node)
            .is_some_and(|support| support.iter().any(|id| storage.typevar_data(id) == typevar))
    }

    /// Returns the constraints under which `lhs` is a subtype of `rhs`, assuming that the
    /// constraints in this constraint set hold. Panics if neither of the types being compared are
    /// a typevar. (That case is handled by `Type::has_relation_to`.)
    pub(crate) fn implies_subtype_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        lhs: Type<'db>,
        rhs: Type<'db>,
    ) -> Self {
        self.verify_builder(builder);
        let mut storage = builder.storage.borrow_mut();
        let (node, extra_source_order) =
            self.node
                .implies_subtype_of(db, env, &mut storage, lhs, rhs);
        let source_order = storage.ordered_source_order(self.source_order, extra_source_order);
        Self::from_node(builder, node, source_order)
    }

    /// Updates this constraint set to hold the union of itself and another constraint set.
    ///
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn union(
        &mut self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: Self,
    ) -> Self {
        self.verify_builder(builder);
        let mut storage = builder.storage.borrow_mut();
        self.node = self.node.or(&mut storage, other.node);
        self.source_order = storage.ordered_source_order(self.source_order, other.source_order);
        *self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    ///
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn intersect(
        &mut self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: Self,
    ) -> Self {
        self.verify_builder(builder);
        let mut storage = builder.storage.borrow_mut();
        self.node = self.node.and(&mut storage, other.node);
        self.source_order = storage.ordered_source_order(self.source_order, other.source_order);
        *self
    }

    /// Returns the negation of this constraint set.
    pub(crate) fn negate(self, _db: &'db dyn Db, builder: &'c ConstraintSetBuilder<'db>) -> Self {
        self.verify_builder(builder);
        let mut storage = builder.storage.borrow_mut();
        Self::from_node(builder, self.node.negate(&mut storage), self.source_order)
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    ///
    /// In the result's source order, `self` will appear before `other`.
    #[inline]
    pub(crate) fn and(
        mut self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> Self,
    ) -> Self {
        self.verify_builder(builder);
        if !self.is_trivially_never_satisfied() {
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
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn or(
        mut self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: impl FnOnce() -> Self,
    ) -> Self {
        self.verify_builder(builder);
        if !self.is_trivially_always_satisfied() {
            let other = other();
            other.verify_builder(builder);
            self.union(db, builder, other);
        }
        self
    }

    /// Returns a constraint set encoding that this constraint set implies another.
    ///
    /// In the result's source order, `self` will appear before `other`.
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
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn iff(
        self,
        _db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        other: Self,
    ) -> Self {
        self.verify_builder(builder);
        let mut storage = builder.storage.borrow_mut();
        let node = self.node.iff(&mut storage, other.node);
        let source_order = storage.ordered_source_order(self.source_order, other.source_order);
        Self::from_node(builder, node, source_order)
    }

    /// Reduces the set of inferable typevars for this constraint set. You provide the typevars that
    /// were inferable when this constraint set was created, and which should be abstracted away.
    /// Those typevars will be removed from the constraint set, and the constraint set will return
    /// true whenever there was _any_ specialization of those typevars that returned true before.
    pub(crate) fn reduce_inferable(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        to_remove: TypeVarSet<'db>,
    ) -> Self {
        self.verify_builder(builder);
        if to_remove == TypeVarSet::None {
            return self;
        }
        let mut storage = builder.storage.borrow_mut();
        let (node, derived_source_order) =
            self.node
                .exists(db, env, &mut storage, to_remove, self.source_order);
        // The eliminated typevars must also leave the source-order history. Otherwise recursive
        // relations can re-import each other's quantified constraints after their live graphs have
        // stabilized. Keep the original order of the remaining entries and append derived facts.
        let source_order = storage
            .calculate_source_orders(self.source_order)
            .into_iter()
            .fold(None, |source_order, constraint| {
                if storage.constraint_mentions_typevars(db, constraint, to_remove) {
                    return source_order;
                }
                let constraint_source_order = storage.constraint_source_order(constraint);
                storage.ordered_source_order(source_order, Some(constraint_source_order))
            });
        let source_order = storage.ordered_source_order(source_order, derived_source_order);
        Self::from_node(builder, node, source_order)
    }

    /// Applies a type mapping to every constraint in this constraint set.
    pub(crate) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        fn rebuild_node(
            storage: &mut ConstraintSetStorage<'_>,
            old_node: NodeId,
            mapped_constraints: &FxHashMap<ConstraintId, (NodeId, Option<SourceOrderId>)>,
            mapped_nodes: &mut FxHashMap<NodeId, NodeId>,
        ) -> NodeId {
            if old_node.is_terminal() {
                return old_node;
            }
            if let Some(mapped) = mapped_nodes.get(&old_node) {
                return *mapped;
            }

            let old_interior = storage.interior_node_data(old_node);
            let (condition, _) = mapped_constraints[&old_interior.constraint];
            let if_true = rebuild_node(
                storage,
                old_interior.if_true,
                mapped_constraints,
                mapped_nodes,
            );
            let if_uncertain = rebuild_node(
                storage,
                old_interior.if_uncertain,
                mapped_constraints,
                mapped_nodes,
            );
            let if_false = rebuild_node(
                storage,
                old_interior.if_false,
                mapped_constraints,
                mapped_nodes,
            );
            let mapped = condition.ite_uncertain(storage, if_true, if_uncertain, if_false);
            mapped_nodes.insert(old_node, mapped);
            mapped
        }

        // We have to collect this into a temporary vec since we can't hold an open borrow on the
        // storage during the apply_type_mapping calls below, since they also need to borrow the
        // storage.
        let storage = self.builder.storage.borrow();
        let mut constraints = SmallVec::<[_; 8]>::new();
        self.node
            .for_each_unique_constraint(&storage, &mut |constraint_id| {
                let constraint = storage.constraint_data(constraint_id);
                constraints.push((constraint_id, constraint));
            });
        // Mapping can intern constraints and typevars. Preserve their source order rather than
        // letting the old diagram's variable order determine the rebuilt diagram's ordering.
        let source_orders = storage.calculate_source_orders(self.source_order);
        constraints.sort_unstable_by_key(|(constraint, _)| source_orders.get_index_of(constraint));
        drop(storage);

        let mut mapped_constraints = FxHashMap::default();
        for (constraint_id, constraint) in constraints {
            if mapped_constraints.contains_key(&constraint_id) {
                continue;
            }

            let subject = Type::TypeVar(constraint.typevar).apply_type_mapping_impl(
                db,
                type_mapping,
                tcx,
                visitor,
            );
            let lower = constraint.bounds.lower.map(|lower| {
                lower.map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            });
            let upper = constraint.bounds.upper.map(|upper| {
                upper.map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            });

            let env = visitor.env;
            let mut storage = self.builder.storage.borrow_mut();
            let mapped = if let Type::TypeVar(typevar) = subject {
                Constraint::new_node_with_bounds(db, env, &mut storage, typevar, lower, upper)
            } else {
                let (lower_holds, lower_holds_source_order) = match lower {
                    Some(lower) => storage.load(
                        db,
                        env,
                        &lower
                            .ty()
                            .when_constraint_set_assignable_to_owned(db, env, subject),
                    ),
                    None => (ALWAYS_TRUE, None),
                };
                let (upper_holds, upper_holds_source_order) = match upper {
                    Some(upper) => storage.load(
                        db,
                        env,
                        &subject.when_constraint_set_assignable_to_owned(db, env, upper.ty()),
                    ),
                    None => (ALWAYS_TRUE, None),
                };
                (
                    lower_holds.and(&mut storage, upper_holds),
                    storage
                        .ordered_source_order(lower_holds_source_order, upper_holds_source_order),
                )
            };
            mapped_constraints.insert(constraint_id, mapped);
        }

        let mut storage = self.builder.storage.borrow_mut();
        let source_order = source_orders
            .into_iter()
            .fold(None, |source_order, constraint| {
                mapped_constraints.get(&constraint).map_or(
                    source_order,
                    |(_, mapped_source_order)| {
                        storage.ordered_source_order(source_order, *mapped_source_order)
                    },
                )
            });
        Self::from_node(
            self.builder,
            rebuild_node(
                &mut storage,
                self.node,
                &mapped_constraints,
                &mut FxHashMap::default(),
            ),
            source_order,
        )
    }

    /// Universally abstracts constraints involving the given type variables from this TDD.
    ///
    /// This is the Boolean dual of [`Self::reduce_inferable`]. Declared type variable bounds and
    /// constraints are not applied implicitly, and must be encoded as implications in the input
    /// constraint set.
    ///
    /// # Preconditions
    ///
    /// An atomic constraint must not relate a removed type variable to one that remains in the
    /// result. Callers that need type-level quantification must project those relationships before
    /// calling this method.
    pub(crate) fn for_all(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        to_remove: TypeVarSet<'db>,
    ) -> Self {
        self.verify_builder(builder);
        if to_remove == TypeVarSet::None {
            return self;
        }

        // Universal and existential quantification are duals. Reusing existential abstraction
        // also keeps this operation on its cached, single-pass implementation.
        self.negate(db, builder)
            .reduce_inferable(db, env, builder, to_remove)
            .negate(db, builder)
    }

    pub(crate) fn display(
        self,
        db: &'db dyn Db,
        env: &'c ProgramEnvironment<'db>,
    ) -> impl Display + 'c {
        std::fmt::from_fn(move |f| {
            let storage = self.builder.storage.borrow();
            self.node.display(db, env, &storage).fmt(f)
        })
    }

    #[expect(dead_code)] // Keep this around for debugging purposes
    fn display_graph<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        prefix: &'a dyn Display,
    ) -> impl Display + 'a
    where
        'db: 'a,
        'c: 'a,
    {
        std::fmt::from_fn(move |f| {
            let storage = self.builder.storage.borrow();
            self.node.display_graph(db, env, &storage, prefix).fmt(f)
        })
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

type ExistsCacheKey<'db> = (NodeId, TypeVarSet<'db>, Option<SourceOrderId>);

#[derive(Debug, Default)]
struct ConstraintSetStorage<'db> {
    /// Compacted owned storage overlaid onto this builder. This is used by
    /// [`OwnedConstraintSet::query`] to create a [`ConstraintSetBuilder`] that is initially a
    /// read-only view of the owned constraint set's storage.
    ///
    /// IDs below the overlay split points are looked up in this storage; newly interned entries
    /// are stored in the dense local arenas below.
    compacted: Option<Arc<OwnedConstraintSetInner<'db>>>,

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
    typevars: IndexVec<TypeVarId, BoundTypeVarInstance<'db>>,

    /// The BDD nodes that appear in any of the constraint sets constructed in this builder.
    nodes: IndexVec<NodeId, InteriorNodeData>,

    supports: IndexVec<SupportId, Support>,
    constraint_supports: IndexVec<ConstraintId, SupportId>,
    node_supports: IndexVec<NodeId, SupportId>,

    /// Encodes an ordering on the constraints in a constraint set, which is based on the order
    /// that the constraints (or more accurately, the Python expressions they're derived from)
    /// appear in the source code. This ensures that any union and intersections types that appear
    /// in solutions are constructed in a stable (and source-consistent) order.
    ///
    /// This is encoded as an interned binary DAG over [`ConstraintId`]s. The first occurrence of
    /// each constraint in a left-first traversal defines the ordering.
    source_orders: IndexVec<SourceOrderId, SourceOrder>,

    // Everything below are the memoization tables for the arenas and for our BDD operations.
    constraint_cache: FxHashMap<Constraint<'db>, ConstraintId>,
    typevar_cache: FxHashMap<BoundTypeVarIdentity<'db>, TypeVarId>,
    node_cache: FxHashMap<InteriorNodeData, NodeId>,
    /// Avoid repeatedly walking deep constraint bounds without imposing Salsa-query overhead on
    /// the many shallow bounds that are cheap to walk once.
    constraint_bound_depth_cache: FxHashMap<ConstraintId, (u16, u16)>,
    source_order_cache: FxHashMap<SourceOrder, SourceOrderId>,
    constraint_implication_cache: FxHashMap<(ConstraintId, ConstraintId), bool>,
    /// Only caches completed top-level results. Recursive results depend on active path
    /// assignments and must not use this cache. A BDD's satisfiability does not depend on the
    /// source order used to traverse it.
    never_satisfied_cache: FxHashMap<NodeId, bool>,

    negate_cache: FxHashMap<NodeId, NodeId>,
    or_cache: FxHashMap<(NodeId, NodeId), NodeId>,
    and_cache: FxHashMap<(NodeId, NodeId), NodeId>,
    /// Existential abstraction derives new constraints in source order and returns their
    /// source-order sidecar, so distinct orderings of the same BDD must not share a cache entry.
    exists_cache: FxHashMap<ExistsCacheKey<'db>, (NodeId, Option<SourceOrderId>)>,

    single_sequent_cache: FxHashMap<ConstraintId, SequentMap>,
    pair_sequent_cache: FxHashMap<(ConstraintId, ConstraintId), SequentMap>,
    constraint_set_subtype_cache: FxHashMap<(Type<'db>, Type<'db>), bool>,
}

impl<'db> ConstraintSetStorage<'db> {
    fn ensure_overlay_identity_caches(&mut self) {
        let Some(compacted) = &self.compacted else {
            return;
        };
        if !self.node_cache.is_empty() {
            return;
        }

        self.constraint_cache.extend(
            compacted
                .constraint_indices
                .iter_ones()
                .zip(compacted.constraints.iter().copied())
                .map(|(old_index, constraint)| (constraint, ConstraintId::from_usize(old_index))),
        );
        self.node_cache.extend(
            compacted
                .node_indices
                .iter_ones()
                .zip(compacted.nodes.iter().copied())
                .map(|(old_index, node)| (node, NodeId::from_usize(old_index))),
        );
        self.source_order_cache.extend(
            compacted
                .source_orders
                .iter()
                .copied()
                .enumerate()
                .map(|(index, source_order)| (source_order, SourceOrderId::from_usize(index))),
        );
    }

    // This is a separate method from `ensure_overlay_identity_caches` because it requires a `db`.
    fn ensure_overlay_typevar_identity_cache(&mut self, db: &'db dyn Db) {
        let Some(compacted) = &self.compacted else {
            return;
        };
        if !self.typevar_cache.is_empty() {
            return;
        }

        self.typevar_cache.extend(
            compacted
                .typevars
                .iter_enumerated()
                .map(|(id, typevar)| (typevar.identity(db), id)),
        );
    }

    fn adjusted_node_id(&self, id: NodeId) -> NodeId {
        if let Some(compacted) = &self.compacted {
            return id + compacted.node_indices.len();
        }
        id
    }

    fn adjusted_constraint_id(&self, id: ConstraintId) -> ConstraintId {
        if let Some(compacted) = &self.compacted {
            return id + compacted.constraint_indices.len();
        }
        id
    }

    fn adjusted_support_id(&self, id: SupportId) -> SupportId {
        if let Some(compacted) = &self.compacted {
            return id + compacted.support_indices.len();
        }
        id
    }

    fn adjusted_source_order_id(&self, id: SourceOrderId) -> SourceOrderId {
        if let Some(compacted) = &self.compacted {
            return id + compacted.source_orders.len();
        }
        id
    }

    fn adjusted_typevar_id(&self, id: TypeVarId) -> TypeVarId {
        if let Some(compacted) = &self.compacted {
            return id + compacted.typevars.len();
        }
        id
    }
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
        // constraint sets can only be used by adding them to a new builder. Operation caches from
        // the original builder aren't relevant to the new builder, and don't need to be retained.
        let constraint = f(&self);
        let node = constraint.node;
        if node.is_terminal() {
            return OwnedConstraintSet {
                node,
                source_order: None,
                inner: None,
            };
        }
        let source_order = constraint
            .source_order
            .expect("non-terminal BDD should have source_order");

        // Combining constraint sets can allocate a new source-order tree even when the BDD is
        // unchanged. Preserve each relevant constraint's first source position, but rebuild the
        // persisted sidecar densely so redundant combinations cannot affect its IDs or owned-set
        // equality. Unlike node and constraint IDs, source-order IDs are not embedded in the BDD,
        // so the sidecar can be rebuilt without remapping the BDD.
        let mut storage = self.storage.into_inner();
        let source_constraints = storage.calculate_source_orders(Some(source_order));

        let mut used_nodes = RankBitBox::bits_with_capacity(storage.nodes.len());
        let mut used_constraints = RankBitBox::bits_with_capacity(storage.constraints.len());
        let mut used_supports = RankBitBox::bits_with_capacity(storage.supports.len());

        let mut stack = vec![node];
        while let Some(node) = stack.pop() {
            if node.is_terminal() || used_nodes[node.index()] {
                continue;
            }
            let interior = storage.interior_node_data(node);
            let node_support = storage
                .node_support_id(node)
                .expect("node should be non-terminal");
            let constraint_support = storage.constraint_support_id(interior.constraint);
            used_nodes.set(node.index(), true);
            used_constraints.set(interior.constraint.index(), true);
            used_supports.set(node_support.index(), true);
            used_supports.set(constraint_support.index(), true);
            stack.push(interior.if_true);
            stack.push(interior.if_uncertain);
            stack.push(interior.if_false);
        }

        let mut source_orders: IndexVec<SourceOrderId, SourceOrder> =
            IndexVec::with_capacity(source_constraints.len().saturating_mul(2).saturating_sub(1));
        let live_support = storage.node_support(node);
        let source_order = source_constraints
            .into_iter()
            .fold(None, |left, source_constraint| {
                // Preserve ordering history for absorbed constraints related to the live graph.
                // Unrelated history can retain fresh typevars and prevent recursive Salsa queries
                // from reaching a fixed point. Incomplete supports may hide a relationship, so
                // preserve those entries.
                let constraint_support_id = storage.constraint_support_id(source_constraint);
                let constraint_support = storage.support_data(constraint_support_id);
                if !used_constraints[source_constraint.index()]
                    && let Some(live_support) = live_support
                    && live_support.is_complete()
                    && constraint_support.is_complete()
                    && !constraint_support.overlaps_with(live_support)
                {
                    return left;
                }
                used_constraints.set(source_constraint.index(), true);
                // Source-order-only constraints are reloaded too, so retain their supports.
                used_supports.set(constraint_support_id.index(), true);
                let right = source_orders.push(SourceOrder::Constraint(source_constraint));

                Some(match left {
                    Some(left) => source_orders.push(SourceOrder::Ordered(left, right)),
                    None => right,
                })
            })
            .expect("non-terminal BDD should have source_order");

        used_nodes.truncate(used_nodes.last_one().map_or(0, |last| last + 1));
        used_constraints.truncate(used_constraints.last_one().map_or(0, |last| last + 1));
        used_supports.truncate(used_supports.last_one().map_or(0, |last| last + 1));

        let nodes = storage
            .nodes
            .into_iter()
            .zip(&used_nodes)
            .filter_map(|(node, used)| used.then_some(node))
            .collect();
        let node_supports = storage
            .node_supports
            .into_iter()
            .zip(&used_nodes)
            .filter_map(|(support, used)| used.then_some(support))
            .collect();
        let node_indices = RankBitBox::from_bits(used_nodes);

        let constraints = storage
            .constraints
            .into_iter()
            .zip(&used_constraints)
            .filter_map(|(constraint, used)| used.then_some(constraint))
            .collect();
        let constraint_supports = storage
            .constraint_supports
            .into_iter()
            .zip(&used_constraints)
            .filter_map(|(support, used)| used.then_some(support))
            .collect();
        let constraint_indices = RankBitBox::from_bits(used_constraints);

        let supports = storage
            .supports
            .into_iter()
            .zip(&used_supports)
            .filter_map(|(support, used)| used.then_some(support))
            .collect();
        let support_indices = RankBitBox::from_bits(used_supports);

        storage.typevars.shrink_to_fit();

        OwnedConstraintSet {
            node,
            source_order: Some(source_order),
            inner: Some(Arc::new(OwnedConstraintSetInner {
                constraints,
                constraint_supports,
                constraint_indices,
                typevars: storage.typevars,
                nodes,
                node_supports,
                node_indices,
                supports,
                support_indices,
                source_orders: source_orders.raw.into_boxed_slice(),
            })),
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
        env: &ProgramEnvironment<'db>,
        other: &OwnedConstraintSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let mut storage = self.storage.borrow_mut();
        let (node, source_order) = storage.load(db, env, other);
        ConstraintSet::from_node(self, node, source_order)
    }
}

impl<'db> ConstraintSetStorage<'db> {
    /// Interns a single typevar, giving it a stable order in this builder
    fn intern_typevar(&mut self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarId {
        self.ensure_overlay_identity_caches();
        self.ensure_overlay_typevar_identity_cache(db);
        let identity = typevar.identity(db);
        if let Some(id) = self.typevar_cache.get(&identity) {
            return *id;
        }
        let id = self.typevars.push(typevar);
        let id = self.adjusted_typevar_id(id);
        self.typevar_cache.insert(identity, id);
        id
    }

    /// Interns all of the typevars mentioned in a type in a stable order.
    fn intern_mentioned_typevars_in_type(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        support: &mut Support,
    ) {
        struct InternMentionedTypevars<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            storage: RefCell<&'a mut ConstraintSetStorage<'db>>,
            support: RefCell<&'a mut Support>,
            recursion_guard: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for InternMentionedTypevars<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }

            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn notify_skipped_lazy_type_attributes(&self) {
                self.support.borrow_mut().mark_incomplete();
            }

            fn visit_type_var_type(&self, _db: &'db dyn Db, _typevar: TypeVarInstance<'db>) {
                // Declaration bounds, constraints, and defaults are not occurrences in the
                // constraint itself and must not contribute to its support.
            }

            fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
                for ty in alias.specialization(db).types(db) {
                    self.visit_type(db, *ty);
                }
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                if let Type::TypeVar(bound_typevar) = ty {
                    let mut storage = self.storage.borrow_mut();
                    let typevar = storage.intern_typevar(db, bound_typevar);
                    let mut support = self.support.borrow_mut();
                    support.insert(typevar);
                }
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        InternMentionedTypevars {
            env,
            storage: RefCell::new(self),
            support: RefCell::new(support),
            recursion_guard: TypeCollector::default(),
        }
        .visit_type(db, ty);
    }

    /// Interns all of the typevars mentioned in a constraint in a stable order.
    fn intern_constraint_typevars(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarInstance<'db>,
        bounds: ConstraintBounds<'db>,
    ) -> Support {
        let mut support = Support::default();
        support.insert(self.intern_typevar(db, typevar));
        if let Some(lower) = bounds.lower {
            self.intern_mentioned_typevars_in_type(db, env, lower.ty(), &mut support);
        }
        if let Some(upper) = bounds.upper {
            self.intern_mentioned_typevars_in_type(db, env, upper.ty(), &mut support);
        }
        support
    }

    fn intern_constraint(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        data: Constraint<'db>,
    ) -> ConstraintId {
        let support = self.intern_constraint_typevars(db, env, data.typevar, data.bounds);

        self.ensure_overlay_identity_caches();
        if let Some(id) = self.constraint_cache.get(&data) {
            return *id;
        }
        let support_id = self.intern_support(support);
        let id = self.constraints.push(data);
        self.constraint_supports.push(support_id);
        let id = self.adjusted_constraint_id(id);
        self.constraint_cache.insert(data, id);
        id
    }

    fn intern_interior_node(&mut self, data: InteriorNodeData) -> NodeId {
        self.ensure_overlay_identity_caches();
        if let Some(id) = self.node_cache.get(&data) {
            return *id;
        }

        let mut support = Support::default();
        support |= self.constraint_support(data.constraint);
        support |= self.node_support(data.if_true);
        support |= self.node_support(data.if_uncertain);
        support |= self.node_support(data.if_false);
        let support = self.intern_support(support);

        let id = self.nodes.push(data);
        self.node_supports.push(support);
        let id = self.adjusted_node_id(id);
        self.node_cache.insert(data, id);
        id
    }

    fn typevar_id(&mut self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarId {
        let identity = typevar.identity(db);
        self.ensure_overlay_identity_caches();
        self.ensure_overlay_typevar_identity_cache(db);
        self.typevar_cache
            .get(&identity)
            .copied()
            .expect("typevar should be interned before ordering")
    }

    fn constraint_data(&self, constraint: ConstraintId) -> Constraint<'db> {
        if let Some(compacted) = &self.compacted {
            let index = constraint.index();
            let split = compacted.constraint_indices.len();
            if index < split {
                let compacted_index = compacted.retained_constraint_index(constraint);
                return compacted.constraints[compacted_index];
            }
            return self.constraints[ConstraintId::from_usize(index - split)];
        }
        self.constraints[constraint]
    }

    fn cached_constraint_bound_depth(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraint: ConstraintId,
    ) -> (u16, u16) {
        if let Some(depth) = self.constraint_bound_depth_cache.get(&constraint) {
            return *depth;
        }

        let depth = self.constraint_data(constraint).bound_depth(db, env);
        self.constraint_bound_depth_cache.insert(constraint, depth);
        depth
    }

    fn cached_constraint_implies(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ante: ConstraintId,
        post: ConstraintId,
    ) -> bool {
        let key = (ante, post);
        if let Some(result) = self.constraint_implication_cache.get(&key) {
            return *result;
        }

        let result = ante.implies(db, env, self, post);
        self.constraint_implication_cache.insert(key, result);
        result
    }

    fn cached_is_constraint_set_subtype_of(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        let key = (source, target);
        if let Some(result) = self.constraint_set_subtype_cache.get(&key) {
            return *result;
        }

        let result = source.is_constraint_set_subtype_of(db, env, target);
        self.constraint_set_subtype_cache.insert(key, result);
        result
    }

    fn interior_node_data(&self, node: NodeId) -> InteriorNodeData {
        if let Some(compacted) = &self.compacted {
            let index = node.index();
            let split = compacted.node_indices.len();
            if index < split {
                let compacted_index = compacted.retained_node_index(node);
                return compacted.nodes[compacted_index];
            }
            return self.nodes[NodeId::from_usize(index - split)];
        }
        self.nodes[node]
    }

    fn intern_source_order(&mut self, data: SourceOrder) -> SourceOrderId {
        self.ensure_overlay_identity_caches();
        if let Some(id) = self.source_order_cache.get(&data) {
            return *id;
        }
        let id = self.source_orders.push(data);
        let id = self.adjusted_source_order_id(id);
        self.source_order_cache.insert(data, id);
        id
    }

    /// Repeating a source-order tree cannot change the first occurrence of any constraint, so
    /// combining identical trees must reuse their existing sidecar.
    fn ordered_source_order(
        &mut self,
        left: Option<SourceOrderId>,
        right: Option<SourceOrderId>,
    ) -> Option<SourceOrderId> {
        match (left, right) {
            (None, None) => None,
            (None, other) | (other, None) => other,
            (Some(left), Some(right)) if left == right => Some(left),
            (Some(left), Some(right)) => {
                Some(self.intern_source_order(SourceOrder::Ordered(left, right)))
            }
        }
    }

    fn constraint_source_order(&mut self, constraint: ConstraintId) -> SourceOrderId {
        self.intern_source_order(SourceOrder::Constraint(constraint))
    }

    fn source_order_data(&self, source_order: SourceOrderId) -> SourceOrder {
        if let Some(compacted) = &self.compacted {
            let index = source_order.index();
            let split = compacted.source_orders.len();
            if index < split {
                return compacted.source_orders[index];
            }
            return self.source_orders[SourceOrderId::from_usize(index - split)];
        }
        self.source_orders[source_order]
    }

    fn calculate_source_orders(
        &self,
        source_order: Option<SourceOrderId>,
    ) -> FxIndexSet<ConstraintId> {
        // Source-order sidecars share interned subtrees. Revisiting a subtree cannot contribute
        // an earlier occurrence of any constraint, and can expand a small DAG exponentially.
        let mut pending = Vec::from_iter(source_order);
        let mut visited = FxHashSet::default();
        let mut result = FxIndexSet::default();
        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            match self.source_order_data(current) {
                SourceOrder::Ordered(left, right) => {
                    pending.extend([right, left]);
                }
                SourceOrder::Constraint(constraint) => {
                    result.insert(constraint);
                }
            }
        }
        result
    }

    fn intern_support(&mut self, data: Support) -> SupportId {
        let id = self.supports.push(data);
        self.adjusted_support_id(id)
    }

    fn typevar_data(&self, typevar: TypeVarId) -> BoundTypeVarInstance<'db> {
        if let Some(compacted) = &self.compacted {
            let index = typevar.index();
            let split = compacted.typevars.len();
            if index < split {
                return compacted.typevars[typevar];
            }
            return self.typevars[TypeVarId::from_usize(index - split)];
        }
        self.typevars[typevar]
    }

    fn support_data(&self, support: SupportId) -> &Support {
        if let Some(compacted) = &self.compacted {
            let index = support.index();
            let split = compacted.support_indices.len();
            if index < split {
                let compacted_index = compacted.retained_support_index(support);
                return &compacted.supports[compacted_index];
            }
            return &self.supports[SupportId::from_usize(index - split)];
        }
        &self.supports[support]
    }

    fn constraint_support_id(&self, constraint: ConstraintId) -> SupportId {
        if let Some(compacted) = &self.compacted {
            let index = constraint.index();
            let split = compacted.constraint_indices.len();
            if index < split {
                let compacted_index = compacted.retained_constraint_index(constraint);
                return compacted.constraint_supports[compacted_index];
            }
            return self.constraint_supports[ConstraintId::from_usize(index - split)];
        }
        self.constraint_supports[constraint]
    }

    fn constraint_support(&self, constraint: ConstraintId) -> &Support {
        self.support_data(self.constraint_support_id(constraint))
    }

    fn constraint_mentions_typevars(
        &self,
        db: &'db dyn Db,
        constraint: ConstraintId,
        typevars: TypeVarSet<'db>,
    ) -> bool {
        self.constraint_support(constraint)
            .iter()
            .any(|typevar| self.typevar_data(typevar).is_inferable(db, typevars))
    }

    fn node_support_id(&self, node: NodeId) -> Option<SupportId> {
        if node.is_terminal() {
            return None;
        }
        if let Some(compacted) = &self.compacted {
            let index = node.index();
            let split = compacted.node_indices.len();
            if index < split {
                let compacted_index = compacted.retained_node_index(node);
                return Some(compacted.node_supports[compacted_index]);
            }
            return Some(self.node_supports[NodeId::from_usize(index - split)]);
        }
        Some(self.node_supports[node])
    }

    fn node_support(&self, node: NodeId) -> Option<&Support> {
        self.node_support_id(node)
            .map(|support| self.support_data(support))
    }

    /// Loads an [`OwnedConstraintSet`] into this storage.
    fn load(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: &OwnedConstraintSet<'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        fn rebuild_node<'db>(
            storage: &mut ConstraintSetStorage<'db>,
            inner: &OwnedConstraintSetInner<'db>,
            constraints: &[(NodeId, Option<SourceOrderId>)],
            cache: &mut FxHashMap<NodeId, NodeId>,
            old_node: NodeId,
        ) -> NodeId {
            if old_node.is_terminal() {
                return old_node;
            }
            if let Some(remapped) = cache.get(&old_node) {
                return *remapped;
            }

            let old_node_index = inner.retained_node_index(old_node);
            let old_interior = inner.nodes[old_node_index];
            let if_true = rebuild_node(storage, inner, constraints, cache, old_interior.if_true);
            let if_uncertain = rebuild_node(
                storage,
                inner,
                constraints,
                cache,
                old_interior.if_uncertain,
            );
            let if_false = rebuild_node(storage, inner, constraints, cache, old_interior.if_false);
            let old_constraint_index = inner.retained_constraint_index(old_interior.constraint);
            let (condition, _) = constraints[old_constraint_index];
            let remapped = condition.ite_uncertain(storage, if_true, if_uncertain, if_false);

            cache.insert(old_node, remapped);
            remapped
        }

        if other.node.is_terminal() {
            return (other.node, None);
        }
        let inner = other
            .inner
            .as_ref()
            .expect("storage-free owned constraint sets must have terminal roots");

        // Restore the saved order of referenced typevars before rebuilding constraints. A stored
        // `T <= U` can have `U` as its subject and `T` as its lower bound. Interning that subject
        // first would reverse the original typevar order, causing successive loads to alternate
        // between equivalent representations and preventing recursive Salsa queries from converging.
        // Keep existing destination IDs, and omit typevars used only by discarded constraints.
        let mut referenced_typevars = Support::default();
        for support in &inner.constraint_supports {
            referenced_typevars |= &inner.supports[inner.retained_support_index(*support)];
        }
        for typevar in referenced_typevars.iter() {
            self.intern_typevar(db, inner.typevars[typevar]);
        }

        // Rebuild constraints in their saved order, using the destination's typevar ordering.
        let constraints: Box<[_]> = inner
            .constraints
            .iter()
            .map(|old_constraint| {
                Constraint::new_node_with_bounds(
                    db,
                    env,
                    self,
                    old_constraint.typevar,
                    old_constraint.bounds.lower,
                    old_constraint.bounds.upper,
                )
            })
            .collect();

        let mut source_orders = vec![None; inner.source_orders.len()];
        for (i, old_source_order) in inner.source_orders.iter().copied().enumerate() {
            match old_source_order {
                SourceOrder::Ordered(old_left, old_right) => {
                    let new_left = source_orders[old_left.index()];
                    let new_right = source_orders[old_right.index()];
                    source_orders[i] = self.ordered_source_order(new_left, new_right);
                }
                SourceOrder::Constraint(old_constraint) => {
                    let old_constraint_index = inner.retained_constraint_index(old_constraint);
                    let (_, constraint_source_order) = constraints[old_constraint_index];
                    source_orders[i] = constraint_source_order;
                }
            }
        }

        // Maps NodeIds in the OwnedConstraintSet to the corresponding NodeIds in this builder.
        let mut cache = FxHashMap::default();
        let node = rebuild_node(self, inner, &constraints, &mut cache, other.node);
        let old_source_order = other
            .source_order
            .expect("non-terminal constraint set should have a source_order");
        let source_order = source_orders[old_source_order.index()];
        (node, source_order)
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
        storage: &mut ConstraintSetStorage<'db>,
        typevar: Self,
    ) -> bool {
        wobble_index(storage.typevar_id(db, self).index())
            < wobble_index(storage.typevar_id(db, typevar).index())
    }
}

/// Optionally applies a transformation to a builder-local typevar or constraint ID, which lets us
/// exercise different BDD variable orderings.
///
/// Under normal operation, the IDs won't be modified, and we will construct BDDs based on the
/// (builder-local) source order that we encounter typevars and constraints.
///
/// Our results _shouldn't_ depend on the BDD variable ordering that we choose. You can use the
/// `TY_CONSTRAINT_SET_ORDER` environment variable to artificially choose different permutations of
/// the "natural" variable ordering, to ensure that results are consistent.
fn wobble_index(index: usize) -> usize {
    #[derive(Clone, Copy)]
    enum Order {
        Normal,
        Reverse,
        Xor(usize),
    }

    static ORDER: LazyLock<Order> = LazyLock::new(|| {
        let Some(value) = std::env::var_os(EnvVars::TY_CONSTRAINT_SET_ORDER) else {
            return Order::Normal;
        };
        if value == "reverse" {
            return Order::Reverse;
        }
        value
            .to_str()
            .and_then(|value| value.parse::<usize>().ok())
            .map_or(Order::Normal, Order::Xor)
    });

    match *ORDER {
        Order::Normal => index,
        Order::Reverse => !index,
        Order::Xor(mask) => index ^ mask,
    }
}

#[derive(Clone, Copy, Debug)]
enum IntersectionResult<'db> {
    Simplified(Constraint<'db>),
    CannotSimplify,
    Disjoint,
}

/// The index of a bound typevar within a [`ConstraintSetStorage`].
#[newtype_index]
#[derive(Ord, PartialOrd, get_size2::GetSize)]
pub struct TypeVarId;

/// The index of an individual constraint (i.e. a BDD variable) within a [`ConstraintSetStorage`].
#[newtype_index]
#[derive(get_size2::GetSize)]
pub struct ConstraintId;

#[newtype_index]
#[derive(get_size2::GetSize)]
struct SourceOrderId;

/// The nodes of the DAG that defines source ordering for a constraint set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum SourceOrder {
    Ordered(SourceOrderId, SourceOrderId),
    Constraint(ConstraintId),
}

/// An individual constraint in a constraint set. This restricts a single typevar to be within a
/// lower and upper bound.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct Constraint<'db> {
    typevar: BoundTypeVarInstance<'db>,
    bounds: ConstraintBounds<'db>,
}

/// The lower or upper bound of a constraint, along with its _provenance_
///
/// Most bounds come from specific relationships found at the call site — for instance, the
/// relationship between the argument type and parameter annotation when invoking a generic
/// function. These bounds express actual user intent, and are called _evidence_ bounds.
///
/// Other bounds are background limitations on which specializations are valid — for instance, a
/// typevar's declared `bound_or_constraints`. These are called _validity_ bounds. Importantly, we
/// don't want to choose a validity bound as a solution unless we have no other choice. There is
/// often an evidence bound that is a better choice.
///
/// A bound derived only from validity remains validity. Any derivation that also depends on
/// evidence is itself evidence.
///
/// Every type is a supertype of `Never` and a subtype of `object`, so `Validity(Never)` represents
/// an absent lower bound and `Validity(object)` represents an absent upper bound.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ConstraintBound<'db> {
    Validity(Type<'db>),
    Evidence(Type<'db>),
}

impl<'db> ConstraintBound<'db> {
    const fn missing_lower() -> Self {
        Self::Validity(Type::Never)
    }

    const fn missing_upper() -> Self {
        Self::Validity(Type::object())
    }

    fn ty(self) -> Type<'db> {
        match self {
            Self::Validity(ty) | Self::Evidence(ty) => ty,
        }
    }

    const fn is_missing_lower(self) -> bool {
        matches!(self, Self::Validity(Type::Never))
    }

    const fn is_missing_upper(self) -> bool {
        matches!(self, Self::Validity(Type::NominalInstance(instance)) if instance.is_object())
    }

    fn map(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        match self {
            Self::Validity(ty) => Self::Validity(f(ty)),
            Self::Evidence(ty) => Self::Evidence(f(ty)),
        }
    }

    fn with_type(self, ty: Type<'db>) -> Self {
        self.map(|_| ty)
    }

    /// Creates a bound produced by mathematically combining `lhs` and `rhs`.
    ///
    /// If one operand already equals the result, that operand alone establishes the combined
    /// bound, so its provenance is retained. Otherwise, the result is validity only if both
    /// operands are validity.
    fn from_combination(combined: Type<'db>, lhs: Self, rhs: Self) -> Self {
        match (combined == lhs.ty(), combined == rhs.ty()) {
            (true, false) => lhs.with_type(combined),
            (false, true) => rhs.with_type(combined),
            _ => match (lhs, rhs) {
                (Self::Validity(_), Self::Validity(_)) => Self::Validity(combined),
                _ => Self::Evidence(combined),
            },
        }
    }

    /// Creates a bound derived by transitivity from two constraint bounds.
    ///
    /// Unlike a union or intersection on one typevar, neither premise is redundant merely because
    /// the result has the same type as one of them. For example, deriving `int ≤ S` from
    /// `int ≤ T` and `T ≤ S` requires both premises even though the resulting bound type is still
    /// `int`.
    ///
    /// The result depends on both premises, so it is validity if both premises are validity and
    /// evidence otherwise.
    fn from_transitive_derivation(combined: Type<'db>, lhs: Self, rhs: Self) -> Self {
        match (lhs, rhs) {
            (Self::Validity(_), Self::Validity(_)) => Self::Validity(combined),
            _ => Self::Evidence(combined),
        }
    }

    /// Applies the source range's provenance to an evidence bound derived by comparing that range.
    /// Bounds not produced by the comparison retain their existing provenance.
    fn with_source_provenance(self, source: ConstraintBounds<'db>) -> Self {
        match self {
            Self::Evidence(ty) => {
                Self::from_transitive_derivation(ty, source.lower_bound(), source.upper_bound())
            }
            Self::Validity(_) => self,
        }
    }
}

/// The lower and upper bounds for a typevar on one constraint path.
///
/// Missing bounds are stored as `None`, even though this is technically redundant with
/// `Validity(Never)` or `Validity(object)`. This is purely an optimization, which makes constraint
/// equality and hashing more performant for the (common) missing-bounds cases.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct ConstraintBounds<'db> {
    pub(crate) lower: Option<ConstraintBound<'db>>,
    pub(crate) upper: Option<ConstraintBound<'db>>,
}

impl<'db> ConstraintBounds<'db> {
    pub(crate) fn new(
        lower: Option<ConstraintBound<'db>>,
        upper: Option<ConstraintBound<'db>>,
    ) -> Self {
        // Canonicalize missing lower/upper bounds so that we always store them as `None`, instead
        // of `Some(Validity(Never/object))`.
        Self {
            lower: lower.filter(|bound| !bound.is_missing_lower()),
            upper: upper.filter(|bound| !bound.is_missing_upper()),
        }
    }

    pub(crate) fn exact(ty: Type<'db>) -> Self {
        Self::new(
            Some(ConstraintBound::Evidence(ty)),
            Some(ConstraintBound::Evidence(ty)),
        )
    }

    fn lower_bound(self) -> ConstraintBound<'db> {
        self.lower.unwrap_or_else(ConstraintBound::missing_lower)
    }

    fn upper_bound(self) -> ConstraintBound<'db> {
        self.upper.unwrap_or_else(ConstraintBound::missing_upper)
    }

    fn as_equality(self) -> Option<Type<'db>> {
        let lower = self.lower?.ty();
        let upper = self.upper?.ty();
        (lower == upper).then_some(lower)
    }

    fn is_concrete(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        iter::chain(self.lower, self.upper).all(|bound| {
            let bound = bound.ty();
            !bound.has_typevar(db, env)
                && !bound.has_provisional_marker(db, env)
                && bound.bottom_materialization(db, env) == bound.top_materialization(db, env)
        })
    }
}

/// A factored conjunction of upper-bound clauses accumulated for one typevar.
///
/// Validity and evidence clauses are stored separately. Clauses may be unions, keeping
/// bounds such as `(A | B) & (C | D)` factored rather than distributing them into the DNF
/// representation used by [`Type`].
///
/// An empty validity set represents an unconstrained validity upper bound of `object`. This avoids
/// allocating or checking the intersection identity on every path. An explicit evidence bound of
/// `object` remains meaningful because evidence and validity clauses are stored separately.
///
/// Redundant clauses are retained to preserve evidence even when a validity restriction is
/// stronger. Consumers that require one effective bound can recover it with
/// [`UpperBound::as_single_bound`] without eagerly expanding intersections of unions.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct UpperBound<'db> {
    evidence: FxOrderSet<Type<'db>>,
    validity: FxOrderSet<Type<'db>>,
}

impl<'db> UpperBound<'db> {
    fn unconstrained() -> Self {
        Self::default()
    }

    /// Creates an upper bound from one explicit evidence clause.
    fn from_clause(clause: Type<'db>) -> Self {
        let mut upper = Self::default();
        upper.evidence.insert(clause);
        upper
    }

    fn is_empty(&self) -> bool {
        self.evidence.is_empty() && self.validity.is_empty()
    }

    fn iter_evidence(&self) -> impl Iterator<Item = ConstraintBound<'db>> + Clone + '_ {
        self.evidence.iter().copied().map(ConstraintBound::Evidence)
    }

    fn iter_validity(&self) -> impl Iterator<Item = ConstraintBound<'db>> + Clone + '_ {
        self.validity.iter().copied().map(ConstraintBound::Validity)
    }

    fn iter_clauses(&self) -> impl Iterator<Item = ConstraintBound<'db>> + Clone + '_ {
        iter::chain(self.iter_evidence(), self.iter_validity())
    }

    fn has_evidence(&self) -> bool {
        !self.evidence.is_empty()
    }

    /// Returns an existing upper-bound clause if every other clause is redundant with it.
    ///
    /// This preserves constrained type variables without distributing unions: expanding
    /// `S & (int | str)` into `(S & int) | (S & str)` would otherwise lose `S` as the single
    /// effective bound. Returns `None` instead of materializing intersections when no existing
    /// clause dominates the others. An unconstrained validity bound remains distinct from an
    /// explicit evidence bound of `object`.
    pub(crate) fn as_single_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        let clauses = self.iter_clauses();
        if clauses.clone().next().is_none() {
            Some(Type::object())
        } else {
            Self::single_bound_from_iterator(db, env, clauses)
        }
    }

    fn single_bound_from_iterator(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut clauses: impl Iterator<Item = ConstraintBound<'db>> + Clone,
    ) -> Option<Type<'db>> {
        let candidate = clauses
            .clone()
            .map(ConstraintBound::ty)
            .reduce(|candidate, clause| {
                if candidate.is_redundant_with(db, env, clause) {
                    candidate
                } else {
                    clause
                }
            })?;

        clauses
            .all(|clause| candidate.is_redundant_with(db, env, clause.ty()))
            .then_some(candidate)
    }

    fn add_clause(&mut self, clause: ConstraintBound<'db>) {
        if clause == ConstraintBound::missing_upper()
            || (matches!(clause, ConstraintBound::Evidence(_))
                && self.evidence.contains(&Type::Never))
        {
            return;
        }

        match clause {
            ConstraintBound::Evidence(Type::Never) => {
                self.evidence.clear();
                self.evidence.insert(Type::Never);
            }
            ConstraintBound::Validity(Type::Never) => {
                self.validity.clear();
                self.validity.insert(Type::Never);
            }
            ConstraintBound::Evidence(ty) => {
                self.evidence.insert(ty);
            }
            ConstraintBound::Validity(ty) => {
                if !self.validity.contains(&Type::Never) {
                    self.validity.insert(ty);
                }
            }
        }
    }

    fn shrink_to_fit(&mut self) {
        self.evidence.shrink_to_fit();
        self.validity.shrink_to_fit();
    }

    /// Exact conversion to an ordinary [`Type`]. This may be expensive: if any stored clause is a
    /// union, [`IntersectionType::from_elements`] converts this factored CNF representation into
    /// ty's ordinary DNF representation by distributing intersections over unions.
    fn materialize_exact(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        IntersectionType::from_elements(db, env, self.iter_clauses().map(ConstraintBound::ty))
    }

    fn has_visible_union_clause(&self) -> bool {
        self.iter_clauses().any(|clause| clause.ty().is_union())
    }

    fn is_satisfied_by(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> bool {
        self.iter_clauses()
            .all(|clause| ty.is_constraint_set_assignable_to(db, env, clause.ty()))
    }

    /// Returns the constraints under which `lower` is assignable to every stored upper clause.
    fn when_satisfied_by(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        lower: Type<'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let mut node = ALWAYS_TRUE;
        let mut source_order = None;
        for clause in self.iter_clauses() {
            let when_clause = lower.when_constraint_set_assignable_to_owned(db, env, clause.ty());
            let (clause_node, clause_source_order) = storage.load(db, env, &when_clause);
            node = node.and(storage, clause_node);
            source_order = storage.ordered_source_order(source_order, clause_source_order);
            if node == ALWAYS_FALSE {
                break;
            }
        }
        (node, source_order)
    }
}

impl ConstraintId {
    fn new<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> ConstraintId {
        Self::new_with_bounds(
            db,
            env,
            storage,
            typevar,
            Some(ConstraintBound::Evidence(lower)),
            Some(ConstraintBound::Evidence(upper)),
        )
    }

    fn new_with_bounds<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Option<ConstraintBound<'db>>,
        upper: Option<ConstraintBound<'db>>,
    ) -> ConstraintId {
        storage.intern_constraint(
            db,
            env,
            Constraint {
                typevar,
                bounds: ConstraintBounds::new(lower, upper),
            },
        )
    }
}

/// Returns the maximum constructor depth of `ty` and the maximum nesting depth of any typevar that
/// it contains.
///
/// Atomic types and bare typevars have constructor depth zero. The typevar depth is `0` if `ty`
/// does not contain any typevars.
fn max_constructor_and_typevar_depth<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> (u16, u16) {
    fn max_constructor_and_typevar_depth_impl<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        _dummy: (),
    ) -> (u16, u16) {
        struct TypeDepthVisitor<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            active: RefCell<FxHashSet<Type<'db>>>,
            current_depth: Cell<u16>,
            max_constructor_depth: Cell<u16>,
            max_typevar_depth: Cell<u16>,
        }

        impl<'db> TypeVisitor<'db> for TypeDepthVisitor<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }

            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                if ty.is_type_var() {
                    self.max_typevar_depth
                        .set(self.max_typevar_depth.get().max(self.current_depth.get()));
                    return;
                }

                let TypeKind::NonAtomic(non_atomic) = TypeKind::from(ty) else {
                    return;
                };
                if !self.active.borrow_mut().insert(ty) {
                    return;
                }

                let current_depth = self.current_depth.get();
                let nested_depth = current_depth.saturating_add(1);
                self.current_depth.set(nested_depth);
                self.max_constructor_depth
                    .set(self.max_constructor_depth.get().max(nested_depth));
                walk_non_atomic_type(db, non_atomic, self);
                self.current_depth.set(current_depth);
                self.active.borrow_mut().remove(&ty);
            }
        }

        let visitor = TypeDepthVisitor {
            env,
            active: RefCell::default(),
            current_depth: Cell::default(),
            max_constructor_depth: Cell::default(),
            max_typevar_depth: Cell::default(),
        };
        visitor.visit_type(db, ty);
        (
            visitor.max_constructor_depth.get(),
            visitor.max_typevar_depth.get(),
        )
    }

    max_constructor_and_typevar_depth_impl(db, env, ty, ())
}

impl<'db> Constraint<'db> {
    fn bound_depth(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> (u16, u16) {
        let both_bounds =
            iter::chain(self.bounds.lower, self.bounds.upper).map(ConstraintBound::ty);
        both_bounds.fold((0, 0), |(constructor_depth, typevar_depth), bound| {
            let (bound_constructor_depth, bound_typevar_depth) =
                max_constructor_and_typevar_depth(db, env, bound);
            (
                constructor_depth.max(bound_constructor_depth),
                typevar_depth.max(bound_typevar_depth),
            )
        })
    }

    /// Returns whether this constraint is produced by dropping exactly one bound from
    /// `antecedent`, without changing its typevar or retained bound.
    fn is_bound_projection_of(self, db: &'db dyn Db, antecedent: Self) -> bool {
        if !self.typevar.is_same_typevar_as(db, antecedent.typevar) {
            return false;
        }

        let keeps_lower = self.bounds.lower.is_some()
            && self.bounds.lower == antecedent.bounds.lower
            && self.bounds.upper.is_none()
            && antecedent.bounds.upper.is_some();
        let keeps_upper = self.bounds.upper.is_some()
            && self.bounds.upper == antecedent.bounds.upper
            && self.bounds.lower.is_none()
            && antecedent.bounds.lower.is_some();
        keeps_lower || keeps_upper
    }

    /// Returns a new range constraint, preserving the presence and provenance of both bounds.
    fn new_node_with_bounds(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        typevar: BoundTypeVarInstance<'db>,
        mut lower: Option<ConstraintBound<'db>>,
        mut upper: Option<ConstraintBound<'db>>,
    ) -> (NodeId, Option<SourceOrderId>) {
        if lower.is_none() && upper.is_none() {
            return (ALWAYS_TRUE, None);
        }

        // It's not useful for an upper bound to be an intersection type, or for a lower bound to
        // be a union type. Because the following equivalences hold, we can break these bounds
        // apart and create an equivalent BDD with more nodes but simpler constraints. (Fewer,
        // simpler constraints mean that our sequent maps won't grow pathologically large.)
        //
        //   T ≤ (α & β)   ⇔ (T ≤ α) ∧ (T ≤ β)
        //   T ≤ (¬α & ¬β) ⇔ (T ≤ ¬α) ∧ (T ≤ ¬β)
        //   (α | β) ≤ T   ⇔ (α ≤ T) ∧ (β ≤ T)
        if let Some(lower_bound) = lower
            && let Type::Union(lower_union) = lower_bound.ty()
        {
            let mut result = ALWAYS_TRUE;
            let mut source_order = None;
            for lower_element in lower_union.elements(db) {
                let (element_node, element_source_order) = Constraint::new_node_with_bounds(
                    db,
                    env,
                    storage,
                    typevar,
                    Some(lower_bound.with_type(*lower_element)),
                    upper,
                );
                result = result.and(storage, element_node);
                source_order = storage.ordered_source_order(source_order, element_source_order);
            }
            return (result, source_order);
        }
        // A negated type ¬α is represented as an intersection with no positive elements, and a
        // single negative element. We _don't_ want to treat that an "intersection" for the
        // purposes of simplifying upper bounds.
        if let Some(upper_bound) = upper
            && let Type::Intersection(upper_intersection) = upper_bound.ty()
            && !upper_intersection.is_simple_negation(db)
        {
            let mut result = ALWAYS_TRUE;
            let mut source_order = None;
            for upper_element in upper_intersection.iter_positive(db) {
                let (element_node, element_source_order) = Constraint::new_node_with_bounds(
                    db,
                    env,
                    storage,
                    typevar,
                    lower,
                    Some(upper_bound.with_type(upper_element)),
                );
                result = result.and(storage, element_node);
                source_order = storage.ordered_source_order(source_order, element_source_order);
            }
            for upper_element in upper_intersection.iter_negative(db) {
                let (element_node, element_source_order) = Constraint::new_node_with_bounds(
                    db,
                    env,
                    storage,
                    typevar,
                    lower,
                    Some(upper_bound.with_type(upper_element.negate(db, env))),
                );
                result = result.and(storage, element_node);
                source_order = storage.ordered_source_order(source_order, element_source_order);
            }
            return (result, source_order);
        }

        // Two identical typevars must always solve to the same type, so it is not useful to have
        // an upper or lower bound that is the typevar being constrained.
        match lower.map(ConstraintBound::ty) {
            Some(Type::TypeVar(lower_bound_typevar))
                if typevar.is_same_typevar_as(db, lower_bound_typevar) =>
            {
                lower = None;
            }
            Some(Type::Intersection(intersection))
                if intersection.positive(db).iter().any(|element| {
                    element.as_typevar().is_some_and(|element_bound_typevar| {
                        typevar.is_same_typevar_as(db, element_bound_typevar)
                    })
                }) =>
            {
                lower = None;
            }
            Some(Type::Intersection(intersection))
                if intersection.negative(db).iter().any(|element| {
                    element.as_typevar().is_some_and(|element_bound_typevar| {
                        typevar.is_same_typevar_as(db, element_bound_typevar)
                    })
                }) =>
            {
                let constraint =
                    ConstraintId::new(db, env, storage, typevar, Type::Never, Type::object());
                let (node, source_order) = Node::new_constraint(storage, constraint);
                let node = node.negate(storage);
                return (node, source_order);
            }
            _ => {}
        }
        match upper.map(ConstraintBound::ty) {
            Some(Type::TypeVar(upper_bound_typevar))
                if typevar.is_same_typevar_as(db, upper_bound_typevar) =>
            {
                upper = None;
            }
            Some(Type::Union(union))
                if union.elements(db).iter().any(|element| {
                    element.as_typevar().is_some_and(|element_bound_typevar| {
                        typevar.is_same_typevar_as(db, element_bound_typevar)
                    })
                }) =>
            {
                upper = None;
            }
            _ => {}
        }

        storage.intern_constraint_typevars(db, env, typevar, ConstraintBounds::new(lower, upper));

        // If `lower ≰ upper` for every possible assignment of typevars, then the constraint cannot
        // be satisfied, since there is no type that is both greater than `lower`, and less than
        // `upper`. We use an existential check here ("is there *some* assignment where
        // `lower ≤ upper`?") rather than a universal check, because the bounds may mention
        // typevars — e.g., `Sequence[int] ≤ A ≤ Sequence[T]` is satisfiable when `int ≤ T`.
        let effective_lower = lower.map_or(Type::Never, ConstraintBound::ty);
        let effective_upper = upper.map_or(Type::object(), ConstraintBound::ty);
        let when =
            effective_lower.when_constraint_set_assignable_to_owned(db, env, effective_upper);
        let is_never_satisfied = when.query(|_storage, when| when.is_never_satisfied(db, env));
        if is_never_satisfied {
            return (ALWAYS_FALSE, None);
        }

        // We have an (arbitrary) ordering for typevars. If the upper and/or lower bounds are
        // typevars, we have to ensure that the bounds are "later" according to that order than the
        // typevar being constrained.
        //
        // In the comments below, we use brackets to indicate which typevar is "earlier", and
        // therefore the typevar that the constraint applies to.
        match (effective_lower, effective_upper) {
            // L ≤ T ≤ L == (T ≤ [L] ≤ T)
            (Type::TypeVar(lower_typevar), Type::TypeVar(upper_typevar))
                if lower_typevar.is_same_typevar_as(db, upper_typevar) =>
            {
                let (bound, subject, lower, upper) =
                    if lower_typevar.can_be_bound_for(db, storage, typevar) {
                        (lower_typevar, typevar, lower, upper)
                    } else {
                        (typevar, lower_typevar, upper, lower)
                    };
                let bound = Type::TypeVar(bound);
                let constraint = ConstraintId::new_with_bounds(
                    db,
                    env,
                    storage,
                    subject,
                    lower.map(|lower| lower.with_type(bound)),
                    upper.map(|upper| upper.with_type(bound)),
                );
                Node::new_constraint(storage, constraint)
            }

            // L ≤ T ≤ U == ([L] ≤ T) && (T ≤ [U])
            (Type::TypeVar(lower_typevar), Type::TypeVar(upper_typevar))
                if typevar.can_be_bound_for(db, storage, lower_typevar)
                    && typevar.can_be_bound_for(db, storage, upper_typevar) =>
            {
                let lower_constraint = ConstraintId::new_with_bounds(
                    db,
                    env,
                    storage,
                    lower_typevar,
                    None,
                    lower.map(|lower| lower.with_type(Type::TypeVar(typevar))),
                );
                let (lower_node, lower_source_order) =
                    Node::new_constraint(storage, lower_constraint);
                let upper_constraint = ConstraintId::new_with_bounds(
                    db,
                    env,
                    storage,
                    upper_typevar,
                    upper.map(|upper| upper.with_type(Type::TypeVar(typevar))),
                    None,
                );
                let (upper_node, upper_source_order) =
                    Node::new_constraint(storage, upper_constraint);
                let node = lower_node.and(storage, upper_node);
                let source_order =
                    storage.ordered_source_order(lower_source_order, upper_source_order);
                (node, source_order)
            }

            // L ≤ T ≤ U == ([L] ≤ T) && ([T] ≤ U)
            (Type::TypeVar(lower_typevar), _)
                if typevar.can_be_bound_for(db, storage, lower_typevar) =>
            {
                let lower_constraint = ConstraintId::new_with_bounds(
                    db,
                    env,
                    storage,
                    lower_typevar,
                    None,
                    lower.map(|lower| lower.with_type(Type::TypeVar(typevar))),
                );
                let (lower_node, lower_source_order) =
                    Node::new_constraint(storage, lower_constraint);
                let (upper_node, upper_source_order) =
                    Constraint::new_node_with_bounds(db, env, storage, typevar, None, upper);
                let node = lower_node.and(storage, upper_node);
                let source_order =
                    storage.ordered_source_order(lower_source_order, upper_source_order);
                (node, source_order)
            }

            // L ≤ T ≤ U == (L ≤ [T]) && (T ≤ [U])
            (_, Type::TypeVar(upper_typevar))
                if typevar.can_be_bound_for(db, storage, upper_typevar) =>
            {
                let (lower_node, lower_source_order) =
                    Constraint::new_node_with_bounds(db, env, storage, typevar, lower, None);
                let upper_constraint = ConstraintId::new_with_bounds(
                    db,
                    env,
                    storage,
                    upper_typevar,
                    upper.map(|upper| upper.with_type(Type::TypeVar(typevar))),
                    None,
                );
                let (upper_node, upper_source_order) =
                    Node::new_constraint(storage, upper_constraint);
                let node = lower_node.and(storage, upper_node);
                let source_order =
                    storage.ordered_source_order(lower_source_order, upper_source_order);
                (node, source_order)
            }

            _ => {
                let constraint =
                    ConstraintId::new_with_bounds(db, env, storage, typevar, lower, upper);
                Node::new_constraint(storage, constraint)
            }
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
        std::cmp::Reverse(wobble_index(self.index()))
    }

    /// Returns whether this constraint implies another — i.e., whether every type that
    /// satisfies this constraint also satisfies `other`.
    ///
    /// This is used to avoid adding redundant implications to a sequent map.
    fn implies<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        other: Self,
    ) -> bool {
        let self_constraint = storage.constraint_data(self);
        let other_constraint = storage.constraint_data(other);
        if !self_constraint
            .typevar
            .is_same_typevar_as(db, other_constraint.typevar)
        {
            return false;
        }
        let other_lower = other_constraint.bounds.lower_bound().ty();
        let self_lower = self_constraint.bounds.lower_bound().ty();
        let self_upper = self_constraint.bounds.upper_bound().ty();
        let other_upper = other_constraint.bounds.upper_bound().ty();
        other_lower.is_constraint_set_assignable_to(db, env, self_lower)
            && self_upper.is_constraint_set_assignable_to(db, env, other_upper)
    }

    /// Returns the intersection of two range constraints, or `None` if the intersection is empty.
    fn intersect<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        other: Self,
    ) -> IntersectionResult<'db> {
        let self_constraint = storage.constraint_data(self);
        let other_constraint = storage.constraint_data(other);

        // A typevar cannot be exactly equal to two different statically eligible types under any
        // specialization. Gradual bounds cannot prove this incompatibility because the resulting
        // pair-impossibility sequent participates in transitive closure.
        if let Some(left) = self_constraint.bounds.as_equality()
            && let Some(right) = other_constraint.bounds.as_equality()
            && left.is_static_sequent_eligible(db, env)
            && right.is_static_sequent_eligible(db, env)
            && !left.can_be_constraint_set_equivalent_to(db, env, right)
        {
            return IntersectionResult::Disjoint;
        }

        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α ≤ (t₁ ∩ t₂))
        let lower = match (self_constraint.bounds.lower, other_constraint.bounds.lower) {
            (Some(left), Some(right)) => {
                let combined = UnionType::from_two_elements(db, env, left.ty(), right.ty());
                Some(ConstraintBound::from_combination(combined, left, right))
            }
            (Some(lower), None) | (None, Some(lower)) => Some(lower),
            (None, None) => None,
        };
        let mut merged_upper = UpperBound::unconstrained();
        if let Some(upper) = self_constraint.bounds.upper {
            merged_upper.add_clause(upper);
        }
        if let Some(upper) = other_constraint.bounds.upper {
            merged_upper.add_clause(upper);
        }
        let effective_lower = lower.map_or(Type::Never, ConstraintBound::ty);

        // If `lower ≰ upper` for every possible assignment of typevars, then the intersection is
        // empty, since there is no type that is both greater than `lower`, and less than `upper`.
        // We use an existential check here ("is there *some* assignment where `lower ≤ upper`?")
        // rather than a universal check ("is `lower ≤ upper` for *all* assignments?"), because the
        // bounds may mention typevars — e.g., `Sequence[int] ≤ A ≤ Sequence[T]` is satisfiable
        // when `int ≤ T`, even though it's not universally true for all `T`.
        let (when, source_order) =
            merged_upper.when_satisfied_by(db, env, storage, effective_lower);
        if when.is_never_satisfied(db, env, storage, source_order) {
            return IntersectionResult::Disjoint;
        }

        // We do not create lower bounds that are unions, or upper bounds that are factored
        // intersections, since those can be broken apart into BDDs over simpler constraints. If the
        // merged upper contains a union clause, keep any useful disjointness result from above but
        // do not try to derive a factored upper-bound constraint.
        if lower.is_some_and(|bound| bound.ty().is_union())
            || merged_upper.has_visible_union_clause()
        {
            return IntersectionResult::CannotSimplify;
        }

        let upper = if merged_upper.is_empty() {
            None
        } else {
            let effective_upper = merged_upper.materialize_exact(db, env);
            if effective_upper.is_nontrivial_intersection(db) {
                return IntersectionResult::CannotSimplify;
            }
            Some(ConstraintBound::from_combination(
                effective_upper,
                self_constraint.bounds.upper_bound(),
                other_constraint.bounds.upper_bound(),
            ))
        };

        IntersectionResult::Simplified(Constraint {
            typevar: self_constraint.typevar,
            bounds: ConstraintBounds::new(lower, upper),
        })
    }

    fn display<'db, 'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        storage: &'a ConstraintSetStorage<'db>,
    ) -> impl Display + 'a {
        self.when_true().display(db, env, storage)
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
/// TDD nodes are locally reduced when they are created. We remove duplicate nodes (via Salsa
/// interning) and collapse several sound, local redundant-edge shapes. This is not yet a fully
/// reduced TDD representation: for example, a node whose `if_true` and `if_false` branches match
/// but whose `if_uncertain` branch is non-empty would require computing a union to reduce further.
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
#[derive(Clone, Copy, Eq, Hash, PartialEq, get_size2::GetSize)]
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
    /// Creates a new BDD node, applying local TDD reductions.
    fn new(
        storage: &mut ConstraintSetStorage<'_>,
        constraint: ConstraintId,
        if_true: NodeId,
        if_false: NodeId,
    ) -> NodeId {
        Self::with_uncertain(storage, constraint, if_true, ALWAYS_FALSE, if_false)
    }

    /// Creates a new TDD node with an explicit `if_uncertain` branch, applying local reductions.
    fn with_uncertain(
        storage: &mut ConstraintSetStorage<'_>,
        constraint: ConstraintId,
        mut if_true: NodeId,
        if_uncertain: NodeId,
        mut if_false: NodeId,
    ) -> NodeId {
        debug_assert!(
            if_true
                .root_constraint(storage)
                .is_none_or(|root_constraint| {
                    root_constraint.ordering() > constraint.ordering()
                })
        );
        debug_assert!(
            if_uncertain
                .root_constraint(storage)
                .is_none_or(|root_constraint| {
                    root_constraint.ordering() > constraint.ordering()
                })
        );
        debug_assert!(
            if_false
                .root_constraint(storage)
                .is_none_or(|root_constraint| {
                    root_constraint.ordering() > constraint.ordering()
                })
        );

        if if_uncertain == ALWAYS_TRUE {
            return ALWAYS_TRUE;
        }

        // A guarded branch covered by the uncertain branch adds no satisfying assignments.
        // Keep the proof bounded and non-allocating: speculative intersections here can trigger
        // further coverage checks and expand a compact disjunction exponentially.
        if if_uncertain != ALWAYS_FALSE {
            let mut remaining_visits = 64;
            if if_true.is_covered_by(storage, if_uncertain, &mut remaining_visits) {
                if_true = ALWAYS_FALSE;
            }
            if if_false.is_covered_by(storage, if_uncertain, &mut remaining_visits) {
                if_false = ALWAYS_FALSE;
            }
        }

        if if_true == if_false {
            if if_true == ALWAYS_FALSE {
                return if_uncertain;
            }
            if if_uncertain == ALWAYS_FALSE {
                return if_true;
            }

            // TODO: A future reduction can handle this remaining `if_true == if_false` case by
            // returning `if_true ∪ if_uncertain`. That needs an `OR` computation, but only after
            // the local equality check has already engaged.
        }

        storage.intern_interior_node(InteriorNodeData {
            constraint,
            if_true,
            if_uncertain,
            if_false,
        })
    }

    /// Proves coverage using existing TDD branches, without constructing another diagram.
    ///
    /// This is deliberately incomplete: a branch must be covered by one target alternative,
    /// rather than by a union assembled from several alternatives. Exhausting the shared
    /// traversal budget also returns false, leaving the original branch unchanged.
    fn is_covered_by(
        self,
        storage: &ConstraintSetStorage<'_>,
        other: Self,
        remaining_visits: &mut usize,
    ) -> bool {
        if self == other || self == ALWAYS_FALSE || other == ALWAYS_TRUE {
            return true;
        }
        let Some(remaining) = remaining_visits.checked_sub(1) else {
            return false;
        };
        *remaining_visits = remaining;
        let (Node::Interior(left), Node::Interior(right)) = (self.node(), other.node()) else {
            return false;
        };
        let left = storage.interior_node_data(left.node());
        let right = storage.interior_node_data(right.node());
        match left.constraint.ordering().cmp(&right.constraint.ordering()) {
            Ordering::Less => {
                left.if_true.is_covered_by(storage, other, remaining_visits)
                    && left
                        .if_uncertain
                        .is_covered_by(storage, other, remaining_visits)
                    && left
                        .if_false
                        .is_covered_by(storage, other, remaining_visits)
            }
            Ordering::Equal => {
                left.if_uncertain
                    .is_covered_by(storage, other, remaining_visits)
                    && (left
                        .if_true
                        .is_covered_by(storage, right.if_true, remaining_visits)
                        || left.if_true.is_covered_by(
                            storage,
                            right.if_uncertain,
                            remaining_visits,
                        ))
                    && (left
                        .if_false
                        .is_covered_by(storage, right.if_false, remaining_visits)
                        || left.if_false.is_covered_by(
                            storage,
                            right.if_uncertain,
                            remaining_visits,
                        ))
            }
            Ordering::Greater => {
                self.is_covered_by(storage, right.if_uncertain, remaining_visits)
                    || (self.is_covered_by(storage, right.if_true, remaining_visits)
                        && self.is_covered_by(storage, right.if_false, remaining_visits))
            }
        }
    }
}

impl Node {
    /// Creates a new BDD node for an individual constraint. (The BDD will evaluate to `true` when
    /// the constraint holds, and to `false` when it does not.)
    fn new_constraint(
        storage: &mut ConstraintSetStorage<'_>,
        constraint: ConstraintId,
    ) -> (NodeId, Option<SourceOrderId>) {
        (
            NodeId::with_uncertain(storage, constraint, ALWAYS_TRUE, ALWAYS_FALSE, ALWAYS_FALSE),
            Some(storage.constraint_source_order(constraint)),
        )
    }

    /// Creates a new BDD node for a positive, negative, or unconstrained individual constraint.
    /// (For a positive constraint, this returns the same BDD node as
    /// [`new_constraint`][Self::new_constraint]. For a negative constraint, it returns the
    /// negation of that BDD node. For an unconstrained constraint, the result holds regardless
    /// of the constraint's truth value.)
    fn new_satisfied_constraint(
        storage: &mut ConstraintSetStorage<'_>,
        constraint: ConstraintAssignment,
    ) -> (NodeId, Option<SourceOrderId>) {
        let constraint_id = constraint.constraint();
        let node = match constraint {
            ConstraintAssignment::Positive(constraint) => {
                NodeId::with_uncertain(storage, constraint, ALWAYS_TRUE, ALWAYS_FALSE, ALWAYS_FALSE)
            }
            ConstraintAssignment::Negative(constraint) => {
                NodeId::with_uncertain(storage, constraint, ALWAYS_FALSE, ALWAYS_FALSE, ALWAYS_TRUE)
            }
            // The result holds regardless of the constraint's truth value, so only
            // `if_uncertain` needs to be `ALWAYS_TRUE` — `n? 0: 1: 0`. It would also be
            // correct to use `n? 1: 1: 1` (i.e., `ALWAYS_TRUE` for all outgoing edges), but
            // that would throw away some of the efficiency gains this representation gives us.
            ConstraintAssignment::Unconstrained(constraint) => {
                NodeId::with_uncertain(storage, constraint, ALWAYS_FALSE, ALWAYS_TRUE, ALWAYS_FALSE)
            }
        };
        (node, Some(storage.constraint_source_order(constraint_id)))
    }
}

impl NodeId {
    fn from_usize(value: usize) -> Self {
        assert!(value <= (SMALLEST_TERMINAL.0 as usize));
        // Safe due to the assertion immediately above:
        // `SMALLEST_TERMINAL.0` is one less than the largest possible u32
        #[expect(clippy::cast_possible_truncation)]
        Self(value as u32)
    }

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
    fn root_constraint(self, storage: &ConstraintSetStorage<'_>) -> Option<ConstraintId> {
        if self.is_terminal() {
            return None;
        }
        let interior = storage.interior_node_data(self);
        Some(interior.constraint)
    }

    /// Checks whether this BDD represents a single conjunction (of an arbitrary number of
    /// positive or negative constraints).
    fn is_single_conjunction(self, storage: &mut ConstraintSetStorage<'_>) -> bool {
        // A BDD can be viewed as an encoding of the formula's DNF representation (OR of ANDs).
        // Each path from the root node to the `always` terminals represents one of the disjoints.
        // The constraints that we encounter on the path represent the conjoints. That means that a
        // BDD can only represent a single conjunction if there is precisely one path from the root
        // node to the `always` terminal.
        //
        // We can take advantage of local reductions. We never create an interior node whose true
        // and false branches both lead to `never` while the uncertain branch also contributes
        // nothing. That means that if we ever encounter a node with both true and false branches
        // pointing to something other than `never`, that node must have at least two paths to the
        // `always` terminal.
        let mut current = self.node();
        loop {
            match current {
                Node::AlwaysTrue => return true,
                Node::AlwaysFalse => return false,
                Node::Interior(interior) => {
                    let data = storage.interior_node_data(interior.node());

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

    /// Returns whether this BDD represent the constant function `true`.
    fn is_always_satisfied<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_order: Option<SourceOrderId>,
    ) -> bool {
        match self.node() {
            Node::AlwaysTrue => true,
            Node::AlwaysFalse => false,
            Node::Interior(interior) => {
                let mut path = interior.path_assignments(db, env, storage, source_order);
                path.visit_negated(db, env, storage, self, &mut IsNeverSatisfiedVisitor)
                    .is_continue()
            }
        }
    }

    /// Returns whether this BDD represent the constant function `false`.
    fn is_never_satisfied<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_order: Option<SourceOrderId>,
    ) -> bool {
        /// Checks whether this BDD is a single conjunction, where either (a) every constraint is
        /// positive lower-bound-only, or (b) every constraint is a positive upper-bound-only. If
        /// so, `object` or `Never` respectively is a valid solution regardless of the contents of
        /// the constraints.
        fn simple_conjunction_is_satisfiable(
            storage: &mut ConstraintSetStorage<'_>,
            mut node: NodeId,
        ) -> bool {
            let mut found_lower = false;
            let mut found_upper = false;
            loop {
                match node.node() {
                    Node::AlwaysTrue => return true,
                    Node::AlwaysFalse => return false,

                    Node::Interior(_) => {
                        let interior = storage.interior_node_data(node);

                        if interior.if_false != ALWAYS_FALSE
                            || interior.if_uncertain != ALWAYS_FALSE
                        {
                            // Not a single conjunction
                            return false;
                        }

                        let constraint = storage.constraint_data(interior.constraint);
                        found_lower |= constraint.bounds.lower.is_some();
                        found_upper |= constraint.bounds.upper.is_some();
                        if found_lower && found_upper {
                            // Might be a single conjunction, but doesn't contain _only_
                            // lower-bound-only or upper-bound-only constraints
                            return false;
                        }

                        node = interior.if_true;
                    }
                }
            }
        }

        match self.node() {
            Node::AlwaysTrue => false,
            Node::AlwaysFalse => true,
            Node::Interior(interior) => {
                if let Some(result) = storage.never_satisfied_cache.get(&self) {
                    return *result;
                }

                let result = if simple_conjunction_is_satisfiable(storage, self) {
                    false
                } else {
                    let mut path = interior.path_assignments(db, env, storage, source_order);
                    path.visit(db, env, storage, self, &mut IsNeverSatisfiedVisitor)
                        .is_continue()
                };
                storage.never_satisfied_cache.insert(self, result);
                result
            }
        }
    }

    /// Returns the negation of this BDD.
    fn negate(self, storage: &mut ConstraintSetStorage<'_>) -> Self {
        match self.node() {
            Node::AlwaysTrue => ALWAYS_FALSE,
            Node::AlwaysFalse => ALWAYS_TRUE,
            Node::Interior(interior) => interior.negate(storage),
        }
    }

    /// Returns the `or` or union of two BDDs.
    fn or(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> Self {
        match (self.node(), other.node()) {
            (Node::AlwaysTrue, _) | (_, Node::AlwaysTrue) => ALWAYS_TRUE,
            (Node::AlwaysFalse, _) => other,
            (_, Node::AlwaysFalse) => self,
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.or(storage, other_interior)
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
    /// intermediate result is the "one" terminal, we can return early.
    fn tree_fold(
        builder: &ConstraintSetBuilder<'_>,
        nodes: impl Iterator<Item = (Self, Option<SourceOrderId>)>,
        zero: Self,
        one: Self,
        mut combine: impl FnMut(Self, &mut ConstraintSetStorage<'_>, Self) -> Self,
    ) -> (Self, Option<SourceOrderId>) {
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
        let mut accumulator: SmallVec<[(NodeId, Option<SourceOrderId>, u8); 8]> =
            SmallVec::default();
        for (node, source_order) in nodes {
            if node == one {
                return (node, source_order);
            }

            let (mut node, mut source_order, mut depth) = (node, source_order, 0);
            while accumulator
                .last()
                .is_some_and(|(_, _, existing)| *existing == depth)
            {
                let (existing_node, existing_source_order, _) =
                    accumulator.pop().expect("accumulator should not be empty");
                let mut storage = builder.storage.borrow_mut();
                node = combine(existing_node, &mut storage, node);
                source_order = storage.ordered_source_order(existing_source_order, source_order);
                if node == one {
                    return (node, source_order);
                }
                depth += 1;
            }
            accumulator.push((node, source_order, depth));
        }

        // At this point, we've consumed all of the iterator. The length of the accumulator will be
        // the same as the number of 1 bits in the length of the iterator. We do a final fold to
        // produce the overall result.
        let mut storage = builder.storage.borrow_mut();
        accumulator.into_iter().fold(
            (zero, None),
            |(result_node, result_source_order), (node, source_order, _)| {
                (
                    combine(result_node, &mut storage, node),
                    storage.ordered_source_order(result_source_order, source_order),
                )
            },
        )
    }

    fn distributed_or(
        builder: &ConstraintSetBuilder<'_>,
        nodes: impl Iterator<Item = (NodeId, Option<SourceOrderId>)>,
    ) -> (Self, Option<SourceOrderId>) {
        Self::tree_fold(builder, nodes, ALWAYS_FALSE, ALWAYS_TRUE, Self::or)
    }

    fn distributed_and(
        builder: &ConstraintSetBuilder<'_>,
        nodes: impl Iterator<Item = (NodeId, Option<SourceOrderId>)>,
    ) -> (Self, Option<SourceOrderId>) {
        Self::tree_fold(builder, nodes, ALWAYS_TRUE, ALWAYS_FALSE, Self::and)
    }

    /// Returns the `and` or intersection of two BDDs.
    fn and(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> Self {
        if self == other {
            return self;
        }
        match (self.node(), other.node()) {
            (Node::AlwaysFalse, _) | (_, Node::AlwaysFalse) => ALWAYS_FALSE,
            (Node::AlwaysTrue, _) => other,
            (_, Node::AlwaysTrue) => self,
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.and(storage, other_interior)
            }
        }
    }

    fn implies(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> Self {
        // p → q == ¬p ∨ q
        self.negate(storage).or(storage, other)
    }

    /// Returns a new BDD that evaluates to `true` when both input BDDs evaluate to the same
    /// result.
    fn iff(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> Self {
        // iff(a, b) = (a ∧ b) ∨ (¬a ∧ ¬b)
        let a_and_b = self.and(storage, other);
        let not_a = self.negate(storage);
        let not_b = other.negate(storage);
        let not_a_and_not_b = not_a.and(storage, not_b);
        a_and_b.or(storage, not_a_and_not_b)
    }

    /// Returns the TDD `if-then-else` of four BDDs: when `self` evaluates to `true`, it returns
    /// what `then_node` evaluates to; when `self` evaluates to `false`, it returns what
    /// `else_node` evaluates to; and `uncertain_node` is included regardless of `self`'s value.
    fn ite_uncertain(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        then_node: Self,
        uncertain_node: Self,
        else_node: Self,
    ) -> Self {
        if uncertain_node == ALWAYS_TRUE {
            return ALWAYS_TRUE;
        }

        match self.node() {
            Node::AlwaysTrue => then_node.or(storage, uncertain_node),
            Node::AlwaysFalse => else_node.or(storage, uncertain_node),
            Node::Interior(_) => {
                let interior = storage.interior_node_data(self);
                // Fast path for a bare positive constraint whose branches are still later in the
                // BDD variable ordering. This is the common case when loading an owned TDD into a
                // fresh builder, and lets us preserve an existing uncertain branch directly.
                if interior.if_true == ALWAYS_TRUE
                    && interior.if_uncertain == ALWAYS_FALSE
                    && interior.if_false == ALWAYS_FALSE
                    && then_node
                        .root_constraint(storage)
                        .is_none_or(|root| root.ordering() > interior.constraint.ordering())
                    && uncertain_node
                        .root_constraint(storage)
                        .is_none_or(|root| root.ordering() > interior.constraint.ordering())
                    && else_node
                        .root_constraint(storage)
                        .is_none_or(|root| root.ordering() > interior.constraint.ordering())
                {
                    return NodeId::with_uncertain(
                        storage,
                        interior.constraint,
                        then_node,
                        uncertain_node,
                        else_node,
                    );
                }

                // For compound conditions, or when the new builder's variable ordering requires
                // one of the branches to move above `self`, fall back to the semantic expansion:
                // `(self ∧ then_node) ∨ uncertain_node ∨ (¬self ∧ else_node)`.
                let if_true = self.and(storage, then_node);
                let if_true_or_uncertain = if_true.or(storage, uncertain_node);
                let negated = self.negate(storage);
                let if_false = negated.and(storage, else_node);
                if_true_or_uncertain.or(storage, if_false)
            }
        }
    }

    fn implies_subtype_of<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        lhs: Type<'db>,
        rhs: Type<'db>,
    ) -> (Self, Option<SourceOrderId>) {
        // When checking subtyping involving a typevar, we can turn the subtyping check into a
        // constraint (i.e, "is `T` a subtype of `int` becomes the constraint `T ≤ int`), and then
        // check when the BDD implies that constraint.
        //
        // Note that we are NOT guaranteed that `lhs` and `rhs` will always be fully static, since
        // these types are coming in from arbitrary subtyping checks that the caller might want to
        // perform. So we have to take the appropriate materialization when translating the check
        // into a constraint.
        let (constraint, constraint_source_order) = match (lhs, rhs) {
            (Type::TypeVar(bound_typevar), _) => Constraint::new_node_with_bounds(
                db,
                env,
                storage,
                bound_typevar,
                None,
                Some(ConstraintBound::Evidence(
                    rhs.bottom_materialization(db, env),
                )),
            ),
            (_, Type::TypeVar(bound_typevar)) => Constraint::new_node_with_bounds(
                db,
                env,
                storage,
                bound_typevar,
                Some(ConstraintBound::Evidence(lhs.top_materialization(db, env))),
                None,
            ),
            _ => panic!("at least one type should be a typevar"),
        };

        let node = self.implies(storage, constraint);
        (node, constraint_source_order)
    }

    /// Returns a new BDD that is the _existential abstraction_ of `self` for a set of typevars.
    /// The result will return true whenever `self` returns true for _any_ assignment of those
    /// typevars. The result will not contain any constraints that mention those typevars.
    fn exists<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        bound_typevars: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
    ) -> (Self, Option<SourceOrderId>) {
        if bound_typevars == TypeVarSet::None {
            return (self, None);
        }

        let Node::Interior(interior) = self.node() else {
            return (self, None);
        };

        let key = (self, bound_typevars, source_order);
        if let Some(result) = storage.exists_cache.get(&key) {
            return *result;
        }

        let result = interior.exists_inner(db, env, storage, bound_typevars, source_order);

        storage.exists_cache.insert(key, result);
        result
    }

    fn remove_noninferable<'db, L: SolutionLimits>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        limits: &mut L,
    ) -> ControlFlow<L::Break, (Self, Option<SourceOrderId>)> {
        match self.node() {
            Node::AlwaysTrue => ControlFlow::Continue((ALWAYS_TRUE, None)),
            Node::AlwaysFalse => ControlFlow::Continue((ALWAYS_FALSE, None)),
            Node::Interior(interior) => {
                interior.remove_noninferable(db, env, storage, inferable, source_order, limits)
            }
        }
    }

    /// Invokes a closure for each unique BDD node that appears anywhere in a BDD.
    ///
    /// This treats the BDD as a DAG and does not revisit shared subgraphs. Use this when the
    /// caller only needs to discover the set of constraints mentioned in a BDD; traversing every
    /// root-to-leaf occurrence can be exponential in the presence of shared subgraphs.
    fn for_each_unique_constraint(
        self,
        storage: &ConstraintSetStorage<'_>,
        f: &mut dyn FnMut(ConstraintId),
    ) {
        fn walk(
            node: NodeId,
            storage: &ConstraintSetStorage<'_>,
            seen: &mut FxHashSet<NodeId>,
            f: &mut dyn FnMut(ConstraintId),
        ) {
            if node.is_terminal() || !seen.insert(node) {
                return;
            }
            let interior = storage.interior_node_data(node);
            f(interior.constraint);
            walk(interior.if_true, storage, seen, f);
            walk(interior.if_uncertain, storage, seen, f);
            walk(interior.if_false, storage, seen, f);
        }

        walk(self, storage, &mut FxHashSet::default(), f);
    }

    /// Returns clauses describing all of the variable assignments that cause this BDD to evaluate
    /// to `true`. (This translates the boolean function that this BDD represents into DNF form.)
    fn satisfied_clauses(self, storage: &ConstraintSetStorage<'_>) -> SatisfiedClauses {
        struct Searcher {
            clauses: SatisfiedClauses,
            current_clause: SatisfiedClause,
        }

        impl Searcher {
            fn visit_node(&mut self, storage: &ConstraintSetStorage<'_>, node: NodeId) {
                match node.node() {
                    Node::AlwaysFalse => {}
                    Node::AlwaysTrue => self.clauses.push(self.current_clause.clone()),
                    Node::Interior(_) => {
                        let interior = storage.interior_node_data(node);
                        self.current_clause.push(interior.constraint.when_true());
                        self.visit_node(storage, interior.if_true);
                        self.current_clause.pop();
                        self.current_clause
                            .push(interior.constraint.when_unconstrained());
                        self.visit_node(storage, interior.if_uncertain);
                        self.current_clause.pop();
                        self.current_clause.push(interior.constraint.when_false());
                        self.visit_node(storage, interior.if_false);
                        self.current_clause.pop();
                    }
                }
            }
        }

        let mut searcher = Searcher {
            clauses: SatisfiedClauses::default(),
            current_clause: SatisfiedClause::default(),
        };
        searcher.visit_node(storage, self);
        searcher.clauses
    }

    fn display<'db, 'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        storage: &'a ConstraintSetStorage<'db>,
    ) -> impl Display + 'a {
        // Render the BDD directly as an unsimplified DNF formula. Each root-to-true path becomes
        // one clause, with true, uncertain, and false edges contributing positive, unconstrained,
        // and negative assignments respectively.
        std::fmt::from_fn(move |f| match self.node() {
            Node::AlwaysTrue => f.write_str("always"),
            Node::AlwaysFalse => f.write_str("never"),
            Node::Interior(_) => Display::fmt(
                &self.satisfied_clauses(storage).display(db, env, storage),
                f,
            ),
        })
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
        env: &'a ProgramEnvironment<'db>,
        storage: &'a ConstraintSetStorage<'db>,
        prefix: &'a dyn Display,
    ) -> impl Display + 'a {
        fn format_node<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            storage: &ConstraintSetStorage<'db>,
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
                    let interior = storage.interior_node_data(node);
                    write!(
                        f,
                        "<{index}> {}",
                        interior.constraint.display(db, env, storage)
                    )?;
                    // Calling display_graph recursively here causes rustc to claim that the
                    // expect(unused) up above is unfulfilled!
                    write!(f, "\n{prefix}┡━₁ ")?;
                    format_node(
                        db,
                        env,
                        storage,
                        interior.if_true,
                        &format_args!("{prefix}│   "),
                        seen,
                        f,
                    )?;
                    write!(f, "\n{prefix}├─? ")?;
                    format_node(
                        db,
                        env,
                        storage,
                        interior.if_uncertain,
                        &format_args!("{prefix}│   "),
                        seen,
                        f,
                    )?;
                    write!(f, "\n{prefix}└─₀ ")?;
                    format_node(
                        db,
                        env,
                        storage,
                        interior.if_false,
                        &format_args!("{prefix}    "),
                        seen,
                        f,
                    )?;
                    Ok(())
                }
            }
        }

        std::fmt::from_fn(move |f| {
            format_node(db, env, storage, self, prefix, &RefCell::default(), f)
        })
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

impl std::ops::Add<usize> for NodeId {
    type Output = NodeId;

    fn add(self, rhs: usize) -> Self::Output {
        NodeId::from_usize(self.index() + rhs)
    }
}

impl Idx for NodeId {
    #[inline]
    fn new(value: usize) -> Self {
        Self::from_usize(value)
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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
struct InteriorNodeData {
    constraint: ConstraintId,
    if_true: NodeId,
    if_uncertain: NodeId,
    if_false: NodeId,
}

/// Accumulates validity and evidence bounds for a single typevar on one TDD path.
///
/// Separate lower-bound unions preserve inference evidence even when a wider validity restriction
/// determines the effective minimum. Upper clauses retain their individual provenance and stay
/// factored to avoid distributing intersections over unions.
#[derive(Default)]
struct ConstraintBoundsBuilder<'db> {
    evidence_lower: FxIndexSet<Type<'db>>,
    validity_lower: FxIndexSet<Type<'db>>,
    upper: UpperBound<'db>,
    // Classify each evidence bound before aggregation: a union can otherwise make gradual and
    // static argument evidence indistinguishable from a single gradual union.
    has_gradual_evidence: bool,
    has_static_evidence: bool,
}

impl<'db> ConstraintBoundsBuilder<'db> {
    fn classify_evidence(&mut self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) {
        if ty.has_unspecialized_type_var(db, env) {
            return;
        }
        if ty.bottom_materialization(db, env) == ty.top_materialization(db, env) {
            self.has_static_evidence = true;
        } else {
            self.has_gradual_evidence = true;
        }
    }

    fn add_lower(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bound: ConstraintBound<'db>,
    ) {
        // Lower bounds are unioned. Our type representation is in DNF, so unioning a new
        // element is typically cheap (in that it does not involve a combinatorial
        // explosion from distributing the clause through an existing disjunction). So we
        // don't need to be as clever here as in `add_upper`.
        match bound {
            ConstraintBound::Evidence(ty) => {
                self.classify_evidence(db, env, ty);
                self.evidence_lower.insert(ty);
            }
            ConstraintBound::Validity(ty) if bound != ConstraintBound::missing_lower() => {
                self.validity_lower.insert(ty);
            }
            ConstraintBound::Validity(_) => {}
        }
    }

    fn add_upper(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bound: ConstraintBound<'db>,
    ) {
        if let ConstraintBound::Evidence(ty) = bound {
            self.classify_evidence(db, env, ty);
        }
        self.upper.add_clause(bound);
    }

    fn finish(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> PathBound<'db> {
        let Self {
            evidence_lower,
            validity_lower,
            mut upper,
            has_gradual_evidence,
            has_static_evidence,
        } = self;
        let evidence_lower =
            (!evidence_lower.is_empty()).then(|| UnionType::from_elements(db, env, evidence_lower));
        let validity_lower = if validity_lower.is_empty() {
            Type::Never
        } else {
            UnionType::from_elements(db, env, validity_lower)
        };
        upper.shrink_to_fit();
        PathBound {
            bound_typevar,
            evidence_lower,
            validity_lower,
            upper,
            has_only_gradual_evidence: has_gradual_evidence && !has_static_evidence,
        }
    }
}

/// The result of selecting a type for one typevar on one constraint path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathBoundSolution<'db> {
    Solved(Type<'db>),
    /// The path provides no type to infer for this variable.
    Unsolved,
    /// The bounds cannot be satisfied, so the entire path must be rejected.
    Unsatisfiable,
    /// Computing the solution exceeded the type-construction budget. A previously known type
    /// can still be used as a conservative fallback, but is not a complete solution.
    BudgetExceeded {
        fallback: Option<Type<'db>>,
    },
}

impl<'db> PathBoundSolution<'db> {
    /// Transforms a selected type without losing whether it is only a budget-exhaustion fallback.
    pub(crate) fn map(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        match self {
            Self::Solved(ty) => Self::Solved(f(ty)),
            Self::BudgetExceeded { fallback } => Self::BudgetExceeded {
                fallback: fallback.map(f),
            },
            Self::Unsolved | Self::Unsatisfiable => self,
        }
    }

    /// Returns the selected type, including a fallback when the budget was exceeded.
    /// Match the outcome directly when completeness or the reason no type was selected matters.
    pub(crate) fn as_type(self) -> Option<Type<'db>> {
        match self {
            Self::Solved(ty) => Some(ty),
            Self::Unsolved | Self::Unsatisfiable => None,
            Self::BudgetExceeded { fallback } => fallback,
        }
    }
}

/// The explicit lower and upper bounds inferred for one typevar on one BDD path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct PathBound<'db> {
    pub(crate) bound_typevar: BoundTypeVarInstance<'db>,
    pub(crate) evidence_lower: Option<Type<'db>>,
    validity_lower: Type<'db>,
    pub(crate) upper: UpperBound<'db>,
    /// Whether the path contains gradual evidence and no static evidence.
    has_only_gradual_evidence: bool,
}

impl<'db> PathBound<'db> {
    pub(crate) fn exact(bound_typevar: BoundTypeVarInstance<'db>, ty: Type<'db>) -> Self {
        Self {
            bound_typevar,
            evidence_lower: Some(ty),
            validity_lower: Type::Never,
            upper: UpperBound::from_clause(ty),
            has_only_gradual_evidence: false,
        }
    }

    fn effective_lower(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        let Some(evidence_lower) = self.evidence_lower else {
            return self.validity_lower;
        };
        if self.validity_lower.is_never() {
            return evidence_lower;
        }
        UnionType::from_elements(db, env, [evidence_lower, self.validity_lower])
    }

    fn variance(&self) -> TypeVarVariance {
        match (self.evidence_lower.is_some(), self.has_upper_evidence()) {
            (false, true) => TypeVarVariance::Covariant,
            (true, false) => TypeVarVariance::Contravariant,
            (true, true) => TypeVarVariance::Invariant,
            (false, false) => TypeVarVariance::Bivariant,
        }
    }

    pub(crate) fn has_upper_evidence(&self) -> bool {
        self.upper.has_evidence()
    }

    /// Restricts the range of a gradual solution by the upper bounds inferred for this constraint.
    /// Returns `None` if constructing an intersection exceeds the solution budget.
    fn restrict_gradual_solution(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        solution: Type<'db>,
    ) -> Option<Type<'db>> {
        if self.evidence_lower.is_none()
            || self.effective_lower(db, env) != solution
            || !self.has_upper_evidence()
            || solution.bottom_materialization(db, env) == solution.top_materialization(db, env)
        {
            return Some(solution);
        }

        // Unresolved type-variable relationships must not escape into the specialization.
        if solution.has_typevar(db, env) || solution.has_unspecialized_type_var(db, env) {
            return Some(solution);
        }

        // `Divergent` is not safely reflexive, so we cannot intersect identical bounds.
        if UpperBound::single_bound_from_iterator(db, env, self.upper.iter_evidence())
            == Some(solution)
        {
            return Some(solution);
        }

        // Gradual upper bounds are top-materialized, as the lower bound is already gradual.
        let materialize_upper = |bound: Type<'db>| {
            (!bound.has_typevar(db, env) && !bound.has_unspecialized_type_var(db, env))
                .then(|| bound.top_materialization(db, env))
                .filter(|bound| !bound.is_object())
        };

        let declared_upper = match self.bound_typevar.typevar(db).bound_or_constraints(db, env) {
            // Constrained type variables select solutions from their own set of constraints.
            Some(TypeVarBoundOrConstraints::Constraints(_)) => return Some(solution),
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => materialize_upper(bound),
            _ => None,
        };

        let mut upper_bounds = self
            .upper
            .iter_evidence()
            .map(ConstraintBound::ty)
            .filter_map(materialize_upper);
        let Some(first_upper) = upper_bounds.next() else {
            return Some(solution);
        };

        let upper_bound = IntersectionType::bounded_from_elements(
            db,
            env,
            iter::once(first_upper)
                .chain(upper_bounds)
                .chain(declared_upper),
        )?;

        // Restrict the range of each gradual solution by the upper bound of this constraint.
        let restrict_gradual = |element: Type<'db>| {
            if element.bottom_materialization(db, env) == element.top_materialization(db, env) {
                Some(element)
            } else {
                IntersectionType::bounded_from_elements(db, env, [upper_bound, element])
            }
        };

        match solution {
            Type::Union(union) => union.try_map(db, env, |element| restrict_gradual(*element)),
            _ => restrict_gradual(solution),
        }
    }
}

impl<'db> Type<'db> {
    /// Calculates the [`PathBounds`] that represent the valid solutions for when `self` is
    /// constraint-set assignable to `target`.
    pub(crate) fn assignable_solutions_with_inferable(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> &'db PathBounds<'db> {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _, _, _, _| PathBounds::Unsatisfiable,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn assignable_solutions_impl<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            source: Type<'db>,
            target: Type<'db>,
            inferable: TypeVarSet<'db>,
        ) -> PathBounds<'db> {
            let env = &ProgramEnvironment::from_program(program);
            let when = source.when_constraint_set_assignable_to_owned(db, env, target);
            when.query(|builder, when| {
                let mut storage = builder.storage.borrow_mut();
                PathBounds::compute(
                    db,
                    env,
                    &mut storage,
                    when.node,
                    inferable,
                    when.source_order,
                )
            })
        }

        let program = env.program(db);
        assignable_solutions_impl(db, program, self, target, inferable)
    }
}

#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| true,
    heap_size = get_size2::GetSize::get_heap_size
)]
fn is_possibly_constraint_set_assignable<'db>(db: &'db dyn Db, types: TypePair<'db>) -> bool {
    let program = types.program(db);
    let env = &ProgramEnvironment::from_program(program);
    types
        .first(db)
        .when_constraint_set_assignable_to_owned(db, env, types.second(db))
        .query(|_storage, when| !when.is_never_satisfied(db, env))
}

/// Per-path bounds for all typevars. Each element is the set of typevar bounds for one BDD path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum PathBounds<'db> {
    Unsatisfiable,
    Unconstrained,
    Constrained(Box<[Box<[PathBound<'db>]>]>),
}

/// Limits shared by the preprocessing and collection walks used to extract solutions.
trait SolutionLimits {
    type Break;

    fn visit_node(&mut self) -> ControlFlow<Self::Break> {
        ControlFlow::Continue(())
    }

    fn satisfied_path(&mut self) -> ControlFlow<Self::Break> {
        ControlFlow::Continue(())
    }
}

struct UnboundedSolutionLimits;

impl SolutionLimits for UnboundedSolutionLimits {
    type Break = Infallible;
}

struct BoundedSolutionLimits {
    remaining_paths: usize,
    remaining_visits: usize,
}

impl SolutionLimits for BoundedSolutionLimits {
    type Break = ProjectionError;

    fn visit_node(&mut self) -> ControlFlow<Self::Break> {
        let Some(remaining) = self.remaining_visits.checked_sub(1) else {
            return ControlFlow::Break(ProjectionError::TraversalBudgetExceeded);
        };
        self.remaining_visits = remaining;
        ControlFlow::Continue(())
    }

    fn satisfied_path(&mut self) -> ControlFlow<Self::Break> {
        let Some(remaining) = self.remaining_paths.checked_sub(1) else {
            return ControlFlow::Break(ProjectionError::PathBudgetExceeded);
        };
        self.remaining_paths = remaining;
        ControlFlow::Continue(())
    }
}

impl<'db> PathBounds<'db> {
    /// Computes sorted BDD paths and accumulates per-typevar lower/upper bounds for each path.
    ///
    /// Returns a list of paths, where each path contains the explicit lower/upper bounds for each
    /// typevar that appears in the path's constraints.
    fn compute(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        inferable: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
    ) -> Self {
        let ControlFlow::Continue(result) = Self::compute_with_limits(
            db,
            env,
            storage,
            node,
            inferable,
            source_order,
            &mut UnboundedSolutionLimits,
        );
        result
    }

    /// Computes complete path bounds within limits shared by preprocessing and collection.
    ///
    /// Visits include the concrete-conjunction fast path and both BDD walks. The path limit
    /// counts materialized constrained paths; an unconstrained or unsatisfiable result needs no
    /// path allowance. No partially collected family is returned when either limit is exhausted.
    fn compute_bounded(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        inferable: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        budget: SolutionBudget,
    ) -> Result<Self, ProjectionError> {
        let mut limits = BoundedSolutionLimits {
            remaining_paths: budget.paths,
            remaining_visits: budget.visits,
        };
        match Self::compute_with_limits(
            db,
            env,
            storage,
            node,
            inferable,
            source_order,
            &mut limits,
        ) {
            ControlFlow::Continue(result) => Ok(result),
            ControlFlow::Break(error) => Err(error),
        }
    }

    fn compute_with_limits<L: SolutionLimits>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        node: NodeId,
        inferable: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        limits: &mut L,
    ) -> ControlFlow<L::Break, Self> {
        let mut source_orders = storage.calculate_source_orders(source_order);
        if let Some(path_bounds) = Self::compute_simple_bound_conjunction(
            db,
            env,
            storage,
            &source_orders,
            node,
            inferable,
            limits,
        )? {
            return ControlFlow::Continue(path_bounds);
        }

        let (node, derived_source_order) =
            node.remove_noninferable(db, env, storage, inferable, source_order, limits)?;
        source_orders.extend(storage.calculate_source_orders(derived_source_order));
        let interior = match node.node() {
            Node::AlwaysTrue => {
                limits.visit_node()?;
                return ControlFlow::Continue(PathBounds::Unconstrained);
            }
            Node::AlwaysFalse => {
                limits.visit_node()?;
                return ControlFlow::Continue(PathBounds::Unsatisfiable);
            }
            Node::Interior(interior) => interior,
        };

        let mut walker = SolutionWalker::new(source_orders);
        // Sequent discovery must also happen in source order. Sorting the collected paths is
        // too late: sequent pairs are not commutative, and TDD traversal order can otherwise
        // discard gradual evidence before solution extraction.
        let path_source_order = storage.ordered_source_order(source_order, derived_source_order);
        let mut path = interior.path_assignments(db, env, storage, path_source_order);
        walker.visit_node(db, env, storage, &mut path, node, limits)?;
        ControlFlow::Continue(walker.finish(db, env, storage))
    }

    /// Accumulates a conjunction of concrete bound constraints without constructing a
    /// [`PathAssignments`] or its sequent map.
    ///
    /// There are no relationships to derive between these constraints, as the upper and lower
    /// bounds do not contain typevars. The normal solution-selection logic still validates each
    /// accumulated bound against the typevar's declared bound or constraints.
    fn compute_simple_bound_conjunction<L: SolutionLimits>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_orders: &FxIndexSet<ConstraintId>,
        node: NodeId,
        inferable: TypeVarSet<'db>,
        limits: &mut L,
    ) -> ControlFlow<L::Break, Option<Self>> {
        let mut constraints = Vec::default();
        let mut current = node;
        loop {
            limits.visit_node()?;
            match current.node() {
                Node::AlwaysTrue => {
                    if constraints.is_empty() {
                        return ControlFlow::Continue(Some(PathBounds::Unconstrained));
                    }
                    limits.satisfied_path()?;
                    break;
                }
                Node::AlwaysFalse => {
                    return ControlFlow::Continue(
                        constraints.is_empty().then_some(PathBounds::Unsatisfiable),
                    );
                }
                Node::Interior(_) => {
                    let interior = storage.interior_node_data(current);
                    if interior.if_uncertain != ALWAYS_FALSE || interior.if_false != ALWAYS_FALSE {
                        return ControlFlow::Continue(None);
                    }

                    let constraint = storage.constraint_data(interior.constraint);
                    if !constraint.typevar.is_inferable(db, inferable) {
                        return ControlFlow::Continue(None);
                    }

                    let mut bounds = iter::chain(constraint.bounds.lower, constraint.bounds.upper)
                        .map(ConstraintBound::ty);
                    if bounds.any(|bound| {
                        bound.has_typevar(db, env) || bound.has_provisional_marker(db, env)
                    }) {
                        return ControlFlow::Continue(None);
                    }

                    current = interior.if_true;
                    constraints.push((
                        constraint.typevar,
                        constraint.bounds,
                        source_orders
                            .get_index_of(&interior.constraint)
                            .expect("every TDD constraint should have a source order"),
                    ));
                }
            }
        }

        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, ConstraintBoundsBuilder<'db>> =
            FxIndexMap::default();
        constraints.sort_by_key(|(_, _, source_order)| *source_order);
        for (typevar, constraint, _) in constraints {
            let bounds = mappings.entry(typevar).or_default();
            if let Some(lower) = constraint.lower {
                bounds.add_lower(db, env, lower);
            }
            if let Some(upper) = constraint.upper {
                bounds.add_upper(db, env, upper);
            }
        }

        let path = mappings
            .drain(..)
            .map(|(bound_typevar, bounds)| bounds.finish(db, env, bound_typevar))
            .collect();
        ControlFlow::Continue(Some(PathBounds::Constrained(Box::new([path]))))
    }

    pub(crate) fn solve(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Solutions<'db> {
        self.solve_with(|_variance, path_bound| {
            PathBounds::default_solve(db, env, builder, path_bound)
        })
    }

    /// Solves each path by applying a per-typevar solver function, collecting retained solutions.
    ///
    /// A genuinely unsolved variable does not invalidate a path. Budget exhaustion also retains
    /// the path's available bindings, but marks the resulting path family as incomplete.
    pub(crate) fn solve_with(
        &self,
        choose: impl FnMut(TypeVarVariance, &PathBound<'db>) -> PathBoundSolution<'db>,
    ) -> Solutions<'db> {
        let Ok(solutions) = self.try_solve_with(choose, |_| Ok::<(), Infallible>(()));
        solutions
    }

    /// Checks each retained solution before collecting it or solving the next path.
    fn try_solve_with<E>(
        &self,
        mut choose: impl FnMut(TypeVarVariance, &PathBound<'db>) -> PathBoundSolution<'db>,
        mut check_solution: impl FnMut(&Solution<'db>) -> Result<(), E>,
    ) -> Result<Solutions<'db>, E> {
        let paths = match self {
            PathBounds::Unsatisfiable => return Ok(Solutions::Unsatisfiable),
            PathBounds::Unconstrained => return Ok(Solutions::Unconstrained),
            PathBounds::Constrained(paths) => paths,
        };

        let mut solutions = Vec::with_capacity(paths.len());
        let mut exceeded_budget = false;
        for path in paths {
            let Some((solution, path_exceeded_budget)) = Self::solve_path_with(path, &mut choose)
            else {
                continue;
            };
            check_solution(&solution)?;
            exceeded_budget |= path_exceeded_budget;
            solutions.push(solution);
        }

        if solutions.is_empty() {
            return Ok(Solutions::Unsatisfiable);
        }
        Ok(Solutions::Constrained(if exceeded_budget {
            SolutionPaths::BudgetExceeded(solutions)
        } else {
            SolutionPaths::Complete(solutions)
        }))
    }

    /// Solves one complete path, retaining whether any of its bindings used a fallback.
    /// A later unsatisfiable bound rejects the path even if an earlier bound exhausted its budget.
    fn solve_path_with(
        path: &[PathBound<'db>],
        choose: &mut impl FnMut(TypeVarVariance, &PathBound<'db>) -> PathBoundSolution<'db>,
    ) -> Option<(Solution<'db>, bool)> {
        let mut solution = Vec::with_capacity(path.len());
        let mut exceeded_budget = false;
        for path_bound in path {
            let ty = match choose(path_bound.variance(), path_bound) {
                PathBoundSolution::Solved(ty) => Some(ty),
                PathBoundSolution::Unsolved => None,
                PathBoundSolution::Unsatisfiable => return None,
                PathBoundSolution::BudgetExceeded { fallback } => {
                    exceeded_budget = true;
                    fallback
                }
            };
            if let Some(ty) = ty {
                solution.push(TypeVarSolution {
                    bound_typevar: path_bound.bound_typevar,
                    solution: ty,
                });
            }
        }
        Some((solution, exceeded_budget))
    }

    /// The default solution selection logic for a single typevar on a single BDD path.
    ///
    /// Given the explicit lower and upper bounds for a typevar, selects the solution type.
    /// Missing bounds are materialized to their logical defaults only for satisfiability checks;
    /// they are not selected as inferred solutions.
    pub(crate) fn default_solve(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
        path_bound: &PathBound<'db>,
    ) -> PathBoundSolution<'db> {
        let preliminary = Self::preliminary_solve(db, env, builder, path_bound);
        let PathBoundSolution::Solved(solution) = preliminary else {
            return preliminary;
        };

        let Some(restricted) = path_bound.restrict_gradual_solution(db, env, solution) else {
            return PathBoundSolution::BudgetExceeded {
                fallback: Some(solution),
            };
        };

        // An empty gradual range makes the constraint path unsatisfiable.
        if restricted.is_never() && !solution.is_never() {
            return PathBoundSolution::Unsatisfiable;
        }

        PathBoundSolution::Solved(restricted)
    }

    /// Selects a preliminary solution to use as type context during generic call inference.
    ///
    /// Unlike [`Self::default_solve`], the range of a gradual solution is not restricted by inferred
    /// upper bounds, as the inferred types may not have stabilized yet.
    pub(crate) fn preliminary_solve(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
        path_bound: &PathBound<'db>,
    ) -> PathBoundSolution<'db> {
        // Choose a solution type that satisfies the constraints on this path, as well as any upper
        // bound or constraints of the typevar itself.
        // TODO: Handle the upper bound/constraints by conjoining them with the constraint set
        // before solving.

        let bound_typevar = path_bound.bound_typevar;
        let lower = path_bound.effective_lower(db, env);

        match bound_typevar
            .typevar(db)
            .require_bound_or_constraints(db, env)
        {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                let declared_upper = bound.top_materialization(db, env);

                // Prefer the lower bound (often the concrete actual type seen) over the
                // upper bound (which may include TypeVar bounds/constraints). The upper bound
                // should only be used as a fallback when no concrete type was inferred.
                if path_bound.evidence_lower.is_some() {
                    if !path_bound.upper.is_satisfied_by(db, env, lower) {
                        let mut storage = builder.storage.borrow_mut();
                        let (when_upper, source_order) =
                            path_bound
                                .upper
                                .when_satisfied_by(db, env, &mut storage, lower);
                        if when_upper.is_never_satisfied(db, env, &mut storage, source_order) {
                            // This path does not satisfy the accumulated upper bound, and is
                            // therefore not a valid specialization.
                            return PathBoundSolution::Unsatisfiable;
                        }
                    }

                    if !is_possibly_constraint_set_assignable(
                        db,
                        TypePair::new(db, env.program(db), lower, declared_upper),
                    ) {
                        // This path does not satisfy the typevar's declared upper bound, and is
                        // therefore not a valid specialization.
                        return PathBoundSolution::Unsatisfiable;
                    }

                    return PathBoundSolution::Solved(lower);
                }

                if path_bound.has_upper_evidence() {
                    return IntersectionType::bounded_from_elements(
                        db,
                        env,
                        iter::chain(
                            path_bound.upper.iter_clauses().map(ConstraintBound::ty),
                            [declared_upper],
                        ),
                    )
                    .map_or(
                        PathBoundSolution::BudgetExceeded { fallback: None },
                        PathBoundSolution::Solved,
                    );
                }

                PathBoundSolution::Unsolved
            }

            TypeVarBoundOrConstraints::Constraints(constraints) => {
                // For a constrained typevar, the solution for this path must satisfy at least one
                // of the constraints. If it doesn't, then this path isn't a valid solution. If it
                // satisfies exactly one constraint, that constraint is the solution.
                //
                // If the path satisfies more than one constraint, we behave differently depending
                // on whether the path solution is gradual or not. If it's gradual, then the path
                // solution has _materializations_ that satisfy more than one constraint, and we
                // use the (gradual) path solution as our result, so that we aren't arbitrarily
                // preferring one materialization over the others.
                //
                // If the path solution is fully static, and satisfies more than one constraint, we
                // choose the "tightest" constraint as the solution.
                //
                // TODO: The way we are handling constrained typevars here breaks our assumption
                // that each solution is represented by a single path in the BDD. Moreover, the
                // logic here for disambiguating multiple solutions is different than the logic up
                // in `SpecializationBuilder` that disambiguates solutions that come from multiple
                // BDD paths. Ideally we would handle multiple solutions the same way in both
                // places. The best way to do that is addressed by the TODO comment at the top of
                // this method: we should handle typevar constraints by conjoining them into the
                // constraint set before solving. Because typevar constraints would be modeled by
                // an OR across the constraints, that would "break apart" this BDD path into
                // separate paths, one for each satisfied typevar constraint. And then we would
                // have to move this disambiguation logic up to the code that combines/chooses
                // between solutions from multiple paths.

                // Filter out the typevar constraints that aren't satisfied by this path. If
                // multiple constraints are satisfied, track which one is "tightest".
                let mut compatible_constraint = None;
                let mut multiple_compatible_constraints = false;
                let is_tighter_solution = |candidate: Type<'db>, current_best: Type<'db>| {
                    // Lower-bound evidence asks for the narrowest compatible declared constraint
                    // above the lower bound. With only upper-bound evidence, ask for the widest
                    // compatible declared constraint below the upper bound. If the candidates are
                    // assignable in both directions, prefer a fully static constraint over a
                    // gradual one. Otherwise, keep the current best to preserve the TypeVar's
                    // declared constraint order.
                    let candidate_assignable_to_best =
                        candidate.is_assignable_to(db, env, current_best);
                    let best_assignable_to_candidate =
                        current_best.is_assignable_to(db, env, candidate);

                    if candidate_assignable_to_best != best_assignable_to_candidate {
                        if path_bound.evidence_lower.is_some() {
                            candidate_assignable_to_best
                        } else {
                            best_assignable_to_candidate
                        }
                    } else if candidate_assignable_to_best {
                        let candidate_is_static = candidate.bottom_materialization(db, env)
                            == candidate.top_materialization(db, env);
                        let best_is_static = current_best.bottom_materialization(db, env)
                            == current_best.top_materialization(db, env);
                        candidate_is_static && !best_is_static
                    } else {
                        false
                    }
                };

                for constraint in constraints.elements(db).iter().copied() {
                    let constraint_lower = constraint.bottom_materialization(db, env);
                    let constraint_upper = constraint.top_materialization(db, env);
                    // A gradual constraint can choose any materialization that satisfies this
                    // path. Its top materialization is the most permissive target for lower-bound
                    // evidence, while its bottom materialization is the most permissive source
                    // for upper-bound evidence.
                    let when_lower =
                        lower.when_constraint_set_assignable_to_owned(db, env, constraint_upper);
                    let mut storage = builder.storage.borrow_mut();
                    let (when_upper, upper_source_order) =
                        path_bound
                            .upper
                            .when_satisfied_by(db, env, &mut storage, constraint_lower);
                    let (when_lower, lower_source_order) = storage.load(db, env, &when_lower);
                    let when = when_lower.and(&mut storage, when_upper);
                    let source_order =
                        storage.ordered_source_order(lower_source_order, upper_source_order);
                    if when.is_never_satisfied(db, env, &mut storage, source_order) {
                        continue;
                    }

                    if compatible_constraint.is_some() {
                        multiple_compatible_constraints = true;
                    }
                    if compatible_constraint
                        .is_none_or(|best| is_tighter_solution(constraint, best))
                    {
                        compatible_constraint = Some(constraint);
                    }
                }

                let Some(compatible_constraint) = compatible_constraint else {
                    // This path does not satisfy any of the constraints, and is therefore not a
                    // valid specialization.
                    return PathBoundSolution::Unsatisfiable;
                };

                if let (ty @ Type::TypeVar(_), _) | (_, Some(ty @ Type::TypeVar(_))) = (
                    path_bound.effective_lower(db, env),
                    path_bound.upper.as_single_bound(db, env),
                ) {
                    // This path relates two TypeVars, such as passing `S` to a parameter typed as
                    // `T: (int, str)`. The compatibility check above has verified that at least
                    // one of `T`'s declared constraints can satisfy the path, but choosing a
                    // concrete constraint here would break the relationship between `T` and `S`.
                    // Keep that relationship as the solution instead.
                    return PathBoundSolution::Solved(ty);
                }

                // See above: If the path solution satisfies exactly one constraint, use that
                // constraint as our solution. (Even if the path solution is gradual: if we are
                // checking `list[Any]` against `T: (int, list[int])`, we select `T = list[int]`.)
                //
                // If the path solution satisfies multiple constraints, then we use path solution
                // as the result if it's gradual. (Checking `Any` against `T: (int, str)` selects
                // `T = Any`) If the path solution is fully static, we choose the "tightest"
                // constraint. (Checking `int` against `T: (int, int | str)` selects `T = int`.)
                if multiple_compatible_constraints && path_bound.has_only_gradual_evidence {
                    if path_bound.evidence_lower.is_some() {
                        PathBoundSolution::Solved(path_bound.effective_lower(db, env))
                    } else if path_bound.has_upper_evidence() {
                        IntersectionType::bounded_from_elements(
                            db,
                            env,
                            path_bound.upper.iter_clauses().map(ConstraintBound::ty),
                        )
                        .map_or(
                            PathBoundSolution::BudgetExceeded { fallback: None },
                            PathBoundSolution::Solved,
                        )
                    } else {
                        PathBoundSolution::Unsolved
                    }
                } else {
                    PathBoundSolution::Solved(compatible_constraint)
                }
            }
        }
    }
}

impl InteriorNode {
    fn node(self) -> NodeId {
        self.0
    }

    fn negate(self, storage: &mut ConstraintSetStorage<'_>) -> NodeId {
        let key = self.node();
        if let Some(result) = storage.negate_cache.get(&key) {
            return *result;
        }

        // negate(n ? C : U : D) = n ? negate(or(C, U)) : 0 : negate(or(D, U))
        //
        // The uncertain branch U is absorbed into C and D via union before negation. The result's
        // uncertain branch is always zero. When U = 0 (the common case), this degenerates to the
        // standard binary BDD leaf-swap: n ? negate(C) : 0 : negate(D).
        let interior = storage.interior_node_data(self.node());
        let not_true = interior.if_true.negate(storage);
        let not_uncertain = interior.if_uncertain.negate(storage);
        let not_false = interior.if_false.negate(storage);
        let if_true = not_true.and(storage, not_uncertain);
        let if_false = not_false.and(storage, not_uncertain);
        let result = NodeId::new(storage, interior.constraint, if_true, if_false);

        storage.negate_cache.insert(key, result);
        result
    }

    fn or(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> NodeId {
        let key = (self.node(), other.node());
        if let Some(result) = storage.or_cache.get(&key) {
            return *result;
        }

        let self_interior = storage.interior_node_data(self.node());
        let self_ordering = self_interior.constraint.ordering();
        let other_interior = storage.interior_node_data(other.node());
        let other_ordering = other_interior.constraint.ordering();
        let result = match self_ordering.cmp(&other_ordering) {
            Ordering::Equal => {
                let if_true = self_interior.if_true.or(storage, other_interior.if_true);
                let if_uncertain = self_interior
                    .if_uncertain
                    .or(storage, other_interior.if_uncertain);
                let if_false = self_interior.if_false.or(storage, other_interior.if_false);
                NodeId::with_uncertain(
                    storage,
                    self_interior.constraint,
                    if_true,
                    if_uncertain,
                    if_false,
                )
            }
            // This is from Frisch's original description of TDDs. If self < other, we check self
            // first. Instead of distributing other into the if_true and if_false branches, we
            // "park" it in the if_uncertain branch. That causes us to only evaluate other "lazily"
            // when needed.
            Ordering::Less => {
                let if_uncertain = self_interior.if_uncertain.or(storage, other.node());
                NodeId::with_uncertain(
                    storage,
                    self_interior.constraint,
                    self_interior.if_true,
                    if_uncertain,
                    self_interior.if_false,
                )
            }
            // Ditto above but for the other variable ordering
            Ordering::Greater => {
                let if_uncertain = self.node().or(storage, other_interior.if_uncertain);
                NodeId::with_uncertain(
                    storage,
                    other_interior.constraint,
                    other_interior.if_true,
                    if_uncertain,
                    other_interior.if_false,
                )
            }
        };

        storage.or_cache.insert(key, result);
        result
    }

    fn and(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> NodeId {
        let key = (self.node(), other.node());
        if let Some(result) = storage.and_cache.get(&key) {
            return *result;
        }

        let self_interior = storage.interior_node_data(self.node());
        let self_ordering = self_interior.constraint.ordering();
        let other_interior = storage.interior_node_data(other.node());
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
                let other_if_true = other_interior
                    .if_true
                    .or(storage, other_interior.if_uncertain);
                let true_from_true = self_interior.if_true.and(storage, other_if_true);
                let true_from_uncertain = self_interior
                    .if_uncertain
                    .and(storage, other_interior.if_true);
                let if_true = true_from_true.or(storage, true_from_uncertain);
                let if_uncertain = self_interior
                    .if_uncertain
                    .and(storage, other_interior.if_uncertain);
                let other_if_false = other_interior
                    .if_uncertain
                    .or(storage, other_interior.if_false);
                let false_from_false = self_interior.if_false.and(storage, other_if_false);
                let false_from_uncertain = self_interior
                    .if_uncertain
                    .and(storage, other_interior.if_false);
                let if_false = false_from_false.or(storage, false_from_uncertain);
                NodeId::with_uncertain(
                    storage,
                    self_interior.constraint,
                    if_true,
                    if_uncertain,
                    if_false,
                )
            }
            Ordering::Less => {
                let if_true = self_interior.if_true.and(storage, other.node());
                let if_uncertain = self_interior.if_uncertain.and(storage, other.node());
                let if_false = self_interior.if_false.and(storage, other.node());
                NodeId::with_uncertain(
                    storage,
                    self_interior.constraint,
                    if_true,
                    if_uncertain,
                    if_false,
                )
            }
            Ordering::Greater => {
                let if_true = self.node().and(storage, other_interior.if_true);
                let if_uncertain = self.node().and(storage, other_interior.if_uncertain);
                let if_false = self.node().and(storage, other_interior.if_false);
                NodeId::with_uncertain(
                    storage,
                    other_interior.constraint,
                    if_true,
                    if_uncertain,
                    if_false,
                )
            }
        };

        storage.and_cache.insert(key, result);
        result
    }

    fn exists_inner<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        bound_typevars: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let ControlFlow::Continue(result) = self.abstract_inner(
            db,
            env,
            storage,
            source_order,
            &mut UnboundedSolutionLimits,
            // Remove any node that constrains one of `bound_typevars`, or that has a lower/upper
            // bound that mentions one of them. Removed constraints are still added to `path`, so
            // the sequent map can propagate any derived constraints that do not mention the
            // quantified typevars.
            &mut |storage: &ConstraintSetStorage<'_>, constraint| {
                storage.constraint_mentions_typevars(db, constraint, bound_typevars)
            },
        );
        result
    }

    fn remove_noninferable<'db, L: SolutionLimits>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        limits: &mut L,
    ) -> ControlFlow<L::Break, (NodeId, Option<SourceOrderId>)> {
        let is_bare_inferable_typevar = |bound: Option<ConstraintBound<'_>>| {
            bound.is_some_and(|bound| {
                matches!(
                    bound,
                    ConstraintBound::Evidence(Type::TypeVar(bound_typevar))
                        if bound_typevar.is_inferable(db, inferable)
                )
            })
        };
        self.abstract_inner(
            db,
            env,
            storage,
            source_order,
            limits,
            // We only want to keep constraints on inferable typevars. If the constraint's typevar
            // is itself inferable, we keep it. We also need to keep some constraints in
            // non-inferable typevars, if an evidence bound is a bare inferable typevar. This
            // ensures that our quantification logic does not depend on typevar ordering.
            //
            // For example, `I ≤ N` (where I is inferable and N is non-inferable) could be encoded
            // either as `Never ≤ I ≤ N` or `I ≤ N ≤ object`, depending on typevar ordering. If we
            // only checked the inferability of the constrained typevar, we would keep the first
            // encoding but remove the second.
            &mut |storage: &ConstraintSetStorage<'_>, constraint| {
                let constraint = storage.constraint_data(constraint);
                !constraint.typevar.is_inferable(db, inferable)
                    && !is_bare_inferable_typevar(constraint.bounds.lower)
                    && !is_bare_inferable_typevar(constraint.bounds.upper)
            },
        )
    }

    fn abstract_inner<'db, F, L>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_order: Option<SourceOrderId>,
        limits: &mut L,
        should_remove: F,
    ) -> ControlFlow<L::Break, (NodeId, Option<SourceOrderId>)>
    where
        F: FnMut(&ConstraintSetStorage<'_>, ConstraintId) -> bool,
        L: SolutionLimits,
    {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Disposition {
            Keep,
            Remove,
        }

        struct AbstractVisitor<'a, F, L> {
            should_remove: F,
            limits: &'a mut L,
        }

        impl<F, L> PathVisitor for AbstractVisitor<'_, F, L>
        where
            F: FnMut(&ConstraintSetStorage<'_>, ConstraintId) -> bool,
            L: SolutionLimits,
        {
            type Result = (NodeId, Option<SourceOrderId>);
            type Interior = (Disposition, ConstraintId);
            type Break = L::Break;

            fn visit_node(&mut self) -> ControlFlow<Self::Break> {
                self.limits.visit_node()
            }

            fn visit_satisfied<'db>(
                &mut self,
                _db: &'db dyn Db,
                _storage: &mut ConstraintSetStorage<'db>,
                _path: &PathAssignments,
            ) -> ControlFlow<Self::Break, Self::Result> {
                ControlFlow::Continue((ALWAYS_TRUE, None))
            }

            fn visit_unsatisfied<'db>(
                &mut self,
                _db: &'db dyn Db,
                _storage: &mut ConstraintSetStorage<'db>,
                _path: &PathAssignments,
            ) -> ControlFlow<Self::Break, Self::Result> {
                ControlFlow::Continue((ALWAYS_FALSE, None))
            }

            fn visit_impossible<'db>(
                &mut self,
                _db: &'db dyn Db,
                _storage: &mut ConstraintSetStorage<'db>,
                _path: &PathAssignments,
            ) -> ControlFlow<Self::Break, Self::Result> {
                ControlFlow::Continue((ALWAYS_FALSE, None))
            }

            fn enter_interior<'db>(
                &mut self,
                _db: &'db dyn Db,
                storage: &mut ConstraintSetStorage<'db>,
                interior: InteriorNode,
            ) -> ControlFlow<Self::Break, Self::Interior> {
                let interior = storage.interior_node_data(interior.node());
                let disposition = if (self.should_remove)(storage, interior.constraint) {
                    Disposition::Remove
                } else {
                    Disposition::Keep
                };
                ControlFlow::Continue((disposition, interior.constraint))
            }

            fn visit_edge<'db>(
                &mut self,
                _db: &'db dyn Db,
                storage: &mut ConstraintSetStorage<'db>,
                interior: &Self::Interior,
                subtree: Self::Result,
                path: &PathAssignments,
                new_range: Range<usize>,
            ) -> ControlFlow<Self::Break, Self::Result> {
                let (disposition, _) = interior;
                match disposition {
                    // If we are keeping this node, we don't need to add any derived facts to the
                    // result; we can always re-derive them later.
                    Disposition::Keep => ControlFlow::Continue(subtree),

                    // If we are removing this node, we have to check if there are any derived facts
                    // that depend on the constraint we're about to remove. If so, we need to
                    // "remember" them by AND-ing them in with the corresponding branch.
                    Disposition::Remove => {
                        let (mut result, mut result_source_order) = subtree;
                        for (assignment, _) in &path.assignments[new_range] {
                            // Don't add back any derived facts if they are ones that we would have
                            // removed!
                            if (self.should_remove)(storage, assignment.constraint()) {
                                continue;
                            }
                            let (assignment, assignment_source_order) =
                                Node::new_satisfied_constraint(storage, *assignment);
                            result = result.and(storage, assignment);
                            result_source_order = storage
                                .ordered_source_order(result_source_order, assignment_source_order);
                        }
                        ControlFlow::Continue((result, result_source_order))
                    }
                }
            }

            fn leave_interior<'db>(
                &mut self,
                _db: &'db dyn Db,
                storage: &mut ConstraintSetStorage<'db>,
                interior: &Self::Interior,
                if_true: Self::Result,
                if_uncertain: Self::Result,
                if_false: Self::Result,
            ) -> ControlFlow<Self::Break, Self::Result> {
                let (disposition, constraint) = interior;
                match disposition {
                    // Preserve the uncertain branch when rebuilding the node. Recursive calls
                    // can introduce derived constraints earlier in the variable ordering, so
                    // use `ite_uncertain` rather than constructing a node directly.
                    Disposition::Keep => {
                        let (guard, guard_source_order) =
                            Node::new_constraint(storage, *constraint);
                        let (if_true, if_true_source_order) = if_true;
                        let (if_uncertain, if_uncertain_source_order) = if_uncertain;
                        let (if_false, if_false_source_order) = if_false;
                        let node = guard.ite_uncertain(storage, if_true, if_uncertain, if_false);
                        let left_source_order =
                            storage.ordered_source_order(guard_source_order, if_true_source_order);
                        let right_source_order = storage
                            .ordered_source_order(if_uncertain_source_order, if_false_source_order);
                        ControlFlow::Continue((
                            node,
                            storage.ordered_source_order(left_source_order, right_source_order),
                        ))
                    }

                    // If we are removing this node, then we replace it with the OR of all of its
                    // outgoing edges. That is, the result is true if there's any assignment of
                    // this node's constraint that is true. (We will have already added any
                    // necessary derived facts in the `visit_edge` method.)
                    Disposition::Remove => {
                        let (if_true, if_true_source_order) = if_true;
                        let (if_uncertain, if_uncertain_source_order) = if_uncertain;
                        let (if_false, if_false_source_order) = if_false;
                        let node = if_true.or(storage, if_uncertain).or(storage, if_false);
                        let source_order = storage
                            .ordered_source_order(if_true_source_order, if_uncertain_source_order);
                        ControlFlow::Continue((
                            node,
                            storage.ordered_source_order(source_order, if_false_source_order),
                        ))
                    }
                }
            }
        }

        let mut path = self.path_assignments(db, env, storage, source_order);
        let mut visitor = AbstractVisitor {
            should_remove,
            limits,
        };
        let (node, derived_source_order) =
            path.visit(db, env, storage, self.node(), &mut visitor)?;
        let derived_source_order =
            path.projection_source_order(storage, source_order, derived_source_order);
        ControlFlow::Continue((node, derived_source_order))
    }

    fn path_assignments<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_order: Option<SourceOrderId>,
    ) -> PathAssignments {
        let mut constraints: SmallVec<[_; 8]> = SmallVec::new();
        self.node()
            .for_each_unique_constraint(storage, &mut |constraint| {
                constraints.push(constraint);
            });
        let source_orders = storage.calculate_source_orders(source_order);
        // `PathAssignments` seeds its insertion-ordered discovered-constraint map from this list,
        // and uses that order when constructing non-commutative sequent pairs. Do not replace this
        // with TDD traversal order: doing so can change inference and lose gradual constraints.
        // Every constraint in the TDD must appear in the sidecar. If an operation introduces new
        // constraints, it must preserve their source orders rather than invent an order here.
        constraints.sort_by_key(|constraint| {
            source_orders
                .get_index_of(constraint)
                .expect("every BDD constraint should have a source-order entry")
        });

        if !self.node().is_single_conjunction(storage) {
            return PathAssignments::new(constraints, FxHashSet::default());
        }

        let mut independent_typevars = FxHashSet::default();
        let mut dependent_typevars = FxHashSet::default();
        for constraint_id in &constraints {
            let constraint = storage.constraint_data(*constraint_id);
            let typevar = storage.typevar_id(db, constraint.typevar);
            if constraint.bounds.is_concrete(db, env) {
                independent_typevars.insert(typevar);
            } else {
                dependent_typevars.extend(storage.constraint_support(*constraint_id).iter());
            }
        }

        independent_typevars.retain(|typevar| !dependent_typevars.contains(typevar));

        PathAssignments::new(constraints, independent_typevars)
    }
}

/// The result of solving a constraint set for per-typevar specializations.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Solutions<'db> {
    Unsatisfiable,
    Unconstrained,
    Constrained(SolutionPaths<'db>),
}

/// The retained solution paths and whether all their bindings could be computed.
///
/// An unsolved variable can occur in a complete result when no evidence selects its type. An
/// exhausted budget is different: consumers must not treat the fallback bindings as an exhaustive
/// set of valid specializations.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum SolutionPaths<'db> {
    Complete(Vec<Solution<'db>>),
    BudgetExceeded(Vec<Solution<'db>>),
}

impl<'db> SolutionPaths<'db> {
    /// Borrows the available solution paths, including fallback bindings if solving was incomplete.
    /// Match the outcome directly when completeness matters.
    pub(crate) fn as_slice(&self) -> &[Solution<'db>] {
        match self {
            Self::Complete(paths) | Self::BudgetExceeded(paths) => paths,
        }
    }

    /// Returns the available solution paths, discarding completeness information.
    pub(crate) fn into_vec(self) -> Vec<Solution<'db>> {
        match self {
            Self::Complete(paths) | Self::BudgetExceeded(paths) => paths,
        }
    }
}

pub(crate) type Solution<'db> = Vec<TypeVarSolution<'db>>;

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct TypeVarSolution<'db> {
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

    fn display<'db, 'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        storage: &'a ConstraintSetStorage<'db>,
    ) -> impl Display + 'a {
        let (equality_sign, range_prefix) = match self {
            ConstraintAssignment::Positive(_) => ("=", ""),
            ConstraintAssignment::Negative(_) => ("≠", "¬"),
            ConstraintAssignment::Unconstrained(_) => ("=?", "?"),
        };

        std::fmt::from_fn(move |f| {
            let constraint_data = storage.constraint_data(self.constraint());
            let lower = constraint_data.bounds.lower_bound().ty();
            let upper = constraint_data.bounds.upper_bound().ty();
            let typevar = constraint_data.typevar;
            if lower.is_equivalent_to(db, env, upper) {
                // If this typevar is equivalent to another, output the constraint in a
                // consistent alphabetical order, regardless of the salsa ordering that we are
                // using the in BDD.
                if let Type::TypeVar(bound) = lower {
                    let bound = bound.identity(db).display(db).to_string();
                    let typevar = typevar.identity(db).display(db).to_string();
                    let (smaller, larger) = if bound < typevar {
                        (bound, typevar)
                    } else {
                        (typevar, bound)
                    };
                    return write!(f, "({smaller} {equality_sign} {larger})");
                }

                return write!(
                    f,
                    "({} {} {})",
                    typevar.identity(db).display(db),
                    equality_sign,
                    lower.display(db, env)
                );
            }

            if lower.is_never() && upper.is_object() {
                return write!(
                    f,
                    "({} {} *)",
                    typevar.identity(db).display(db),
                    equality_sign
                );
            }

            f.write_str(range_prefix)?;
            f.write_str("(")?;
            if !lower.is_never() {
                write!(f, "{} ≤ ", lower.display(db, env))?;
            }
            typevar.identity(db).display(db).fmt(f)?;
            if !upper.is_object() {
                write!(f, " ≤ {}", upper.display(db, env))?;
            }
            f.write_str(")")
        })
    }
}

/// A visitor for walking the paths of a BDD.
///
/// **NOTE**: This trait gives you full control over the walking process: in particular, you have
/// more opportunities to abort the walk early. If you want to perform a simple "fold" over all of
/// the paths, the [`PathFold`] trait is easier to implement, and can also be used as a
/// `PathVisitor`.
///
/// Each path starts at the root node and ends at a terminal node, and represents one family of
/// typevar assignments described by the BDD. Each path can be either _satisfied_, meaning that
/// this family of assignments is accepted by the constraint set; _unsatisfied_, meaning that this
/// family of assignments is _not_ accepted by the constraint set; or _impossible_, meaning that
/// this family of assignments contains a contradiction, and cannot possibly ever occur.
///
/// To visit the BDD paths:
///
/// - We start at the root node.
///
/// - Each time we encounter an interior node, we call the visitor's `enter_interior` method. We
///   then process walk the interior node's `true`, `uncertain`, and `false` outgoing edges.
///
/// - To process an edge, we recursively visit the node that the edge points to (getting a `Result`
///   for that subtree), and then call the visitor's `visit_edge` method. This lets you modify the
///   subtree's value based on the assignments that were added to the path by this edge. (This
///   includes at least the constraint checked by the interior node containing this edge, and can
///   also include any additional derived facts that we learn based on whatever other assignments
///   currently hold on the path.)
///
/// - Once we have processed all of the edges for an interior node, we call the visitor's
///   `leave_interior` method. This lets you combine the `Result`s from each outgoing edge into a
///   single `Result` that represents the subtree rooted at this interior node.
///
/// Throughout this process, if any of your methods return [`ControlFlow::Break`], we will abort
/// the path walk and immediately return that value.
trait PathVisitor {
    type Result;
    type Interior;
    type Break;

    /// Called before visiting any interior or terminal node. Returning `Break` prevents the
    /// traversal from entering the node or deriving facts from its outgoing edges.
    fn visit_node(&mut self) -> ControlFlow<Self::Break> {
        ControlFlow::Continue(())
    }

    /// Called when we reach the end of a satisfied path. `path` will contain all of the
    /// assignments on this path. The `Result` value that you return will be propagated back up as
    /// we "unwind" this path.
    fn visit_satisfied<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result>;

    /// Called when we reach the end of an unsatisfied path. `path` will contain all of the
    /// assignments on this path. The `Result` value that you return will be propagated back up as
    /// we "unwind" this path.
    fn visit_unsatisfied<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result>;

    /// Called when we determine that a path is impossible, either because its assignments
    /// contradict each other, or because an edge is structurally absent (such as the uncertain
    /// edge when visiting a negated BDD). The `Result` value that you return will be propagated
    /// back up as we "unwind" this path.
    fn visit_impossible<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result>;

    /// Called on the way down as we enter each interior node. You can create a
    /// [`Interior`][Self::Interior] value that will be passed to the
    /// [`visit_edge`][Self::visit_edge] and [`leave_interior`][Self::leave_interior] methods
    /// when we call them for this node.
    fn enter_interior<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        interior_node: InteriorNode,
    ) -> ControlFlow<Self::Break, Self::Interior>;

    /// Called once for each edge in the BDD. You are given the [`Result`][Self::Result] value
    /// of the subtree that the edge points to, as well as the origin and derived assignments that
    /// are added by the edge.
    fn visit_edge<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        interior_value: &Self::Interior,
        subtree: Self::Result,
        path: &PathAssignments,
        new_range: Range<usize>,
    ) -> ControlFlow<Self::Break, Self::Result>;

    /// Called on the way back up as we leave each interior node in the BDD. Combines the
    /// [`Result`][Self::Result] values for each of the interior node's subtrees.
    fn leave_interior<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        interior_value: &Self::Interior,
        if_true: Self::Result,
        if_uncertain: Self::Result,
        if_false: Self::Result,
    ) -> ControlFlow<Self::Break, Self::Result>;
}

/// A visitor for "folding" over the paths in a BDD, producing a single value that summarizes all
/// of them.
///
/// This is a simpler trait to implement when you don't need as much control over the path walk.
/// Any type that implements this trait can also be used as a [`PathVisitor`].
trait PathFold {
    type Result;
    type Break;

    /// Returns the base case value that represents a satisfied path.
    fn satisfied<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result>;

    /// Returns the base case value that represents an unsatisfied path.
    fn unsatisfied<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result>;

    /// Returns the base case value that represents an impossible path.
    fn impossible<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result>;

    /// Combines the values for each subtree of an interior node, returning a value that represents
    /// the subtree rooted at that node.
    fn combine<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        if_true: Self::Result,
        if_uncertain: Self::Result,
        if_false: Self::Result,
    ) -> ControlFlow<Self::Break, Self::Result>;
}

impl<T> PathVisitor for T
where
    T: PathFold,
{
    type Result = <T as PathFold>::Result;
    type Interior = ();
    type Break = <T as PathFold>::Break;

    fn visit_satisfied<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result> {
        PathFold::satisfied(self, db, storage, path)
    }

    fn visit_unsatisfied<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result> {
        PathFold::unsatisfied(self, db, storage, path)
    }

    fn visit_impossible<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result> {
        PathFold::impossible(self, db, storage, path)
    }

    fn enter_interior<'db>(
        &mut self,
        _db: &'db dyn Db,
        _storage: &mut ConstraintSetStorage<'db>,
        _interior_node: InteriorNode,
    ) -> ControlFlow<Self::Break, Self::Interior> {
        ControlFlow::Continue(())
    }

    fn visit_edge<'db>(
        &mut self,
        _db: &'db dyn Db,
        _storage: &mut ConstraintSetStorage<'db>,
        _interior_value: &Self::Interior,
        subtree: Self::Result,
        _path: &PathAssignments,
        _new_range: Range<usize>,
    ) -> ControlFlow<Self::Break, Self::Result> {
        ControlFlow::Continue(subtree)
    }

    fn leave_interior<'db>(
        &mut self,
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        _interior_value: &Self::Interior,
        if_true: Self::Result,
        if_uncertain: Self::Result,
        if_false: Self::Result,
    ) -> ControlFlow<Self::Break, Self::Result> {
        PathFold::combine(self, db, storage, if_true, if_uncertain, if_false)
    }
}

/// A path visitor that breaks early if it encounters a satisfied path. When applying this visitor,
/// a `Continue` result indicates that no satisfied path was found, and the BDD was therefore
/// unsatisfiable. A `Break` result indicates the opposite.
struct IsNeverSatisfiedVisitor;

impl PathFold for IsNeverSatisfiedVisitor {
    type Result = ();
    type Break = ();

    fn satisfied<'db>(
        &mut self,
        _db: &'db dyn Db,
        _storage: &mut ConstraintSetStorage<'db>,
        _path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result> {
        ControlFlow::Break(())
    }

    fn unsatisfied<'db>(
        &mut self,
        _db: &'db dyn Db,
        _storage: &mut ConstraintSetStorage<'db>,
        _path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result> {
        ControlFlow::Continue(())
    }

    fn impossible<'db>(
        &mut self,
        _db: &'db dyn Db,
        _storage: &mut ConstraintSetStorage<'db>,
        _path: &PathAssignments,
    ) -> ControlFlow<Self::Break, Self::Result> {
        ControlFlow::Continue(())
    }

    fn combine<'db>(
        &mut self,
        _db: &'db dyn Db,
        _storage: &mut ConstraintSetStorage<'db>,
        _if_true: Self::Result,
        _if_uncertain: Self::Result,
        _if_false: Self::Result,
    ) -> ControlFlow<Self::Break, Self::Result> {
        ControlFlow::Continue(())
    }
}

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

    fn display<'db>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &ConstraintSetStorage<'db>,
    ) -> String {
        if self.constraints.is_empty() {
            return String::from("always");
        }

        // This is a bit heavy-handed, but we need to output the constraints in a consistent order
        // even though Salsa IDs are assigned non-deterministically. This Display output is only
        // used in test cases, so we don't need to over-optimize it.
        let mut constraints: Vec<_> = self
            .constraints
            .iter()
            .map(|constraint| constraint.display(db, env, storage).to_string())
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

    fn display<'db>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &ConstraintSetStorage<'db>,
    ) -> String {
        // This is a bit heavy-handed, but we need to output the clauses in a consistent order
        // even though Salsa IDs are assigned non-deterministically. This Display output is only
        // used in test cases, so we don't need to over-optimize it.

        if self.clauses.is_empty() {
            return String::from("never");
        }
        let mut clauses: Vec<_> = self
            .clauses
            .iter()
            .map(|clause| clause.display(db, env, storage))
            .collect();
        clauses.sort();
        clauses.join(" ∨ ")
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    use indoc::indoc;
    use pretty_assertions::assert_eq;

    use crate::db::tests::{TestDb, setup_db};
    use crate::place::global_symbol;
    use crate::types::generics::ApplySpecialization;
    use crate::types::typevar::{
        TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation,
    };
    use crate::types::{BoundTypeVarInstance, KnownClass, SubclassOfType, TypeVarVariance};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::name::Name;
    use ty_python_core::ProgramFile;

    fn create_typevar<'db>(db: &'db TestDb, name: &'static str) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            Name::new_static(name),
            TypeVarVariance::Invariant,
        )
    }

    fn create_constraint<'db, 'c>(
        db: &'db TestDb,
        builder: &'c ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
        bound: KnownClass,
    ) -> ConstraintSet<'db, 'c> {
        let env = db.program_environment();
        let ty = bound.to_instance(db, &env);
        ConstraintSet::constrain_typevar(db, &env, builder, bound_typevar, ty, ty)
    }

    fn known_instance(db: &TestDb, class: KnownClass) -> Type<'_> {
        class.to_instance(db, &db.program_environment())
    }

    fn bounded_path_bounds<'db>(
        db: &'db TestDb,
        set: ConstraintSet<'db, '_>,
        inferable: TypeVarSet<'db>,
        max_paths: usize,
        max_visits: usize,
    ) -> Result<PathBounds<'db>, ProjectionError> {
        PathBounds::compute_bounded(
            db,
            &db.program_environment(),
            &mut set.builder.storage.borrow_mut(),
            set.node,
            inferable,
            set.source_order,
            SolutionBudget {
                paths: max_paths,
                visits: max_visits,
                ..SolutionBudget::default()
            },
        )
    }

    #[derive(Default)]
    struct CountSolutionLimits {
        visits: usize,
        paths: usize,
    }

    impl SolutionLimits for CountSolutionLimits {
        type Break = Infallible;

        fn visit_node(&mut self) -> ControlFlow<Self::Break> {
            self.visits += 1;
            ControlFlow::Continue(())
        }

        fn satisfied_path(&mut self) -> ControlFlow<Self::Break> {
            self.paths += 1;
            ControlFlow::Continue(())
        }
    }

    #[test]
    fn type_mapping_updates_constraint_bounds() {
        // (list[U] ≤ T ≤ list[U])[U ↦ int] = (list[int] ≤ T ≤ list[int])
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let list_of_u = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(u)]);
        let set = ConstraintSet::constrain_typevar(db, &env, &builder, t, list_of_u, list_of_u);

        let int = KnownClass::Int.to_instance(db, &env);
        let mapped = set.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Single(u, int)),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        );
        let list_of_int = KnownClass::List.to_specialized_instance(db, &env, &[int]);
        let expected =
            ConstraintSet::constrain_typevar(db, &env, &builder, t, list_of_int, list_of_int);

        assert!(
            mapped
                .iff(db, &builder, expected)
                .is_always_satisfied(db, &env)
        );
    }

    #[test]
    fn constraint_support_ignores_typevar_declaration_defaults() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let metadata = create_typevar(db, "Metadata");
        let u = create_typevar(db, "U");
        let declaration = TypeVarInstance::new(
            db,
            u.typevar(db).identity(db),
            None,
            Some(TypeVarVariance::Invariant),
            Some(TypeVarDefaultEvaluation::Eager(Type::TypeVar(metadata))),
        );
        let u = BoundTypeVarInstance::new(
            db,
            declaration,
            u.binding_context(db),
            u.paramspec_attr(db),
            u.freshness(db),
        );
        let actual_bound = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(u)]);
        let mut storage = ConstraintSetStorage::default();
        let support = storage.intern_constraint_typevars(
            db,
            &env,
            t,
            ConstraintBounds::new(None, Some(ConstraintBound::Evidence(actual_bound))),
        );
        let mentioned = support
            .iter()
            .map(|typevar| storage.typevar_data(typevar))
            .collect::<Vec<_>>();

        assert_eq!(mentioned, vec![t, u]);
        assert!(support.is_complete());

        let builder = ConstraintSetBuilder::new();
        let constraint =
            ConstraintSet::constrain_typevar_upper_bound(db, &env, &builder, t, actual_bound);
        assert!(constraint.mentions_typevar(t));
        assert!(constraint.mentions_typevar(u));
        assert!(!constraint.mentions_typevar(metadata));
    }

    #[test]
    fn constraint_support_is_complete_for_lazy_typevar_declaration_metadata() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        for (bound_or_constraints, default) in [
            (
                Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
                None,
            ),
            (
                Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints),
                None,
            ),
            (None, Some(TypeVarDefaultEvaluation::Lazy)),
        ] {
            let declaration = TypeVarInstance::new(
                db,
                u.typevar(db).identity(db),
                bound_or_constraints,
                Some(TypeVarVariance::Invariant),
                default,
            );
            let u = BoundTypeVarInstance::new(
                db,
                declaration,
                u.binding_context(db),
                u.paramspec_attr(db),
                u.freshness(db),
            );
            let mut storage = ConstraintSetStorage::default();
            let support = storage.intern_constraint_typevars(
                db,
                &env,
                t,
                ConstraintBounds::new(None, Some(ConstraintBound::Evidence(Type::TypeVar(u)))),
            );
            let mentioned = support
                .iter()
                .map(|typevar| storage.typevar_data(typevar))
                .collect::<Vec<_>>();

            assert_eq!(mentioned, vec![t, u]);
            assert!(support.is_complete());
        }
    }

    #[test]
    fn type_mapping_evaluates_mapped_subjects() {
        // ((T = int) ∧ ¬(T = str))[T ↦ int] = true
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let set = create_constraint(db, &builder, t, KnownClass::Int).and(db, &builder, || {
            create_constraint(db, &builder, t, KnownClass::Str).negate(db, &builder)
        });

        let mapped = set.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Single(
                t,
                KnownClass::Int.to_instance(db, &env),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        );

        assert!(mapped.is_always_satisfied(db, &env));
    }

    #[test]
    fn type_mapping_handles_absorbed_constraints_in_source_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let str = create_constraint(db, &builder, t, KnownClass::Str);
        let int = create_constraint(db, &builder, t, KnownClass::Int);
        let set = str.or(db, &builder, || int).and(db, &builder, || str);

        let mapped = set.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Single(
                t,
                KnownClass::Str.to_instance(db, &env),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        );

        assert!(mapped.is_always_satisfied(db, &env));
    }

    #[test]
    fn upper_bound_collapses_never() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let int = known_instance(db, KnownClass::Int);

        let mut upper = UpperBound::from_clause(int);
        upper.add_clause(ConstraintBound::Evidence(Type::Never));
        assert_eq!(upper.evidence, FxOrderSet::from_iter([Type::Never]));
        assert_eq!(upper.materialize_exact(db, &env), Type::Never);

        upper.add_clause(ConstraintBound::Evidence(int));
        assert_eq!(upper.evidence, FxOrderSet::from_iter([Type::Never]));
    }

    #[test]
    fn upper_bound_recovers_redundant_single_bounds() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let int = known_instance(db, KnownClass::Int);
        let bool = known_instance(db, KnownClass::Bool);
        let str = known_instance(db, KnownClass::Str);
        let int_or_str = UnionType::from_two_elements(db, &env, int, str);
        let u = create_typevar(db, "U").map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(int_or_str))
        });
        let u = Type::TypeVar(u);

        for (clauses, expected) in [
            ([Type::object(), int], int),
            ([int, Type::object()], int),
            ([int, bool], bool),
            ([bool, int], bool),
            ([int_or_str, u], u),
            ([u, int_or_str], u),
        ] {
            let mut upper = UpperBound::unconstrained();
            for clause in clauses {
                upper.add_clause(ConstraintBound::Evidence(clause));
            }

            assert_eq!(upper.evidence.len(), 2);
            assert_eq!(upper.as_single_bound(db, &env), Some(expected));
        }
    }

    #[test]
    fn upper_bound_distinguishes_missing_bound_from_explicit_object() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let missing = UpperBound::unconstrained();
        assert!(!missing.has_evidence());
        assert_eq!(missing.as_single_bound(db, &env), Some(Type::object()));

        let explicit = UpperBound::from_clause(Type::object());
        assert!(explicit.has_evidence());
        assert_eq!(explicit.as_single_bound(db, &env), Some(Type::object()));
    }

    #[test]
    fn upper_bound_does_not_materialize_overlapping_union_clauses() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);
        let bytes = known_instance(db, KnownClass::Bytes);
        let int_or_str = UnionType::from_two_elements(db, &env, int, str);
        let int_or_bytes = UnionType::from_two_elements(db, &env, int, bytes);

        for clauses in [[int_or_str, int_or_bytes], [int_or_bytes, int_or_str]] {
            let mut upper = UpperBound::unconstrained();
            for clause in clauses {
                upper.add_clause(ConstraintBound::Evidence(clause));
            }

            assert_eq!(upper.materialize_exact(db, &env), int);
            assert_eq!(upper.as_single_bound(db, &env), None);
        }
    }

    #[test]
    fn upper_bound_does_not_treat_nontrivial_intersection_as_single_bound() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let int = known_instance(db, KnownClass::Int);
        let u = Type::TypeVar(create_typevar(db, "U"));
        let mut upper = UpperBound::from_clause(u);
        upper.add_clause(ConstraintBound::Evidence(int));

        assert!(
            upper
                .materialize_exact(db, &env)
                .is_nontrivial_intersection(db)
        );
        assert_eq!(upper.as_single_bound(db, &env), None);
    }

    #[test]
    fn trivial_disjointness_does_not_claim_bounded_typevar_class_is_disjoint() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let bool = known_instance(db, KnownClass::Bool);
        let u = create_typevar(db, "U")
            .map_bound_or_constraints(db, |_| Some(TypeVarBoundOrConstraints::UpperBound(bool)));
        let type_of_u = SubclassOfType::from(db, &env, u);
        let bool_class = KnownClass::Bool.to_class_literal(db, &env);

        for (left, right) in [(type_of_u, bool_class), (bool_class, type_of_u)] {
            let trivial =
                left.when_trivially_disjoint_from(db, &env, right, &builder, TypeVarSet::None);
            let full = left.when_disjoint_from(db, &env, right, &builder, TypeVarSet::None);

            assert!(trivial.is_trivially_never_satisfied());
            assert!(!full.is_always_satisfied(db, &env));
        }
    }

    #[test]
    fn trivial_disjointness_implies_full_disjointness() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let bool = known_instance(db, KnownClass::Bool);
        let u = create_typevar(db, "U")
            .map_bound_or_constraints(db, |_| Some(TypeVarBoundOrConstraints::UpperBound(bool)));
        let types = [
            Type::Never,
            Type::object(),
            bool,
            known_instance(db, KnownClass::Int),
            known_instance(db, KnownClass::Str),
            Type::int_literal(0),
            Type::int_literal(1),
            Type::bool_literal(true),
            Type::bool_literal(false),
            Type::string_literal(db, "value"),
            KnownClass::Bool.to_class_literal(db, &env),
            KnownClass::Int.to_class_literal(db, &env),
            SubclassOfType::from(db, &env, u),
        ];
        let mut positive_results = 0;

        for left in types {
            for right in types {
                let trivial =
                    left.when_trivially_disjoint_from(db, &env, right, &builder, TypeVarSet::None);
                if trivial.is_trivially_always_satisfied() {
                    positive_results += 1;
                    assert!(
                        left.when_disjoint_from(db, &env, right, &builder, TypeVarSet::None)
                            .is_always_satisfied(db, &env),
                        "cheap disjointness incorrectly accepts `{}` and `{}`",
                        left.display(db, &env),
                        right.display(db, &env)
                    );
                }
            }
        }

        assert!(positive_results > 0);
    }

    #[test]
    fn bounded_path_fast_paths_respect_limits() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let t = create_typevar(db, "T");
        let inferable = TypeVarSet::from_typevars(db, [t]);

        for (set, expected) in [
            (ConstraintSet::always(&builder), PathBounds::Unconstrained),
            (ConstraintSet::never(&builder), PathBounds::Unsatisfiable),
        ] {
            assert_eq!(
                bounded_path_bounds(db, set, inferable, 0, 0),
                Err(ProjectionError::TraversalBudgetExceeded)
            );
            assert_eq!(bounded_path_bounds(db, set, inferable, 0, 1), Ok(expected));
        }

        let set = create_constraint(db, &builder, t, KnownClass::Int);
        let expected = PathBounds::compute(
            db,
            &env,
            &mut builder.storage.borrow_mut(),
            set.node,
            inferable,
            set.source_order,
        );
        assert_eq!(
            bounded_path_bounds(db, set, inferable, 0, 2),
            Err(ProjectionError::PathBudgetExceeded)
        );
        assert_eq!(
            bounded_path_bounds(db, set, inferable, 1, 1),
            Err(ProjectionError::TraversalBudgetExceeded)
        );
        assert_eq!(bounded_path_bounds(db, set, inferable, 1, 2), Ok(expected));
    }

    #[test]
    fn bounded_path_collection_shares_preprocessing_visits() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let t = create_typevar(db, "T");
        let hidden = create_typevar(db, "Hidden");
        let visible = create_constraint(db, &builder, t, KnownClass::Int);
        let hidden_alternatives =
            create_constraint(db, &builder, hidden, KnownClass::Str).or(db, &builder, || {
                create_constraint(db, &builder, hidden, KnownClass::Bytes)
            });
        let set = visible.and(db, &builder, || hidden_alternatives);
        let inferable = TypeVarSet::from_typevars(db, [t]);
        let mut storage = builder.storage.borrow_mut();
        let source_orders = storage.calculate_source_orders(set.source_order);
        let mut preprocessing = CountSolutionLimits::default();
        let ControlFlow::Continue(fast_path) = PathBounds::compute_simple_bound_conjunction(
            db,
            &env,
            &mut storage,
            &source_orders,
            set.node,
            inferable,
            &mut preprocessing,
        );
        assert_eq!(fast_path, None);
        let ControlFlow::Continue(_) = set.node.remove_noninferable(
            db,
            &env,
            &mut storage,
            inferable,
            set.source_order,
            &mut preprocessing,
        );

        let mut complete = CountSolutionLimits::default();
        let ControlFlow::Continue(expected) = PathBounds::compute_with_limits(
            db,
            &env,
            &mut storage,
            set.node,
            inferable,
            set.source_order,
            &mut complete,
        );
        assert_eq!(complete.paths, 1);
        assert!(complete.visits > preprocessing.visits);
        drop(storage);

        assert_eq!(
            bounded_path_bounds(db, set, inferable, 1, preprocessing.visits),
            Err(ProjectionError::TraversalBudgetExceeded)
        );
        assert_eq!(
            bounded_path_bounds(db, set, inferable, 1, complete.visits - 1),
            Err(ProjectionError::TraversalBudgetExceeded)
        );
        assert_eq!(
            bounded_path_bounds(db, set, inferable, 1, complete.visits),
            Ok(expected)
        );
        assert_eq!(
            bounded_path_bounds(db, set, inferable, 0, complete.visits),
            Err(ProjectionError::PathBudgetExceeded)
        );
    }

    #[test]
    fn simple_lower_bound_conjunction_skips_sequent_analysis() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let int = KnownClass::Int.to_instance(db, &env);
        let str = KnownClass::Str.to_instance(db, &env);
        let set = ConstraintSet::constrain_typevar_lower_bound(db, &env, &builder, t, int).and(
            db,
            &builder,
            || ConstraintSet::constrain_typevar_lower_bound(db, &env, &builder, t, str),
        );
        let inferable = TypeVarSet::from_typevars(db, [t]);
        let (single_sequents, pair_sequents) = {
            let storage = builder.storage.borrow();
            (
                storage.single_sequent_cache.len(),
                storage.pair_sequent_cache.len(),
            )
        };

        let solutions = set.solutions(db, &env, inferable);
        assert_eq!(
            solutions,
            Ok(Solutions::Constrained(SolutionPaths::Complete(vec![vec![
                TypeVarSolution {
                    bound_typevar: t,
                    solution: UnionType::from_elements(db, &env, [int, str]),
                }
            ]])))
        );

        let storage = builder.storage.borrow();
        assert_eq!(storage.single_sequent_cache.len(), single_sequents);
        assert_eq!(storage.pair_sequent_cache.len(), pair_sequents);
    }

    #[test]
    fn simple_exact_bound_conjunction_skips_sequent_analysis() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let int = KnownClass::Int.to_instance(db, &env);
        let set = ConstraintSet::constrain_typevar(db, &env, &builder, t, int, int).and(
            db,
            &builder,
            || ConstraintSet::constrain_typevar(db, &env, &builder, u, int, int),
        );
        let inferable = TypeVarSet::from_typevars(db, [t, u]);
        let (single_sequents, pair_sequents) = {
            let storage = builder.storage.borrow();
            (
                storage.single_sequent_cache.len(),
                storage.pair_sequent_cache.len(),
            )
        };

        let Ok(Solutions::Constrained(solutions)) = set.solutions(db, &env, inferable) else {
            panic!("expected constrained solutions");
        };
        let solutions = solutions.into_vec();
        assert_eq!(solutions.len(), 1);
        assert_eq!(solutions[0].len(), 2);
        assert!(solutions[0].contains(&TypeVarSolution {
            bound_typevar: t,
            solution: int,
        }));
        assert!(solutions[0].contains(&TypeVarSolution {
            bound_typevar: u,
            solution: int,
        }));

        let storage = builder.storage.borrow();
        assert_eq!(storage.single_sequent_cache.len(), single_sequents);
        assert_eq!(storage.pair_sequent_cache.len(), pair_sequents);
    }

    #[test]
    fn simple_unsatisfiable_exact_bound_conjunction_skips_sequent_analysis() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let int = KnownClass::Int.to_instance(db, &env);
        let str = KnownClass::Str.to_instance(db, &env);
        let set = ConstraintSet::constrain_typevar(db, &env, &builder, t, int, int).and(
            db,
            &builder,
            || ConstraintSet::constrain_typevar(db, &env, &builder, t, str, str),
        );
        let inferable = TypeVarSet::from_typevars(db, [t]);
        let (single_sequents, pair_sequents) = {
            let storage = builder.storage.borrow();
            (
                storage.single_sequent_cache.len(),
                storage.pair_sequent_cache.len(),
            )
        };

        assert_eq!(
            set.solutions(db, &env, inferable),
            Ok(Solutions::Unsatisfiable)
        );

        let storage = builder.storage.borrow();
        assert_eq!(storage.single_sequent_cache.len(), single_sequents);
        assert_eq!(storage.pair_sequent_cache.len(), pair_sequents);
    }

    #[test]
    fn default_solve_leaves_unbounded_typevar_unsolved_without_bounds() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let path_bound = PathBound {
            bound_typevar: t,
            evidence_lower: None,
            validity_lower: Type::Never,
            upper: UpperBound::unconstrained(),
            has_only_gradual_evidence: false,
        };

        assert_eq!(
            PathBounds::default_solve(db, &env, &builder, &path_bound),
            PathBoundSolution::Unsolved
        );
        assert_eq!(PathBoundSolution::Unsolved.as_type(), None);
        assert_eq!(
            PathBounds::Constrained(Box::new([Box::new([path_bound])])).solve(db, &env, &builder),
            Solutions::Constrained(SolutionPaths::Complete(vec![vec![]]))
        );
    }

    #[test]
    fn default_solve_distinguishes_invalid_bounds_from_never() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let mut bounds = ConstraintBoundsBuilder::default();
        bounds.add_lower(
            db,
            &env,
            ConstraintBound::Evidence(known_instance(db, KnownClass::Int)),
        );
        bounds.add_upper(
            db,
            &env,
            ConstraintBound::Evidence(known_instance(db, KnownClass::Str)),
        );
        let invalid = bounds.finish(db, &env, t);

        assert_eq!(
            PathBounds::preliminary_solve(db, &env, &builder, &invalid),
            PathBoundSolution::Unsatisfiable
        );
        assert_eq!(
            PathBounds::default_solve(db, &env, &builder, &invalid),
            PathBoundSolution::Unsatisfiable
        );
        assert_eq!(PathBoundSolution::Unsatisfiable.as_type(), None);
        assert_eq!(
            PathBounds::default_solve(db, &env, &builder, &PathBound::exact(t, Type::Never)),
            PathBoundSolution::Solved(Type::Never)
        );
        assert_eq!(
            PathBoundSolution::Solved(Type::Never).as_type(),
            Some(Type::Never)
        );
    }

    #[test]
    fn promoting_solutions_preserves_completeness() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let literal = Type::int_literal(1);
        let int = known_instance(db, KnownClass::Int);

        for (solution, expected) in [
            (
                PathBoundSolution::Solved(literal),
                PathBoundSolution::Solved(int),
            ),
            (
                PathBoundSolution::BudgetExceeded {
                    fallback: Some(literal),
                },
                PathBoundSolution::BudgetExceeded {
                    fallback: Some(int),
                },
            ),
            (PathBoundSolution::Unsolved, PathBoundSolution::Unsolved),
            (
                PathBoundSolution::Unsatisfiable,
                PathBoundSolution::Unsatisfiable,
            ),
            (
                PathBoundSolution::BudgetExceeded { fallback: None },
                PathBoundSolution::BudgetExceeded { fallback: None },
            ),
        ] {
            assert_eq!(solution.map(|ty| ty.promote(db, &env)), expected);
        }
    }

    #[test]
    fn solution_budget_exhaustion_preserves_available_bindings() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
class A: ...
class B: ...
class C: ...
class D: ...
class E: ...
"#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        let instance = |name| {
            global_symbol(db, file, name)
                .place
                .expect_type()
                .to_instance_approximation(db, &env)
                .ok_or_else(|| anyhow::anyhow!("expected class {name}"))
        };
        // Six non-disjoint intersections exceed the four-term DNF construction budget.
        let left = UnionType::from_elements(db, &env, [instance("A")?, instance("B")?]);
        let right =
            UnionType::from_elements(db, &env, [instance("C")?, instance("D")?, instance("E")?]);
        assert!(IntersectionType::bounded_from_elements(db, &env, [left, right]).is_none());

        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);
        let binding = |bound_typevar, solution| TypeVarSolution {
            bound_typevar,
            solution,
        };

        for lower in [None, Some(Type::any())] {
            let mut bounds = ConstraintBoundsBuilder::default();
            bounds.add_lower(
                db,
                &env,
                lower.map_or_else(ConstraintBound::missing_lower, ConstraintBound::Evidence),
            );
            bounds.add_upper(db, &env, ConstraintBound::Evidence(left));
            bounds.add_upper(db, &env, ConstraintBound::Evidence(right));
            let exhausted = bounds.finish(db, &env, t);
            let expected = PathBoundSolution::BudgetExceeded { fallback: lower };
            assert_eq!(
                PathBounds::preliminary_solve(db, &env, &builder, &exhausted),
                lower.map_or(expected, PathBoundSolution::Solved)
            );
            assert_eq!(
                PathBounds::default_solve(db, &env, &builder, &exhausted),
                expected
            );
            assert_eq!(expected.as_type(), lower);

            for reverse in [false, true] {
                let mut paths = vec![
                    vec![exhausted.clone(), PathBound::exact(u, str)].into_boxed_slice(),
                    vec![PathBound::exact(t, int)].into_boxed_slice(),
                ];
                let mut recovered = lower
                    .map(|ty| binding(t, ty))
                    .into_iter()
                    .collect::<Vec<_>>();
                recovered.push(binding(u, str));
                let mut expected_paths = vec![recovered, vec![binding(t, int)]];
                if reverse {
                    paths.reverse();
                    expected_paths.reverse();
                }
                assert_eq!(
                    PathBounds::Constrained(paths.into_boxed_slice()).solve(db, &env, &builder),
                    Solutions::Constrained(SolutionPaths::BudgetExceeded(expected_paths))
                );
            }

            // A later contradiction rejects the entire path, including its exhausted binding.
            let mut invalid = ConstraintBoundsBuilder::default();
            invalid.add_lower(db, &env, ConstraintBound::Evidence(int));
            invalid.add_upper(db, &env, ConstraintBound::Evidence(str));
            let invalid = invalid.finish(db, &env, u);
            for invalid_first in [false, true] {
                let mut rejected = vec![exhausted.clone(), invalid.clone()];
                if invalid_first {
                    rejected.reverse();
                }
                let paths = PathBounds::Constrained(Box::new([
                    rejected.into_boxed_slice(),
                    Box::new([PathBound::exact(t, int)]),
                ]));
                assert_eq!(
                    paths.solve(db, &env, &builder),
                    Solutions::Constrained(SolutionPaths::Complete(vec![vec![binding(t, int)]]))
                );
            }
        }

        // Gradual upper bounds can admit multiple declared constraints while still exceeding
        // the budget needed to construct their intersection.
        let constrained = create_typevar(db, "Constrained").map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::Constraints(
                TypeVarConstraints::new(db, [int, str].as_slice()),
            ))
        });
        let gradual_upper =
            [left, right].map(|upper| UnionType::from_two_elements(db, &env, upper, Type::any()));
        assert!(IntersectionType::bounded_from_elements(db, &env, gradual_upper).is_none());
        let mut bounds = ConstraintBoundsBuilder::default();
        for upper in gradual_upper {
            bounds.add_upper(db, &env, ConstraintBound::Evidence(upper));
        }
        let exhausted = bounds.finish(db, &env, constrained);
        assert!(exhausted.has_only_gradual_evidence);
        assert_eq!(
            PathBounds::preliminary_solve(db, &env, &builder, &exhausted),
            PathBoundSolution::BudgetExceeded { fallback: None }
        );
        assert_eq!(
            PathBounds::default_solve(db, &env, &builder, &exhausted),
            PathBoundSolution::BudgetExceeded { fallback: None }
        );
        Ok(())
    }

    #[test]
    fn constraint_intersection_detects_disjoint_union_upper_bounds() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);
        let bytes = known_instance(db, KnownClass::Bytes);
        let bytearray = known_instance(db, KnownClass::Bytearray);
        let int_or_str = UnionType::from_two_elements(db, &env, int, str);
        let bytes_or_bytearray = UnionType::from_two_elements(db, &env, bytes, bytearray);
        let mut storage = builder.storage.borrow_mut();
        let left = ConstraintId::new_with_bounds(
            db,
            &env,
            &mut storage,
            t,
            Some(ConstraintBound::Evidence(int)),
            Some(ConstraintBound::Evidence(int_or_str)),
        );
        let right = ConstraintId::new_with_bounds(
            db,
            &env,
            &mut storage,
            t,
            None,
            Some(ConstraintBound::Evidence(bytes_or_bytearray)),
        );

        // Check satisfiability against each upper clause before punting on the union-bearing
        // merged upper bound. The old size heuristic returned `CannotSimplify` here before
        // discovering that `int` cannot satisfy the second upper clause.
        assert_matches!(
            left.intersect(db, &env, &mut storage, right),
            IntersectionResult::Disjoint
        );
    }

    #[test]
    fn trivial_satisfaction_only_recognizes_terminals() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let t_str = create_constraint(db, &builder, t, KnownClass::Str);
        let impossible = t_int.and(db, &builder, || t_str);

        assert!(ConstraintSet::always(&builder).is_trivially_always_satisfied());
        assert!(!ConstraintSet::always(&builder).is_trivially_never_satisfied());
        assert!(ConstraintSet::never(&builder).is_trivially_never_satisfied());
        assert!(!ConstraintSet::never(&builder).is_trivially_always_satisfied());
        assert!(!t_int.is_trivially_always_satisfied());
        assert!(!t_int.is_trivially_never_satisfied());
        assert!(impossible.is_never_satisfied(db, &env));
        assert!(!impossible.is_trivially_never_satisfied());

        let t_bool_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            &builder,
            t,
            KnownClass::Bool.to_instance(db, &env),
        );
        let t_int_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            &builder,
            t,
            KnownClass::Int.to_instance(db, &env),
        );
        let tautology = t_bool_upper
            .negate(db, &builder)
            .or(db, &builder, || t_int_upper);

        assert!(tautology.is_always_satisfied(db, &env));
        assert!(!tautology.is_trivially_always_satisfied());
    }

    #[test]
    fn combinators_only_short_circuit_on_terminal_saturation() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let t_str = create_constraint(db, &builder, t, KnownClass::Str);
        let impossible = t_int.and(db, &builder, || t_str);
        let t_bool_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            &builder,
            t,
            KnownClass::Bool.to_instance(db, &env),
        );
        let t_int_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            &builder,
            t,
            KnownClass::Int.to_instance(db, &env),
        );
        let tautology = t_bool_upper
            .negate(db, &builder)
            .or(db, &builder, || t_int_upper);

        let forced = Cell::new(0);
        ConstraintSet::never(&builder).and(db, &builder, || {
            forced.set(forced.get() + 1);
            t_int
        });
        ConstraintSet::always(&builder).or(db, &builder, || {
            forced.set(forced.get() + 1);
            t_int
        });
        assert_eq!(forced.get(), 0);

        impossible.and(db, &builder, || {
            forced.set(forced.get() + 1);
            t_int
        });
        tautology.or(db, &builder, || {
            forced.set(forced.get() + 1);
            t_int
        });
        assert_eq!(forced.get(), 2);

        let visited = Cell::new(0);
        [impossible, t_int]
            .into_iter()
            .when_all(db, &builder, |set| {
                visited.set(visited.get() + 1);
                set
            });
        assert_eq!(visited.get(), 2);

        visited.set(0);
        [tautology, t_int]
            .into_iter()
            .when_any(db, &builder, |set| {
                visited.set(visited.get() + 1);
                set
            });
        assert_eq!(visited.get(), 2);

        visited.set(0);
        [ConstraintSet::never(&builder), t_int]
            .into_iter()
            .when_all(db, &builder, |set| {
                visited.set(visited.get() + 1);
                set
            });
        assert_eq!(visited.get(), 1);

        visited.set(0);
        [ConstraintSet::always(&builder), t_int]
            .into_iter()
            .when_any(db, &builder, |set| {
                visited.set(visited.get() + 1);
                set
            });
        assert_eq!(visited.get(), 1);
    }

    #[test]
    fn never_satisfied_results_are_cached() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let t_str = create_constraint(db, &builder, t, KnownClass::Str);
        let impossible = t_int.and(db, &builder, || t_str);

        assert!(!t_int.is_never_satisfied(db, &env));
        assert!(!t_int.is_never_satisfied(db, &env));
        assert!(impossible.is_never_satisfied(db, &env));
        assert!(impossible.is_never_satisfied(db, &env));
        assert!(ConstraintSet::never(&builder).is_never_satisfied(db, &env));
        assert!(!ConstraintSet::always(&builder).is_never_satisfied(db, &env));

        {
            let storage = builder.storage.borrow();
            assert_eq!(storage.never_satisfied_cache.get(&t_int.node), Some(&false));
            assert_eq!(
                storage.never_satisfied_cache.get(&impossible.node),
                Some(&true)
            );
            assert_eq!(storage.never_satisfied_cache.len(), 2);
        }

        let owned = create_compacted_owned_set(db);
        owned.query(|builder, set| {
            assert!(!set.is_never_satisfied(db, &env));
            assert!(!set.is_never_satisfied(db, &env));
            let storage = builder.storage.borrow();
            assert_eq!(storage.never_satisfied_cache.get(&set.node), Some(&false));
        });
    }

    #[test]
    fn never_satisfied_cache_is_shared_across_source_orders() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);

        let first = t_int.and(db, &builder, || u_str);
        let second = u_str.and(db, &builder, || t_int);

        assert_eq!(first.node, second.node);
        assert_ne!(first.source_order, second.source_order);
        assert!(!first.is_never_satisfied(db, &env));
        assert!(!second.is_never_satisfied(db, &env));
        let storage = builder.storage.borrow();
        assert_eq!(storage.never_satisfied_cache.len(), 1);
    }

    #[derive(Clone, Copy)]
    struct PermutedConstraint<'db>(
        BoundTypeVarInstance<'db>,
        ConstraintBound<'db>,
        ConstraintBound<'db>,
    );

    impl<'db> PermutedConstraint<'db> {
        fn node(
            self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            storage: &mut ConstraintSetStorage<'db>,
        ) -> NodeId {
            let PermutedConstraint(typevar, lower, upper) = self;
            let bounds = ConstraintBounds::new(Some(lower), Some(upper));
            Constraint::new_node_with_bounds(db, env, storage, typevar, bounds.lower, bounds.upper)
                .0
        }
    }

    /// Tests that we get the same set of solutions for a constraint set, regardless of the
    /// variable ordering that is chosen for its "atoms" (the raw constraints that the constraint
    /// set is built from).
    ///
    /// TODO: We _don't_ currently get a consistent result for each permutation. Right now,
    /// `expected` is a list of all of the different results that we get. Once we solve all of the
    /// sources of nondeterminism, `expected` should become a single string, and we should verify
    /// that we get that specific result for each permutation.
    #[track_caller]
    fn check_solutions_for_constraint_orderings<'db>(
        db: &'db TestDb,
        typevars: &[BoundTypeVarInstance<'db>],
        atoms: &[PermutedConstraint<'db>],
        build_bdd: impl Fn(&mut ConstraintSetStorage<'db>) -> NodeId,
        expected: impl IntoIterator<Item = &'static str>,
    ) {
        let env = db.program_environment();
        let inferable = TypeVarSet::from_typevars(db, typevars.iter().copied());
        let mut signatures = FxIndexSet::default();

        for constraint_order in (0..atoms.len()).permutations(atoms.len()) {
            let builder = ConstraintSetBuilder::new();
            let mut storage = builder.storage.borrow_mut();
            for typevar in typevars {
                storage.intern_typevar(db, *typevar);
            }
            for index in constraint_order {
                let PermutedConstraint(typevar, lower, upper) = atoms[index];
                storage.intern_constraint(
                    db,
                    &env,
                    Constraint {
                        typevar,
                        bounds: ConstraintBounds::new(Some(lower), Some(upper)),
                    },
                );
            }

            let node = build_bdd(&mut storage);
            let source_order = atoms.iter().fold(None, |source_order, atom| {
                let PermutedConstraint(typevar, lower, upper) = *atom;
                let constraint = storage.intern_constraint(
                    db,
                    &env,
                    Constraint {
                        typevar,
                        bounds: ConstraintBounds::new(Some(lower), Some(upper)),
                    },
                );
                let constraint_source_order = storage.constraint_source_order(constraint);
                storage.ordered_source_order(source_order, Some(constraint_source_order))
            });
            drop(storage);

            let set = ConstraintSet::from_node(&builder, node, source_order);
            let solutions = set.solutions(db, &env, inferable);
            let mut merged = FxHashMap::default();
            if let Ok(Solutions::Constrained(paths)) = &solutions {
                for path in paths.as_slice() {
                    for binding in path {
                        merged
                            .entry(binding.bound_typevar)
                            .and_modify(|existing| {
                                *existing = UnionType::from_two_elements(
                                    db,
                                    &env,
                                    *existing,
                                    binding.solution,
                                );
                            })
                            .or_insert(binding.solution);
                    }
                }
            }
            let merged = typevars
                .iter()
                .filter_map(|typevar| {
                    merged.get(typevar).map(|ty| {
                        format!(
                            "{}={}",
                            typevar.identity(db).display(db),
                            ty.display(db, &env)
                        )
                    })
                })
                .join(", ");
            let paths = match &solutions {
                Ok(Solutions::Unsatisfiable) => String::from("unsatisfiable"),
                Ok(Solutions::Unconstrained) => String::from("unconstrained"),
                Ok(Solutions::Constrained(paths)) => paths
                    .as_slice()
                    .iter()
                    .map(|path| {
                        path.iter()
                            .map(|binding| {
                                format!(
                                    "{}={}",
                                    binding.bound_typevar.identity(db).display(db),
                                    binding.solution.display(db, &env)
                                )
                            })
                            .join(", ")
                    })
                    .join("; "),
                Err(error) => format!("error: {error:?}"),
            };
            signatures.insert(format!(
                "never={} always={} merged=[{merged}] paths=[{paths}]",
                set.is_never_satisfied(db, &env),
                set.is_always_satisfied(db, &env),
            ));
        }

        let expected: FxIndexSet<_> = expected.into_iter().map(String::from).collect();
        assert_eq!(signatures, expected);
    }

    #[test]
    fn constraint_absorption_is_independent_of_constraint_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let str = KnownClass::Str.to_instance(db, &env);
        let int = KnownClass::Int.to_instance(db, &env);
        let atoms = [
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(str),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(int),
                ConstraintBound::missing_upper(),
            ),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t],
            &atoms,
            |storage| {
                let [str_t, int_t] = atoms.map(|atom| atom.node(db, &env, storage));
                str_t.or(storage, int_t).and(storage, str_t)
            },
            ["never=false always=false merged=[T=str] paths=[T=str]"],
        );

        check_solutions_for_constraint_orderings(
            db,
            &[t],
            &atoms,
            |storage| {
                let [str_t, int_t] = atoms.map(|atom| atom.node(db, &env, storage));
                str_t.or(storage, int_t)
            },
            ["never=false always=false merged=[T=str | int] paths=[T=str; T=int]"],
        );
    }

    #[test]
    fn compound_constraint_absorption_is_independent_of_constraint_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let str = KnownClass::Str.to_instance(db, &env);
        let bytes = KnownClass::Bytes.to_instance(db, &env);
        let int = KnownClass::Int.to_instance(db, &env);
        let atoms = [
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(str),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                u,
                ConstraintBound::Evidence(bytes),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(int),
                ConstraintBound::missing_upper(),
            ),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u],
            &atoms,
            |storage| {
                let [str_t, bytes_u, int_t] = atoms.map(|atom| atom.node(db, &env, storage));
                let compound = str_t.and(storage, bytes_u);
                compound.or(storage, int_t).and(storage, compound)
            },
            ["never=false always=false merged=[T=str, U=bytes] paths=[T=str, U=bytes]"],
        );
    }

    #[test]
    fn compound_constraint_absorption_preserves_binding_source_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let x = create_typevar(db, "X");
        let str = KnownClass::Str.to_instance(db, &env);
        let bytes = KnownClass::Bytes.to_instance(db, &env);
        let int = KnownClass::Int.to_instance(db, &env);
        let atoms = [
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(str),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                u,
                ConstraintBound::Evidence(bytes),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                x,
                ConstraintBound::Evidence(int),
                ConstraintBound::missing_upper(),
            ),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u, x],
            &atoms,
            |storage| {
                let [str_t, bytes_u, int_x] = atoms.map(|atom| atom.node(db, &env, storage));
                let early = int_x.and(storage, str_t).and(storage, bytes_u);
                let late = bytes_u.and(storage, str_t);
                early.or(storage, late)
            },
            ["never=false always=false merged=[T=str, U=bytes] paths=[T=str, U=bytes]"],
        );
    }

    #[test]
    fn constraint_partition_is_independent_of_constraint_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let str = KnownClass::Str.to_instance(db, &env);
        let int = KnownClass::Int.to_instance(db, &env);
        let atoms = [
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(str),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(int),
                ConstraintBound::missing_upper(),
            ),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t],
            &atoms,
            |storage| {
                let [str_t, int_t] = atoms.map(|atom| atom.node(db, &env, storage));
                let true_path = int_t.and(storage, str_t);
                let false_path = int_t.negate(storage).and(storage, str_t);
                true_path.or(storage, false_path)
            },
            ["never=false always=false merged=[T=str] paths=[T=str]"],
        );
    }

    #[test]
    fn constraint_ordering_preserves_nested_transitive_solutions() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let v = create_typevar(db, "V");
        let int = KnownClass::Int.to_instance(db, &env);
        let bytes = KnownClass::Bytes.to_instance(db, &env);
        let list_u = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(u)]);
        let list_int = KnownClass::List.to_specialized_instance(db, &env, &[int]);
        let atoms = [
            PermutedConstraint(
                t,
                ConstraintBound::missing_lower(),
                ConstraintBound::Evidence(list_u),
            ),
            PermutedConstraint(
                u,
                ConstraintBound::missing_lower(),
                ConstraintBound::Evidence(int),
            ),
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(list_int),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                v,
                ConstraintBound::Evidence(bytes),
                ConstraintBound::missing_upper(),
            ),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u, v],
            &atoms,
            |storage| {
                let [t_list_u, u_int, list_int_t, bytes_v] =
                    atoms.map(|atom| atom.node(db, &env, storage));
                t_list_u
                    .and(storage, u_int)
                    .and(storage, list_int_t)
                    .or(storage, bytes_v)
            },
            // The unrelated `V = bytes` alternative must not pick up bindings for `T` or `U`.
            [
                "never=false always=false merged=[T=list[int], U=int, V=bytes] paths=[T=list[int], U=int; V=bytes]",
            ],
        );
    }

    #[test]
    fn constraint_ordering_preserves_negated_alternative_solutions() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let int = KnownClass::Int.to_instance(db, &env);
        let str = KnownClass::Str.to_instance(db, &env);
        let bytes = KnownClass::Bytes.to_instance(db, &env);
        let atoms = [
            PermutedConstraint(
                t,
                ConstraintBound::missing_lower(),
                ConstraintBound::Evidence(int),
            ),
            PermutedConstraint(
                t,
                ConstraintBound::missing_lower(),
                ConstraintBound::Evidence(str),
            ),
            PermutedConstraint(
                u,
                ConstraintBound::Evidence(bytes),
                ConstraintBound::missing_upper(),
            ),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u],
            &atoms,
            |storage| {
                let [t_int, t_str, bytes_u] = atoms.map(|atom| atom.node(db, &env, storage));
                t_int
                    .or(storage, t_str)
                    .negate(storage)
                    .or(storage, bytes_u)
            },
            // A satisfied alternative must not infer `T` from unrelated positive decisions
            // made earlier in a TDD path.
            ["never=false always=false merged=[U=bytes] paths=[; U=bytes]"],
        );
    }

    #[test]
    fn constraint_ordering_preserves_independent_concrete_solutions() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let int = KnownClass::Int.to_instance(db, &env);
        let str = KnownClass::Str.to_instance(db, &env);
        let atoms = [
            PermutedConstraint(
                t,
                ConstraintBound::missing_lower(),
                ConstraintBound::Evidence(int),
            ),
            PermutedConstraint(
                t,
                ConstraintBound::missing_lower(),
                ConstraintBound::Evidence(str),
            ),
            PermutedConstraint(
                t,
                ConstraintBound::Evidence(int),
                ConstraintBound::missing_upper(),
            ),
            PermutedConstraint(
                u,
                ConstraintBound::missing_lower(),
                ConstraintBound::Evidence(int),
            ),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u],
            &atoms,
            |storage| {
                let [t_int, t_str, int_t, u_int] = atoms.map(|atom| atom.node(db, &env, storage));
                t_int
                    .or(storage, t_str)
                    .and(storage, int_t)
                    .and(storage, u_int)
            },
            ["never=false always=false merged=[T=int, U=int] paths=[T=int, U=int]"],
        );
    }

    #[track_caller]
    fn check_display_graph<'db, 'c>(
        db: &'db TestDb,
        builder: &'c ConstraintSetBuilder<'db>,
        set: ConstraintSet<'db, 'c>,
        expected: &str,
    ) {
        let env = db.program_environment();
        let storage = builder.storage.borrow();
        let expected = expected.trim_end();
        let actual = set.node.display_graph(db, &env, &storage, &"").to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_display_graph_output() {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let constraints = ConstraintSetBuilder::new();
        let t_str = create_constraint(db, &constraints, t, KnownClass::Str);
        let t_bool = create_constraint(db, &constraints, t, KnownClass::Bool);
        let u_str = create_constraint(db, &constraints, u, KnownClass::Str);
        let u_bool = create_constraint(db, &constraints, u, KnownClass::Bool);
        // Construct this in a different order than above to make the source_orders more
        // interesting.
        let set = (u_str.or(db, &constraints, || u_bool))
            .and(db, &constraints, || t_str.or(db, &constraints, || t_bool));
        check_display_graph(
            db,
            &constraints,
            set,
            indoc! {r#"
                <0> (U = bool)
                ┡━₁ <1> (T = bool)
                │   ┡━₁ always
                │   ├─? <2> (T = str)
                │   │   ┡━₁ always
                │   │   ├─? never
                │   │   └─₀ never
                │   └─₀ never
                ├─? <3> (U = str)
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
                <0> (T = int)
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
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();

        // Neither lhs nor rhs have uncertain branches (checked above). The operand with the
        // "lower" BDD variable (in this case, the lhs) is parked into a new uncertain branch in
        // the union result.
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let union = t_int.or(db, &builder, || u_str);
        check_display_graph(
            db,
            &builder,
            union,
            indoc! {r#"
                <0> (U = str)
                ┡━₁ always
                ├─? <1> (T = int)
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
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let t_bool = create_constraint(db, &builder, t, KnownClass::Bool);
        let u_int = create_constraint(db, &builder, u, KnownClass::Int);

        // lhs and rhs both have uncertain branches (checked above). These uncertain branches are
        // carried through to the intersection result.
        let lhs = t_int.or(db, &builder, || u_str);
        let rhs = t_bool.or(db, &builder, || u_int);
        let intersection = lhs.and(db, &builder, || rhs);
        check_display_graph(
            db,
            &builder,
            intersection,
            indoc! {r#"
                <0> (U = int)
                ┡━₁ <1> (U = str)
                │   ┡━₁ always
                │   ├─? <2> (T = int)
                │   │   ┡━₁ always
                │   │   ├─? never
                │   │   └─₀ never
                │   └─₀ never
                ├─? <3> (T = bool)
                │   ┡━₁ <1> SHARED
                │   ├─? never
                │   └─₀ never
                └─₀ never
            "#},
        );
    }

    #[test]
    fn tdd_uncertain_branch_absorbs_stronger_paths() {
        let db = setup_db();
        let db = &db;
        let builder = ConstraintSetBuilder::new();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let v = create_typevar(db, "V");
        let last = create_constraint(db, &builder, t, KnownClass::Int);
        let middle = create_constraint(db, &builder, u, KnownClass::Str);
        let first = create_constraint(db, &builder, v, KnownClass::Bytes);

        // The uncertain branch already accepts every assignment of the stronger guarded path,
        // whether that path requires or excludes the first constraint.
        for guard in [first, first.negate(db, &builder)] {
            let stronger = guard
                .and(db, &builder, || middle)
                .and(db, &builder, || last);
            let absorbed = stronger.or(db, &builder, || middle);
            assert_eq!(absorbed.node, middle.node);
        }
    }

    #[test]
    fn disjunction_of_independent_conjunctions_stays_compact() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let count = 12;
        let atoms = |prefix| {
            (0..count)
                .rev()
                .map(|index| {
                    let typevar = BoundTypeVarInstance::synthetic(
                        db,
                        &env,
                        Name::new(format!("{prefix}{index}")),
                        TypeVarVariance::Invariant,
                    );
                    create_constraint(db, &builder, typevar, KnownClass::Int)
                })
                .collect::<Vec<_>>()
        };
        // Place all X conditions before all Y conditions in the TDD ordering. The disjunction
        // (X0 ∧ Y0) ∨ … ∨ (Xn ∧ Yn) has a small diagram without distributing its alternatives.
        let y = atoms("Y");
        let x = atoms("X");
        let mut groups: Vec<_> = x
            .into_iter()
            .zip(y)
            .rev()
            .map(|(x, y)| x.and(db, &builder, || y))
            .collect();
        while groups.len() > 1 {
            groups = groups
                .chunks(2)
                .map(|pair| {
                    let left = pair[0];
                    pair.get(1)
                        .map_or(left, |right| left.or(db, &builder, || *right))
                })
                .collect();
        }
        let nodes = builder.storage.borrow().nodes.len();
        assert!(nodes < 4 * count * count, "allocated {nodes} nodes");
    }

    /// Negation always produces flat TDDs (all uncertain branches are `ALWAYS_FALSE`).
    #[test]
    fn tdd_negation_produces_flat_tdd() {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let union = t_int.or(db, &builder, || u_str);
        let negated = union.negate(db, &builder);
        check_display_graph(
            db,
            &builder,
            negated,
            indoc! {r#"
                <0> (U = str)
                ┡━₁ never
                ├─? never
                └─₀ <1> (T = int)
                    ┡━₁ never
                    ├─? never
                    └─₀ always
            "#},
        );
    }

    #[test]
    fn tdd_negation_correctness() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();

        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let tdd = t_int.or(db, &builder, || u_str);
        let negated = tdd.negate(db, &builder);

        // T ∧ ¬T == false
        assert!(
            tdd.and(db, &builder, || negated)
                .is_never_satisfied(db, &env)
        );

        // T ∨ ¬T == true
        assert!(
            tdd.or(db, &builder, || negated)
                .is_always_satisfied(db, &env)
        );
    }

    /// Double negation of a TDD with uncertain branches is semantically equivalent to the
    /// original (though the structure may differ since negation produces flat TDDs).
    #[test]
    fn tdd_double_negation() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let tdd = t_int.or(db, &builder, || u_str);
        let negated = tdd.negate(db, &builder);
        let double_negated = negated.negate(db, &builder);
        let equivalent = tdd.iff(db, &builder, double_negated);
        assert!(equivalent.is_always_satisfied(db, &env));
    }

    /// `iff(T, T)` is always satisfied for TDDs with uncertain branches.
    #[test]
    fn tdd_iff_self() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let tdd = t_int.or(db, &builder, || u_str);

        // iff(T, T) == true
        assert!(tdd.iff(db, &builder, tdd).is_always_satisfied(db, &env));

        // iff(T, ¬T) == false
        let negated = tdd.negate(db, &builder);
        assert!(tdd.iff(db, &builder, negated).is_never_satisfied(db, &env));
    }

    #[test]
    fn constraint_set_source_order_combination_is_idempotent() {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let t_int = create_constraint(db, &builder, t, KnownClass::Int);
        let u_str = create_constraint(db, &builder, u, KnownClass::Str);
        let combined = t_int.and(db, &builder, || u_str);

        let t_bool = create_constraint(db, &builder, t, KnownClass::Bool);
        let u_int = create_constraint(db, &builder, u, KnownClass::Int);
        let alternatives = t_int
            .and(db, &builder, || u_int)
            .or(db, &builder, || u_str.and(db, &builder, || t_bool));

        for original in [t_int, combined, alternatives] {
            let storage = builder.storage.borrow();
            let original_source_order_count = storage.source_orders.len();
            drop(storage);
            let intersection = original.and(db, &builder, || original);
            let union = original.or(db, &builder, || original);

            assert_eq!(intersection.node, original.node);
            assert_eq!(intersection.source_order, original.source_order);
            assert_eq!(union.node, original.node);
            assert_eq!(union.source_order, original.source_order);
            let storage = builder.storage.borrow();
            assert_eq!(storage.source_orders.len(), original_source_order_count);
        }
    }

    #[test]
    fn shared_source_order_subtrees_are_visited_once() {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let mut left = create_constraint(db, &builder, t, KnownClass::Int);
        let mut right = create_constraint(db, &builder, t, KnownClass::Str);
        let expected = {
            let storage = builder.storage.borrow();
            [left.node, right.node].map(|node| storage.interior_node_data(node).constraint)
        };
        let original = left.or(db, &builder, || right);

        // The TDD stops growing, but each sidecar shares both of its predecessors. Walking the
        // sidecar as a tree would take exponentially many steps.
        for _ in 0..63 {
            let next = left.or(db, &builder, || right);
            left = right;
            right = next;
        }
        assert_eq!(right.node, original.node);
        assert_eq!(
            builder
                .storage
                .borrow()
                .calculate_source_orders(right.source_order)
                .into_iter()
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn deeply_nested_source_order_preserves_first_occurrences() {
        let mut storage = ConstraintSetStorage::default();
        let first = ConstraintId::from_usize(0);
        let second = ConstraintId::from_usize(1);
        let first_order = storage.constraint_source_order(first);
        let second_order = storage.constraint_source_order(second);
        let mut source_order = storage.ordered_source_order(Some(second_order), Some(first_order));

        // Appending a repeated leaf creates a deep left spine without changing the order. The
        // first occurrence of `second` is in the left subtree, not the right leaf at the root.
        for _ in 0..32_768 {
            source_order = storage.ordered_source_order(source_order, Some(second_order));
        }
        assert_eq!(
            storage
                .calculate_source_orders(source_order)
                .into_iter()
                .collect::<Vec<_>>(),
            [second, first]
        );
    }

    #[test]
    fn owned_constraint_set_typevar_order_survives_round_trip() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = Type::TypeVar(create_typevar(db, "U"));

        for (lower, upper) in [
            (Some(ConstraintBound::Evidence(u)), None),
            (None, Some(ConstraintBound::Evidence(u))),
            (
                Some(ConstraintBound::Evidence(u)),
                Some(ConstraintBound::Evidence(u)),
            ),
        ] {
            let original = ConstraintSetBuilder::new().into_owned(|builder| {
                ConstraintSet::constrain_typevar_with_bounds(db, &env, builder, t, lower, upper)
            });
            let mut reloaded = original.clone();

            for _ in 0..3 {
                reloaded = ConstraintSetBuilder::new()
                    .into_owned(|builder| builder.load(db, &env, &reloaded));
                assert_eq!(original, reloaded);
            }
        }
    }

    #[test]
    fn owned_constraint_set_load_discards_unreferenced_typevars() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let unused = create_typevar(db, "Unused");
        let u = create_typevar(db, "U");

        let original = ConstraintSetBuilder::new().into_owned(|builder| {
            let _unused_t_int = create_constraint(db, builder, t, KnownClass::Int);
            let _unused_str = create_constraint(db, builder, unused, KnownClass::Str);
            ConstraintSet::constrain_typevar_upper_bound(db, &env, builder, t, Type::TypeVar(u))
        });
        let reloaded =
            ConstraintSetBuilder::new().into_owned(|builder| builder.load(db, &env, &original));

        assert_eq!(
            reloaded
                .inner
                .as_ref()
                .map(|inner| inner.typevars.iter().copied().collect::<Vec<_>>()),
            Some(vec![t, u]),
        );
        let reloaded_again =
            ConstraintSetBuilder::new().into_owned(|builder| builder.load(db, &env, &reloaded));
        assert_eq!(reloaded, reloaded_again);
    }

    #[test]
    fn owned_constraint_set_load_preserves_overlay_typevar_ids() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let source = ConstraintSetBuilder::new().into_owned(|builder| {
            ConstraintSet::constrain_typevar_upper_bound(db, &env, builder, t, Type::TypeVar(u))
        });
        let destination = ConstraintSetBuilder::new()
            .into_owned(|builder| create_constraint(db, builder, u, KnownClass::Int));

        destination.query(|builder, _| {
            let original_u_id = builder.storage.borrow_mut().typevar_id(db, u);
            let loaded = builder.load(db, &env, &source);
            let direct = ConstraintSet::constrain_typevar_upper_bound(
                db,
                &env,
                builder,
                t,
                Type::TypeVar(u),
            );
            assert!(
                loaded
                    .iff(db, builder, direct)
                    .is_always_satisfied(db, &env)
            );

            let mut storage = builder.storage.borrow_mut();
            assert_eq!(storage.typevar_id(db, u), original_u_id);
            assert_eq!(storage.typevar_id(db, t).index(), 1);
        });
    }

    fn create_compacted_owned_set(db: &TestDb) -> OwnedConstraintSet<'_> {
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let v = create_typevar(db, "V");

        ConstraintSetBuilder::new().into_owned(|builder| {
            let _unused_t_int = create_constraint(db, builder, t, KnownClass::Int);
            let _unused_u_str = create_constraint(db, builder, u, KnownClass::Str);
            create_constraint(db, builder, v, KnownClass::Bool)
        })
    }

    #[test]
    fn owned_constraint_set_compacts_unreachable_storage() {
        let db = setup_db();
        let owned = create_compacted_owned_set(&db);
        let inner = owned
            .inner
            .as_ref()
            .expect("nonterminal root should retain storage");

        assert_eq!(owned.node.index(), 2);
        assert_eq!(owned.source_order.map(SourceOrderId::index), Some(0));
        assert_eq!(inner.nodes.len(), 1);
        assert_eq!(inner.constraints.len(), 1);
        assert_eq!(inner.source_orders.len(), 1);
        assert_eq!(inner.node_indices.len(), 3);
        assert_eq!(inner.constraint_indices.len(), 3);
        assert_eq!(inner.node_indices.iter_ones().collect::<Vec<_>>(), vec![2]);
        assert_eq!(
            inner.constraint_indices.iter_ones().collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(inner.typevars.len(), 3);
        assert!(owned.node.index() >= inner.nodes.len());
    }

    #[test]
    fn owned_constraint_set_discards_unrelated_quantified_constraints() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");

        let owned = ConstraintSetBuilder::new().into_owned(|builder| {
            let t_int = create_constraint(db, builder, t, KnownClass::Int);
            let u_str = create_constraint(db, builder, u, KnownClass::Str);
            t_int.and(db, builder, || u_str).reduce_inferable(
                db,
                &env,
                builder,
                TypeVarSet::from_typevars(db, [t]),
            )
        });

        assert_eq!(
            owned
                .types()
                .filter_map(Type::as_typevar)
                .collect::<Vec<_>>(),
            vec![u],
        );
        assert_eq!(
            owned.inner.as_ref().map(|inner| inner.source_orders.len()),
            Some(1),
        );
    }

    #[test]
    fn owned_constraint_set_preserves_projected_solution_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let inferable = TypeVarSet::from_typevars(db, [u]);
        let expected = Ok(Solutions::Constrained(SolutionPaths::Complete(vec![
            vec![TypeVarSolution {
                bound_typevar: u,
                solution: known_instance(db, KnownClass::Int),
            }],
            vec![TypeVarSolution {
                bound_typevar: u,
                solution: known_instance(db, KnownClass::Str),
            }],
        ])));

        let owned = ConstraintSetBuilder::new().into_owned(|builder| {
            let u_t = ConstraintSet::constrain_typevar(
                db,
                &env,
                builder,
                u,
                Type::TypeVar(t),
                Type::TypeVar(t),
            );
            let t_str = create_constraint(db, builder, t, KnownClass::Str);
            let u_int = create_constraint(db, builder, u, KnownClass::Int);

            // Eliminating T leaves a derived U = str alternative alongside the direct U = int.
            let projected = u_t
                .and(db, builder, || t_str)
                .or(db, builder, || u_int)
                .reduce_inferable(db, &env, builder, TypeVarSet::from_typevars(db, [t]));
            assert_eq!(projected.solutions(db, &env, inferable), expected);
            projected
        });

        let reloaded =
            ConstraintSetBuilder::new().into_owned(|builder| builder.load(db, &env, &owned));
        for constraints in [&owned, &reloaded] {
            constraints.query(|_builder, constraints| {
                assert_eq!(constraints.solutions(db, &env, inferable), expected);
            });
        }

        let reloaded_again =
            ConstraintSetBuilder::new().into_owned(|builder| builder.load(db, &env, &reloaded));
        assert_eq!(reloaded, reloaded_again);
    }

    #[test]
    fn projected_constraint_source_order_is_independent_of_allocation_order() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let a = create_typevar(db, "A");
        let r = create_typevar(db, "R");
        let fresh_a = create_typevar(db, "FreshA");
        let fresh_r = create_typevar(db, "FreshR");
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);
        let atoms = [
            (fresh_r, Some(int), None),
            (fresh_a, None, Some(int)),
            (fresh_r, Some(str), None),
            (fresh_a, None, Some(str)),
            (r, Some(Type::TypeVar(fresh_r)), None),
            (a, None, Some(Type::TypeVar(fresh_a))),
        ];

        let project = |allocation_order: &[usize]| {
            let builder = ConstraintSetBuilder::new();
            // Keep typevar orientation fixed while changing only the TDD variable order.
            for typevar in [fresh_r, fresh_a, a, r] {
                builder.storage.borrow_mut().intern_typevar(db, typevar);
            }
            let atom = |index: usize| {
                let (typevar, lower, upper) = atoms[index];
                ConstraintSet::constrain_typevar_with_bounds(
                    db,
                    &env,
                    &builder,
                    typevar,
                    lower.map(ConstraintBound::Evidence),
                    upper.map(ConstraintBound::Evidence),
                )
            };
            for &index in allocation_order {
                let _ = atom(index);
            }
            let [int_r, a_int, str_r, a_str, r_bound, a_bound] = [0, 1, 2, 3, 4, 5].map(atom);

            // Eliminating FreshA and FreshR leaves both bounds of the int and str alternatives
            // on A and R. All original constraints disappear, but their source order survives.
            let projected = int_r
                .and(db, &builder, || a_int)
                .or(db, &builder, || str_r.and(db, &builder, || a_str))
                .and(db, &builder, || r_bound)
                .and(db, &builder, || a_bound)
                .reduce_inferable(
                    db,
                    &env,
                    &builder,
                    TypeVarSet::from_typevars(db, [fresh_a, fresh_r]),
                );
            let storage = builder.storage.borrow();
            storage
                .calculate_source_orders(projected.source_order)
                .into_iter()
                .map(|constraint| storage.constraint_data(constraint))
                .collect::<Vec<_>>()
        };

        let expected = project(&[0, 1, 2, 3, 4, 5]);
        for allocation_order in (0..6).permutations(6) {
            assert_eq!(
                project(&allocation_order),
                expected,
                "allocation order {allocation_order:?}"
            );
        }
    }

    #[test]
    fn owned_constraint_set_source_order_ignores_construction_history() {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");

        let build = |include_redundant_combination| {
            ConstraintSetBuilder::new().into_owned(|builder| {
                let t_int = create_constraint(db, builder, t, KnownClass::Int);
                let u_str = create_constraint(db, builder, u, KnownClass::Str);
                let combined = t_int.and(db, builder, || u_str);

                if include_redundant_combination {
                    // Repeating one constraint leaves the BDD and first-occurrence source order
                    // unchanged, but creates a distinct, reachable source-order tree. Both trees
                    // must compact to the same owned set.
                    let redundant = combined.and(db, builder, || t_int);
                    assert_eq!(redundant.node, combined.node);
                    assert_ne!(redundant.source_order, combined.source_order);
                    redundant
                } else {
                    combined
                }
            })
        };

        assert_eq!(build(false), build(true));
    }

    #[test]
    fn owned_constraint_set_preserves_order_when_reintroducing_constraints() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let inferable = TypeVarSet::from_typevars(db, [t]);
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);

        // Absorption can remove a constraint without eliminating its typevar. Its source order
        // still matters if it is reintroduced, including when gradual bounds affect the solutions.
        for (lower, upper) in [(Some(str), Some(str)), (None, Some(Type::any()))] {
            let mut expected = None;
            let owned = ConstraintSetBuilder::new().into_owned(|builder| {
                let earlier =
                    ConstraintSet::constrain_typevar_lower_bound(db, &env, builder, t, int);
                let later = ConstraintSet::constrain_typevar_with_bounds(
                    db,
                    &env,
                    builder,
                    t,
                    lower.map(ConstraintBound::Evidence),
                    upper.map(ConstraintBound::Evidence),
                );
                let absorbed = earlier.or(db, builder, || later).and(db, builder, || later);
                expected = Some(
                    absorbed
                        .or(db, builder, || earlier)
                        .solutions(db, &env, inferable),
                );
                absorbed
            });
            assert_matches!(&expected, Some(Ok(Solutions::Constrained(_))));

            let builder = ConstraintSetBuilder::new();
            let reloaded = builder.load(db, &env, &owned);
            let earlier = ConstraintSet::constrain_typevar_lower_bound(db, &env, &builder, t, int);
            assert_eq!(
                Some(
                    reloaded
                        .or(db, &builder, || earlier)
                        .solutions(db, &env, inferable),
                ),
                expected,
            );
        }
    }

    #[test]
    fn owned_constraint_set_query_reads_compacted_overlay() {
        let db = setup_db();
        let owned = create_compacted_owned_set(&db);

        owned.query(|builder, set| {
            check_display_graph(
                &db,
                builder,
                set,
                indoc! {r#"
                    <0> (V = bool)
                    ┡━₁ always
                    ├─? never
                    └─₀ never
                "#},
            );

            let storage = builder.storage.borrow();
            assert!(storage.compacted.is_some());
            assert!(storage.nodes.is_empty());
            assert!(storage.constraints.is_empty());
            assert!(storage.typevars.is_empty());
        });
    }

    #[test]
    fn owned_constraint_set_mutating_query_allocates_after_overlay() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let owned = create_compacted_owned_set(db);

        owned.query(|builder, set| {
            let (node_split, constraint_split, typevar_split, source_order_split) = {
                let storage = builder.storage.borrow();
                let compacted = storage
                    .compacted
                    .as_ref()
                    .expect("query builder should have compacted storage");
                (
                    compacted.node_indices.len(),
                    compacted.constraint_indices.len(),
                    compacted.typevars.len(),
                    compacted.source_orders.len(),
                )
            };

            let mut storage = builder.storage.borrow_mut();
            let existing_constraint = storage.interior_node_data(set.node).constraint;
            assert_eq!(
                Some(storage.constraint_source_order(existing_constraint)),
                set.source_order
            );
            drop(storage);

            let w = create_typevar(db, "W");
            let w_str = create_constraint(db, builder, w, KnownClass::Str);
            let mut storage = builder.storage.borrow_mut();
            let new_constraint = w_str
                .node
                .root_constraint(&storage)
                .expect("new constraint should be nonterminal");

            assert!(w_str.node.index() >= node_split);
            assert!(new_constraint.index() >= constraint_split);
            assert!(storage.typevar_id(db, w).index() >= typevar_split);
            drop(storage);
            assert!(
                w_str
                    .source_order
                    .is_some_and(|source_order| source_order.index() >= source_order_split)
            );

            let combined = set.and(db, builder, || w_str);
            assert!(!combined.is_never_satisfied(db, &env));

            let storage = builder.storage.borrow();
            assert!(!storage.nodes.is_empty());
            assert!(!storage.constraints.is_empty());
            assert!(!storage.typevars.is_empty());
        });
    }

    #[test]
    fn owned_constraint_set_load_reads_compacted_storage() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let owned = create_compacted_owned_set(db);

        let builder = ConstraintSetBuilder::new();
        let loaded = builder.load(db, &env, &owned);
        check_display_graph(
            db,
            &builder,
            loaded,
            indoc! {r#"
                <0> (V = bool)
                ┡━₁ always
                ├─? never
                └─₀ never
            "#},
        );
    }

    #[test]
    fn terminal_owned_constraint_set_discards_storage() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let owned = ConstraintSetBuilder::new().into_owned(|builder| {
            let _unused = create_constraint(db, builder, t, KnownClass::Int);
            ConstraintSet::always(builder)
        });

        assert!(owned.inner.is_none());

        owned.query(|builder, set| {
            assert!(set.is_always_satisfied(db, &env));
            let storage = builder.storage.borrow();
            assert!(storage.compacted.is_none());
            assert!(storage.nodes.is_empty());
            assert!(storage.constraints.is_empty());
            assert!(storage.typevars.is_empty());
        });

        let builder = ConstraintSetBuilder::new();
        let loaded = builder.load(db, &env, &owned);
        assert!(loaded.is_always_satisfied(db, &env));
    }

    /// Round-trip through `OwnedConstraintSet`: build a TDD with uncertain branches, convert to
    /// owned, load into a new builder, and verify that we preserve the uncertain branch.
    #[test]
    fn tdd_owned_round_trip() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");

        // Build a TDD with uncertain branches and convert to owned
        let builder = ConstraintSetBuilder::new();
        let owned = builder.into_owned(|builder| {
            let t_int = create_constraint(db, builder, t, KnownClass::Int);
            let u_str = create_constraint(db, builder, u, KnownClass::Str);
            let result = t_int.or(db, builder, || u_str);
            check_display_graph(
                db,
                builder,
                result,
                indoc! {r#"
                    <0> (U = str)
                    ┡━₁ always
                    ├─? <1> (T = int)
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
        let loaded = builder.load(db, &env, &owned);
        check_display_graph(
            db,
            &builder,
            loaded,
            indoc! {r#"
                <0> (U = str)
                ┡━₁ always
                ├─? <1> (T = int)
                │   ┡━₁ always
                │   ├─? never
                │   └─₀ never
                └─₀ never
            "#},
        );
    }
}
