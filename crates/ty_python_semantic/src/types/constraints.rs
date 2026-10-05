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

#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::convert::Infallible;
use std::fmt::{Debug, Display};
use std::iter;
use std::marker::PhantomData;
use std::ops::{ControlFlow, Range};
use std::sync::{Arc, LazyLock};

#[cfg(test)]
use itertools::Itertools;
use ruff_index::{Idx, IndexVec, newtype_index};
use rustc_hash::{FxHashMap, FxHashSet};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::{QuoteError, QuoteFuel};
use smallvec::SmallVec;
use ty_python_core::Program;
use ty_python_core::rank::RankBitBox;
use ty_static::EnvVars;

use crate::types::constraints::control::{
    AllocationKind, PathAdvance, PathTable, PathTypevarSet, PathWork, TddControl, TddError,
    TddWork, Unrestricted, admit_path_work, receiver_seen_storage, reserve_smallvec,
    reserve_vec, sequence_growth, unrestricted,
};
use crate::types::constraints::projection::{ProjectionError, SolutionBudget};
use crate::types::constraints::satisfaction::{
    OrdinarySatisfaction, SatisfactionKind, node_satisfaction_sync, path_assignments_sync,
};
use crate::types::constraints::support::{Support, SupportId};
#[cfg(all(test, feature = "experimental-analysis"))]
use crate::types::infer::legacy_callable_observations;
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarSet};
use crate::types::visitor::{
    OrdinaryTypeWalk, SyncTypeSupportEffects, TypeWalkFacts, Unrestricted as UnrestrictedWalk,
    support_type_sync,
};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, IntersectionType, Type, TypeContext,
    TypeMapping, TypePair, TypeVarBoundOrConstraints, TypeVarVariance, UnionType,
};
use crate::{Db, FxIndexMap, FxIndexSet, FxOrderSet, ProgramEnvironment};

mod apply;
mod combination;
pub(in crate::types) mod control;
mod fold;
pub(crate) mod paths;
pub(crate) mod projection;
pub(crate) mod resolution;
#[cfg(test)]
pub(in crate::types) mod runtime;
mod satisfaction;
mod sequents;
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod source;
#[cfg(test)]
pub(in crate::types) use self::sequents::runtime::UnsupportedSequentOperation;
mod solutions;
mod source_order;
mod storage;
mod support;
mod type_analysis;
#[cfg(test)]
pub(in crate::types) mod typevar_equivalence;
mod variables;

#[cfg(test)]
mod fold_probe;
#[cfg(test)]
mod scheduling_probe;
#[cfg(test)]
pub(crate) use scheduling_probe::ConstraintHandleKey;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(super) use combination::ConstraintCombination;
pub(super) use fold::{ConstraintFold, ConstraintFoldKind};

use paths::PathAssignments;
use solutions::SolutionWalker;
use variables::{Constraint, ConstraintProvenance};

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

/// A constraint result that holds unconditionally or cannot hold, without stored conditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum TerminalConstraint {
    Always,
    Never,
}

impl TerminalConstraint {
    const fn node(self) -> NodeId {
        match self {
            Self::Always => ALWAYS_TRUE,
            Self::Never => ALWAYS_FALSE,
        }
    }
}

/// A query builder paired with the root whose compacted arenas it shares.
pub(in crate::types) struct OwnedConstraintSetQuery<'db> {
    builder: ConstraintSetBuilder<'db>,
    node: NodeId,
    source_order: Option<SourceOrderId>,
}

impl<'db> OwnedConstraintSetQuery<'db> {
    pub(in crate::types) fn parts(&self) -> (&ConstraintSetBuilder<'db>, ConstraintSet<'db, '_>) {
        (
            &self.builder,
            ConstraintSet::from_node(&self.builder, self.node, self.source_order),
        )
    }

    #[cfg(test)]
    pub(in crate::types) fn ownership_probe_matches(
        &self,
        owned: &OwnedConstraintSet<'db>,
    ) -> bool {
        let storage = self.builder.storage.borrow();
        self.node == owned.node
            && self.source_order == owned.source_order
            && match (&storage.compacted, &owned.inner) {
                (Some(actual), Some(expected)) => Arc::ptr_eq(actual, expected),
                (None, None) => true,
                _ => false,
            }
    }
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

impl OwnedConstraintSetInner<'_> {
    #[cfg(any(test, feature = "experimental-analysis"))]
    fn retirement_work(&self) -> Option<usize> {
        // into_owned retains the complete typevar arena. Support insertion uses an interned
        // typevar's word index, and merging only takes the maximum source length, so this
        // bounds every support without scanning their SmallVecs before retirement admission.
        let word_bits = usize::BITS as usize;
        let support_words = (self.typevars.len() / word_bits)
            .checked_add(usize::from(self.typevars.len() % word_bits != 0))?;
        let support_word_work = self.supports.len().checked_mul(support_words)?;

        // Quote possible final-Arc destruction even when another owner currently exists.
        // RankBitBox owns bits and chunk ranks; twice its bit length bounds both arrays.
        // Spare collection capacity has no initialized elements to destroy. The fixed work
        // covers the outer ownership and inner containers, not allocator latency.
        [
            self.constraints.len(),
            self.constraint_supports.len(),
            self.typevars.len(),
            self.nodes.len(),
            self.node_supports.len(),
            self.supports.len(),
            self.source_orders.len(),
            self.constraint_indices.len().checked_mul(2)?,
            self.node_indices.len().checked_mul(2)?,
            self.support_indices.len().checked_mul(2)?,
            support_word_work,
        ]
        .into_iter()
        .try_fold(12usize, usize::checked_add)
    }
}

impl<'db> OwnedConstraintSet<'db> {
    /// Classifies a terminal only when no backing graph or source-order data is retained.
    pub(in crate::types) const fn source_free_terminal(&self) -> Option<TerminalConstraint> {
        if self.inner.is_none() && self.source_order.is_none() {
            self.terminal()
        } else {
            None
        }
    }

    /// Classifies a terminal root without evaluating any stored type-variable conditions.
    pub(in crate::types) const fn terminal(&self) -> Option<TerminalConstraint> {
        match self.node {
            ALWAYS_TRUE => Some(TerminalConstraint::Always),
            ALWAYS_FALSE => Some(TerminalConstraint::Never),
            _ => None,
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn retirement_work(&self) -> Option<usize> {
        self.inner
            .as_deref()
            .map_or(Some(1), OwnedConstraintSetInner::retirement_work)
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn field_work_with(
        &self,
        admit: &mut impl FnMut() -> Result<(), QuoteError>,
    ) -> Result<usize, QuoteError> {
        admit()?;
        let mut work = self.retirement_work().ok_or(QuoteError::Overflow)?;
        if let Some(inner) = &self.inner {
            // Hash/Eq include the complete retained arena, including constraints kept only
            // for source ordering. The semantic `types()` iterator omits those entries.
            for constraint in &inner.constraints {
                admit()?;
                for ty in constraint.type_pair() {
                    work = work
                        .checked_add(ty.inline_payload_bytes())
                        .ok_or(QuoteError::Overflow)?;
                }
            }
        }
        Ok(work)
    }

    /// Visits the native equality payload, including constraints retained only for source order.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn native_comparison_type_pairs(
        &self,
    ) -> impl ExactSizeIterator<Item = [Type<'db>; 2]> + '_ {
        let constraints = self
            .inner
            .as_deref()
            .map_or([].as_slice(), |inner| inner.constraints.as_ref());
        constraints.iter().map(|constraint| constraint.type_pair())
    }

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
        let view = self.query_view();
        let (builder, set) = view.parts();
        f(builder, set)
    }

    pub(in crate::types) fn query_view(&self) -> OwnedConstraintSetQuery<'db> {
        OwnedConstraintSetQuery {
            builder: ConstraintSetBuilder {
                storage: RefCell::new(self.query_storage()),
            },
            node: self.node,
            source_order: self.source_order,
        }
    }

    fn query_storage(&self) -> ConstraintSetStorage<'db> {
        ConstraintSetStorage {
            compacted: self.inner.clone(),
            ..ConstraintSetStorage::default()
        }
    }

    /// Returns the typevars and stored bound types still reachable from the decision diagram.
    ///
    /// Source ordering can retain constraints that are no longer in the diagram, but their type
    /// variables must not participate in semantic walks or callable freshening.
    /// Synthetic defaults are not stored types and must not affect these walks either.
    pub(crate) fn types(&self) -> impl Iterator<Item = Type<'db>> + '_ {
        self.type_steps().flatten().flatten()
    }

    /// Advances one retained decision node, including nodes with an already visited constraint.
    /// Keeping skipped steps visible lets interruptible walks account for duplicate scanning.
    pub(super) fn type_steps(&self) -> impl Iterator<Item = Option<[Type<'db>; 2]>> + '_ {
        let mut cursor = OwnedConstraintTypeCursor::new(self);
        std::iter::from_fn(move || unrestricted(cursor.next_with(&mut Unrestricted)))
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct OwnedConstraintSetProfile;

#[cfg(any(test, feature = "experimental-analysis"))]
impl<C> salsa::execution_probe::PassiveMemoProfile<C> for OwnedConstraintSetProfile
where
    C: for<'db> salsa::plumbing::function::Configuration<Output<'db> = OwnedConstraintSet<'db>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output.retirement_work()
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        output.retirement_work().ok_or(QuoteError::Overflow)
    }
}

/// Scalar cursor state for admission and retirement observations without retaining the cursor.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct ReceiverCursorState {
    pub(in crate::types) next: usize,
    pub(in crate::types) seen_len: usize,
    pub(in crate::types) seen_capacity: usize,
    #[cfg(all(test, feature = "experimental-analysis"))]
    pub(in crate::types) observation_id: Option<usize>,
}

/// A retained-node cursor whose duplicate checks and growth remain explicit to callers.
#[derive(Debug)]
pub(in crate::types) struct OwnedConstraintTypeCursor<'a, 'db> {
    inner: Option<&'a OwnedConstraintSetInner<'db>>,
    next: usize,
    seen: FxHashSet<ConstraintId>,
    #[cfg(all(test, feature = "experimental-analysis"))]
    observation_id: Option<usize>,
}

impl<'a, 'db> OwnedConstraintTypeCursor<'a, 'db> {
    pub(in crate::types) fn new(set: &'a OwnedConstraintSet<'db>) -> Self {
        Self {
            inner: set.inner.as_deref(),
            next: 0,
            seen: FxHashSet::default(),
            #[cfg(all(test, feature = "experimental-analysis"))]
            observation_id: legacy_callable_observations::receiver_cursor_created(),
        }
    }

    /// Reports the cursor's position and seen-table size without transferring its ownership.
    /// Experimental-analysis test builds include an optional ID to match this state with the
    /// same cursor's destructor entry across moves.
    #[cfg(test)]
    pub(in crate::types) fn state(&self) -> ReceiverCursorState {
        ReceiverCursorState {
            next: self.next,
            seen_len: self.seen.len(),
            seen_capacity: self.seen.capacity(),
            #[cfg(all(test, feature = "experimental-analysis"))]
            observation_id: self.observation_id,
        }
    }

    /// Visits one retained node and yields its stored type pair on the first visit to its
    /// constraint. `Some(None)` marks a repeated constraint; outer `None` marks completion.
    /// A refusal preserves the cursor's position and seen table, without refunding any earlier
    /// accepted admission for the step.
    pub(in crate::types) fn next_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<Option<Option<[Type<'db>; 2]>>, control::TddError<C::Error>> {
        let Some(inner) = self.inner else {
            return Ok(None);
        };
        let Some(node) = inner.nodes.get(self.next) else {
            return Ok(None);
        };
        control.admit(TddWork::Advance)?;
        let constraint = node.constraint;
        if self.seen.contains(&constraint) {
            self.next += 1;
            return Ok(Some(None));
        }
        if self.seen.len() == self.seen.capacity() {
            let required = self
                .seen
                .len()
                .checked_add(1)
                .ok_or(control::TddError::CapacityExhausted)?;
            let mut plan =
                sequence_growth::<ConstraintId, C::Error>(self.seen.capacity(), required)?;
            plan.relocation_units = self.seen.len();
            let storage = receiver_seen_storage::<C::Error>(self.seen.capacity(), plan)?;
            control.admit(TddWork::Grow {
                allocation: AllocationKind::TypeWalkReceiverSeen,
                plan,
            })?;
            control.admit(storage)?;
            self.seen.reserve(plan.requested_capacity - self.seen.len());
        }
        self.seen.insert(constraint);
        self.next += 1;
        Ok(Some(Some(
            inner.constraints[inner.retained_constraint_index(constraint)].type_pair(),
        )))
    }
}

#[cfg(all(test, feature = "experimental-analysis"))]
impl Drop for OwnedConstraintTypeCursor<'_, '_> {
    fn drop(&mut self) {
        // Fields are dropped after this callback; this observes destructor entry, not completed
        // backing deallocation. The observer stores only the ID and scalar state.
        legacy_callable_observations::receiver_cursor_drop(self.observation_id, self.state());
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
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn is_source_free_terminal(self) -> bool {
        self.node.is_terminal() && self.source_order.is_none()
    }

    /// Packages a terminal result without consuming the builder that produced it.
    /// Nonterminal results require their reachable arenas to be compacted as well.
    pub(in crate::types) fn to_owned_terminal(self) -> Option<OwnedConstraintSet<'db>> {
        self.node.is_terminal().then_some(OwnedConstraintSet {
            node: self.node,
            source_order: None,
            inner: None,
        })
    }

    /// Compares the builder, decision node, and source ordering without traversing storage.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn has_same_identity(self, other: Self) -> bool {
        std::ptr::eq(self.builder, other.builder)
            && self.node == other.node
            && self.source_order == other.source_order
    }

    #[cfg(test)]
    pub(crate) fn ownership_probe_same_set(self, other: Self) -> bool {
        self.has_same_identity(other)
    }

    #[cfg(test)]
    pub(in crate::types) fn ownership_probe_single_equivalence(
        self,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Option<([usize; 7], [usize; 2])> {
        // Cleanup probes must observe missing or borrowed storage without panicking in Drop.
        let storage = self.builder.storage.try_borrow().ok()?;
        if self.node.is_terminal() || storage.compacted.is_some() {
            return None;
        }
        let node = storage.nodes.get(self.node)?;
        let Constraint::ConcreteEquivalence(constraint) =
            storage.constraints.get(node.constraint)?
        else {
            return None;
        };
        let source = storage.source_orders.get(self.source_order?)?;
        let node_support_id = *storage.node_supports.get(self.node)?;
        let node_support = storage.supports.get(node_support_id)?;
        let constraint_support_id = *storage.constraint_supports.get(node.constraint)?;
        let constraint_support = storage.supports.get(constraint_support_id)?;
        if constraint.provenance != ConstraintProvenance::Evidence
            || constraint.typevar != typevar
            || constraint.bound != bound
            || node.if_true != ALWAYS_TRUE
            || node.if_uncertain != ALWAYS_FALSE
            || node.if_false != ALWAYS_FALSE
            || *source != SourceOrder::Constraint(node.constraint)
            || storage.typevars.get(TypeVarId::from_usize(0)) != Some(&typevar)
            || constraint_support.words() != [1]
            || !constraint_support.is_complete()
            || node_support.words() != [1]
            || !node_support.is_complete()
        {
            return None;
        }
        Some((
            [
                storage.constraints.len(),
                storage.typevars.len(),
                storage.nodes.len(),
                storage.supports.len(),
                storage.constraint_supports.len(),
                storage.node_supports.len(),
                storage.source_orders.len(),
            ],
            [constraint_support_id.index(), node_support_id.index()],
        ))
    }

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
        let mut storage = builder.storage.borrow_mut();
        if lower == upper {
            // Intern typevar first so that if the resulting bound happens to be a
            // TypeVarEquivalenceBound, we'll intern the left/right typevars in a builder-specific
            // stable order.
            storage.intern_typevar(db, typevar);
            let constraints = Constraint::new_equivalence_bound(
                db,
                env,
                ConstraintProvenance::Evidence,
                typevar,
                lower,
            );
            let (node, source_order) = Constraint::new_nodes(db, env, &mut storage, constraints);
            return Self::from_node(builder, node, source_order);
        }

        let constraints = iter::chain(
            Constraint::new_lower_bound(db, ConstraintProvenance::Evidence, typevar, lower),
            Constraint::new_upper_bound(db, env, ConstraintProvenance::Evidence, typevar, upper),
        );
        let (node, source_order) = Constraint::new_nodes(db, env, &mut storage, constraints);
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
        let mut storage = builder.storage.borrow_mut();
        let constraints =
            Constraint::new_lower_bound(db, ConstraintProvenance::Evidence, typevar, lower);
        let (node, source_order) = Constraint::new_nodes(db, env, &mut storage, constraints);
        Self::from_node(builder, node, source_order)
    }

    /// Returns a constraint set that constrains a typevar to be a subtype of `upper`.
    pub(crate) fn constrain_typevar_upper_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        let mut storage = builder.storage.borrow_mut();
        let constraints =
            Constraint::new_upper_bound(db, env, ConstraintProvenance::Evidence, typevar, upper);
        let (node, source_order) = Constraint::new_nodes(db, env, &mut storage, constraints);
        Self::from_node(builder, node, source_order)
    }

    /// Returns a constraint set that constrains a typevar to be equivalent to `bound`.
    pub(crate) fn constrain_typevar_equivalence_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        let mut storage = builder.storage.borrow_mut();
        // Intern typevar first so that if the resulting bound happens to be a
        // TypeVarEquivalenceBound, we'll intern the left/right typevars in a builder-specific
        // stable order.
        storage.intern_typevar(db, typevar);
        let constraints = Constraint::new_equivalence_bound(
            db,
            env,
            ConstraintProvenance::Evidence,
            typevar,
            bound,
        );
        let (node, source_order) = Constraint::new_nodes(db, env, &mut storage, constraints);
        Self::from_node(builder, node, source_order)
    }

    /// Returns whether this constraint set uses `builder`'s arenas.
    pub(in crate::types) fn is_from_builder(self, builder: &ConstraintSetBuilder<'db>) -> bool {
        std::ptr::eq(self.builder, builder)
    }

    /// Verifies that this constraint set was created by `builder`
    #[track_caller]
    pub(super) fn verify_builder(self, builder: &'c ConstraintSetBuilder<'db>) {
        debug_assert!(self.is_from_builder(builder));
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn satisfaction_start(self, always: bool) -> Option<bool> {
        let kind = if always {
            satisfaction::SatisfactionKind::Always
        } else {
            satisfaction::SatisfactionKind::Never
        };
        satisfaction::node_satisfaction_start(self.node, kind).break_value()
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
            Ok(Solutions::Unsatisfiable(_))
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
    pub(super) fn mentions_typevar(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
    ) -> bool {
        let identity = typevar.identity(db);
        let storage = self.builder.storage.borrow();
        storage.node_support(self.node).is_some_and(|support| {
            support
                .iter()
                .any(|id| storage.typevar_data(id).identity(db) == identity)
        })
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
        (self.node, self.source_order) = combination::Combination::new(
            ConstraintFoldKind::Any,
            (self.node, self.source_order),
            (other.node, other.source_order),
        )
        .finish(&mut storage);
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
        (self.node, self.source_order) = combination::Combination::new(
            ConstraintFoldKind::All,
            (self.node, self.source_order),
            (other.node, other.source_order),
        )
        .finish(&mut storage);
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
            let mapped =
                constraint.apply_type_mapping_impl(db, self.builder, type_mapping, tcx, visitor);
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
    overlay_identity_state: storage::OverlayIdentityState,

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
}

impl<'db> ConstraintSetStorage<'db> {
    fn ensure_overlay_identity_caches(&mut self) {
        while control::unrestricted(self.advance_identity_caches(&mut control::Unrestricted))
            .is_continue()
        {}
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

    /// Loads an unconditional result without reading or changing either constraint arena.
    pub(in crate::types) fn load_terminal<'c>(
        &'c self,
        terminal: TerminalConstraint,
    ) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_node(self, terminal.node(), None)
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
        if let Some(owned) = constraint.to_owned_terminal() {
            return owned;
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
        if let Some(terminal) = other.terminal() {
            return self.load_terminal(terminal);
        }
        let mut storage = self.storage.borrow_mut();
        let (node, source_order) = storage.load(db, env, other);
        ConstraintSet::from_node(self, node, source_order)
    }
}

struct OrdinaryConstraintSupport<'a, 'db> {
    storage: &'a mut ConstraintSetStorage<'db>,
    support: &'a mut Support,
}
impl<'db> SyncTypeSupportEffects<'db>
    for OrdinaryTypeWalk<'_, '_, 'db, UnrestrictedWalk, OrdinaryConstraintSupport<'_, 'db>>
{
    fn record_occurrence(&mut self, typevar: BoundTypeVarInstance<'db>) -> Result<(), Infallible> {
        let id = self.query.storage.intern_typevar(self.db, typevar);
        self.query.support.insert(id);
        Ok(())
    }
    fn skipped_lazy(&mut self) -> Result<(), Infallible> {
        self.query.support.mark_incomplete();
        Ok(())
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
        self.publish_typevar_miss(identity, typevar)
    }

    fn publish_typevar_miss(
        &mut self,
        identity: BoundTypeVarIdentity<'db>,
        typevar: BoundTypeVarInstance<'db>,
    ) -> TypeVarId {
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
        let result = support_type_sync(
            ty,
            TypeWalkFacts,
            &mut OrdinaryTypeWalk {
                db,
                env,
                control: &mut UnrestrictedWalk,
                query: OrdinaryConstraintSupport {
                    storage: self,
                    support,
                },
            },
        );
        match result {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    /// Interns all of the typevars mentioned in a constraint in a stable order.
    fn intern_constraint_typevars(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraint: Constraint<'db>,
    ) -> Support {
        let mut support = Support::default();
        match constraint {
            Constraint::ConcreteLower(constraint) => {
                support.insert(self.intern_typevar(db, constraint.typevar));
                self.intern_mentioned_typevars_in_type(db, env, constraint.bound, &mut support);
            }
            Constraint::ConcreteUpper(constraint) => {
                support.insert(self.intern_typevar(db, constraint.typevar));
                self.intern_mentioned_typevars_in_type(db, env, constraint.bound, &mut support);
            }
            Constraint::ConcreteEquivalence(constraint) => {
                support.insert(self.intern_typevar(db, constraint.typevar));
                self.intern_mentioned_typevars_in_type(db, env, constraint.bound, &mut support);
            }
            Constraint::TypeVarRange(constraint) => {
                support.insert(self.intern_typevar(db, constraint.left));
                support.insert(self.intern_typevar(db, constraint.right));
            }
            Constraint::TypeVarEquivalence(constraint) => {
                support.insert(self.intern_typevar(db, constraint.left));
                support.insert(self.intern_typevar(db, constraint.right));
            }
        }
        support
    }

    fn intern_constraint(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        data: Constraint<'db>,
    ) -> ConstraintId {
        let support = self.intern_constraint_typevars(db, env, data);

        self.ensure_overlay_identity_caches();
        if let Some(id) = self.constraint_cache.get(&data) {
            return *id;
        }
        self.publish_constraint_miss(data, support)
    }

    fn publish_constraint_miss(&mut self, data: Constraint<'db>, support: Support) -> ConstraintId {
        let support_id = self.intern_support(support);
        let id = self.constraints.push(data);
        self.constraint_supports.push(support_id);
        let id = self.adjusted_constraint_id(id);
        self.constraint_cache.insert(data, id);
        id
    }

    fn intern_interior_node(&mut self, data: InteriorNodeData) -> NodeId {
        storage::PendingNode::new(data, None, false).finish(self)
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
        let result = type_analysis::cached_constraint_bound_depth_sync(
            constraint,
            &mut type_analysis::OrdinaryConstraintDepthCache {
                db,
                env,
                storage: self,
            },
        );
        match result {
            Ok(depth) => depth,
            Err(never) => match never {},
        }
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
        source_order::PendingSourceOrder::new(data).finish(self)
    }

    /// Repeating a source-order tree cannot change the first occurrence of any constraint, so
    /// combining identical trees must reuse their existing sidecar.
    fn ordered_source_order(
        &mut self,
        left: Option<SourceOrderId>,
        right: Option<SourceOrderId>,
    ) -> Option<SourceOrderId> {
        source_order::OrderedSource::new(left, right).finish(self)
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
        let mut scan = SourceOrderScan::new(source_order);
        loop {
            if unrestricted(scan.advance_with(self, &mut Unrestricted)).is_break() {
                return scan.result;
            }
        }
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

        if let Some(terminal) = other.terminal() {
            return (terminal.node(), None);
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
            .map(|old_constraint| old_constraint.new_node(db, env, self))
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
        wobble_index(storage.typevar_id(db, self).index() as u64)
            < wobble_index(storage.typevar_id(db, typevar).index() as u64)
    }
}

/// Optionally applies a transformation to the orderings that we use to compare builder-local
/// typevar and constraint IDs, and to canonicalize
/// [`TypeVarEquivalenceBound`][variables::TypeVarEquivalenceBound]s. This lets us exercise
/// different BDD variable orderings, among other things.
///
/// Under normal operation, the orderings won't be modified, and we will construct BDDs based on
/// the (builder-local) source order that we encounter typevars and constraints.
///
/// Our results _shouldn't_ depend on the BDD variable ordering that we choose. You can use the
/// `TY_CONSTRAINT_SET_ORDER` environment variable to artificially choose different permutations of
/// the "natural" variable orderings, to ensure that results are consistent.
fn wobble_index(index: u64) -> u64 {
    #[derive(Clone, Copy)]
    enum Order {
        Normal,
        Reverse,
        Xor(u64),
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
            .and_then(|value| value.parse::<u64>().ok())
            .map_or(Order::Normal, Order::Xor)
    });

    match *ORDER {
        Order::Normal => index,
        Order::Reverse => !index,
        Order::Xor(mask) => index ^ mask,
    }
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
struct UpperBound<'db> {
    evidence: FxOrderSet<Type<'db>>,
    validity: FxOrderSet<Type<'db>>,
}

impl<'db> UpperBound<'db> {
    #[cfg(test)]
    fn unconstrained() -> Self {
        Self::default()
    }

    /// Creates an upper bound from one explicit evidence clause.
    fn from_clause(clause: Type<'db>) -> Self {
        let mut upper = Self::default();
        upper.evidence.insert(clause);
        upper
    }

    fn iter_evidence(&self) -> impl Iterator<Item = Type<'db>> + Clone + '_ {
        self.evidence.iter().copied()
    }

    fn iter_validity(&self) -> impl Iterator<Item = Type<'db>> + Clone + '_ {
        self.validity.iter().copied()
    }

    fn iter_clauses(&self) -> impl Iterator<Item = Type<'db>> + Clone + '_ {
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
    fn as_single_bound(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        let mut clauses = self.iter_clauses().peekable();
        if clauses.peek().is_none() {
            Some(Type::object())
        } else {
            Self::single_bound_from_iterator(db, env, clauses)
        }
    }

    fn single_bound_from_iterator(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut clauses: impl Iterator<Item = Type<'db>> + Clone,
    ) -> Option<Type<'db>> {
        let candidate = clauses.clone().reduce(|candidate, clause| {
            if candidate.is_redundant_with(db, env, clause) {
                candidate
            } else {
                clause
            }
        })?;

        clauses
            .all(|clause| candidate.is_redundant_with(db, env, clause))
            .then_some(candidate)
    }

    fn add_clause(&mut self, provenance: ConstraintProvenance, ty: Type<'db>) {
        if provenance == ConstraintProvenance::Validity && ty.is_object() {
            return;
        }

        if provenance == ConstraintProvenance::Evidence && self.evidence.contains(&Type::Never) {
            return;
        }

        match (provenance, ty) {
            (ConstraintProvenance::Evidence, Type::Never) => {
                self.evidence.clear();
                self.evidence.insert(Type::Never);
            }
            (ConstraintProvenance::Validity, Type::Never) => {
                self.validity.clear();
                self.validity.insert(Type::Never);
            }
            (ConstraintProvenance::Evidence, _) => {
                self.evidence.insert(ty);
            }
            (ConstraintProvenance::Validity, _) => {
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

    fn is_satisfied_by(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> bool {
        self.iter_clauses()
            .all(|clause| ty.is_constraint_set_assignable_to(db, env, clause))
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
            let when_clause = lower.when_constraint_set_assignable_to_owned(db, env, clause);
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
    let result = crate::types::visitor::type_depth_sync(
        ty,
        crate::types::visitor::TypeWalkFacts,
        &mut crate::types::visitor::OrdinaryTypeWalk {
            db,
            env,
            control: &mut crate::types::visitor::Unrestricted,
            query: (),
        },
    );
    match result {
        Ok(depth) => depth,
        Err(never) => match never {},
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
        std::cmp::Reverse(wobble_index(self.index() as u64))
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
    #[cfg(test)]
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
        if_true: NodeId,
        if_uncertain: NodeId,
        if_false: NodeId,
    ) -> NodeId {
        match Self::reduce_uncertain(
            storage,
            InteriorNodeData {
                constraint,
                if_true,
                if_uncertain,
                if_false,
            },
        ) {
            storage::ReducedNode::Existing(node) => node,
            storage::ReducedNode::Interior(data) => storage.intern_interior_node(data),
        }
    }

    fn reduce_uncertain(
        storage: &ConstraintSetStorage<'_>,
        data: InteriorNodeData,
    ) -> storage::ReducedNode {
        let InteriorNodeData {
            constraint,
            mut if_true,
            if_uncertain,
            mut if_false,
        } = data;
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
            return storage::ReducedNode::Existing(ALWAYS_TRUE);
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
                return storage::ReducedNode::Existing(if_uncertain);
            }
            if if_uncertain == ALWAYS_FALSE {
                return storage::ReducedNode::Existing(if_true);
            }

            // TODO: A future reduction can handle this remaining `if_true == if_false` case by
            // returning `if_true ∪ if_uncertain`. That needs an `OR` computation, but only after
            // the local equality check has already engaged.
        }

        storage::ReducedNode::Interior(InteriorNodeData {
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
        let mut scan = SingleConjunctionScan::new(self);
        loop {
            if let ControlFlow::Break(result) =
                unrestricted(scan.advance_with(storage, &mut Unrestricted))
            {
                return result;
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
        match node_satisfaction_sync(
            self,
            source_order,
            SatisfactionKind::Always,
            &mut OrdinarySatisfaction { db, env, storage },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
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
        match node_satisfaction_sync(
            self,
            source_order,
            SatisfactionKind::Never,
            &mut OrdinarySatisfaction { db, env, storage },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Returns the negation of this BDD.
    fn negate(self, storage: &mut ConstraintSetStorage<'_>) -> Self {
        apply::Operation::Negate(self).apply(storage)
    }

    /// Returns the `or` or union of two BDDs.
    fn or(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> Self {
        apply::Operation::Or(self, other).apply(storage)
    }

    fn tree_fold(
        builder: &ConstraintSetBuilder<'_>,
        nodes: impl Iterator<Item = (Self, Option<SourceOrderId>)>,
        kind: ConstraintFoldKind,
    ) -> (Self, Option<SourceOrderId>) {
        let mut fold = ConstraintFold::new(builder, kind);
        for (node, source_order) in nodes {
            let next = ConstraintSet::from_node(builder, node, source_order);
            if let ControlFlow::Break(result) = fold.push(next) {
                return (result.node, result.source_order);
            }
        }
        let result = fold.finish();
        (result.node, result.source_order)
    }

    fn distributed_or(
        builder: &ConstraintSetBuilder<'_>,
        nodes: impl Iterator<Item = (NodeId, Option<SourceOrderId>)>,
    ) -> (Self, Option<SourceOrderId>) {
        Self::tree_fold(builder, nodes, ConstraintFoldKind::Any)
    }

    fn distributed_and(
        builder: &ConstraintSetBuilder<'_>,
        nodes: impl Iterator<Item = (NodeId, Option<SourceOrderId>)>,
    ) -> (Self, Option<SourceOrderId>) {
        Self::tree_fold(builder, nodes, ConstraintFoldKind::All)
    }

    /// Returns the `and` or intersection of two BDDs.
    fn and(self, storage: &mut ConstraintSetStorage<'_>, other: Self) -> Self {
        apply::Operation::And(self, other).apply(storage)
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
            (Type::TypeVar(bound_typevar), _) => {
                let constraints = Constraint::new_upper_bound(
                    db,
                    env,
                    ConstraintProvenance::Evidence,
                    bound_typevar,
                    rhs.bottom_materialization(db, env),
                );
                Constraint::new_nodes(db, env, storage, constraints)
            }
            (_, Type::TypeVar(bound_typevar)) => {
                let constraints = Constraint::new_lower_bound(
                    db,
                    ConstraintProvenance::Evidence,
                    bound_typevar,
                    lhs.top_materialization(db, env),
                );
                Constraint::new_nodes(db, env, storage, constraints)
            }
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
        let mut scan = UniqueConstraintScan::new(self);
        loop {
            match unrestricted(scan.advance_with(storage, &mut Unrestricted)) {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(Some(constraint)) => f(constraint),
                ControlFlow::Break(None) => return,
            }
        }
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
struct PathBoundBuilder<'db> {
    evidence_lower: FxIndexSet<Type<'db>>,
    validity_lower: FxIndexSet<Type<'db>>,
    upper: UpperBound<'db>,
}

impl<'db> PathBoundBuilder<'db> {
    fn add_lower(&mut self, provenance: ConstraintProvenance, ty: Type<'db>) {
        // Lower bounds are unioned. Our type representation is in DNF, so unioning a new
        // element is typically cheap (in that it does not involve a combinatorial
        // explosion from distributing the clause through an existing disjunction). So we
        // don't need to be as clever here as in `add_upper`.
        match provenance {
            ConstraintProvenance::Evidence => {
                self.evidence_lower.insert(ty);
            }
            ConstraintProvenance::Validity => {
                if !ty.is_never() {
                    self.validity_lower.insert(ty);
                }
            }
        }
    }

    fn add_upper(&mut self, provenance: ConstraintProvenance, ty: Type<'db>) {
        self.upper.add_clause(provenance, ty);
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
        } = self;

        // Classify the original evidence bounds before aggregation, as gradual and static argument
        // evidence may collapse into a single gradual union.
        //
        // Note that we only compute this flag for constrained typevars.
        let has_only_gradual_evidence = bound_typevar.typevar(db).is_constrained(db).then(|| {
            let mut evidence = evidence_lower
                .iter()
                .copied()
                .chain(upper.iter_evidence())
                .filter(|ty| !ty.has_unspecialized_type_var(db, env))
                .peekable();

            evidence.peek().is_some()
                && evidence
                    .all(|ty| ty.bottom_materialization(db, env) != ty.top_materialization(db, env))
        });

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
            has_only_gradual_evidence,
        }
    }
}

/// The result of selecting a type for one typevar on one constraint path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathBoundSolution<'db> {
    Solved(Type<'db>),
    /// The path provides no type to infer for this variable.
    Unsolved,
    /// The path's lower and upper bounds cannot be satisfied together
    Unsatisfiable,
    /// The path does not satisfy the typevar's declared upper bound
    ViolatesDeclaredUpperBound,
    /// The path does not satisfy the typevar's declared constraints
    ViolatesDeclaredConstraints,
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
            Self::Unsolved
            | Self::Unsatisfiable
            | Self::ViolatesDeclaredUpperBound
            | Self::ViolatesDeclaredConstraints => self,
        }
    }

    /// Returns the selected type, including a fallback when the budget was exceeded.
    /// Match the outcome directly when completeness or the reason no type was selected matters.
    pub(crate) fn as_type(self) -> Option<Type<'db>> {
        match self {
            Self::Solved(ty) => Some(ty),
            Self::Unsolved
            | Self::Unsatisfiable
            | Self::ViolatesDeclaredUpperBound
            | Self::ViolatesDeclaredConstraints => None,
            Self::BudgetExceeded { fallback } => fallback,
        }
    }
}

/// The explicit lower and upper bounds inferred for one typevar on one BDD path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct PathBound<'db> {
    pub(crate) bound_typevar: BoundTypeVarInstance<'db>,
    evidence_lower: Option<Type<'db>>,
    validity_lower: Type<'db>,
    upper: UpperBound<'db>,
    /// Whether the path contains gradual evidence and no static evidence.
    ///
    /// Note that this is only computed for constrained typevars.
    has_only_gradual_evidence: Option<bool>,
}

impl<'db> PathBound<'db> {
    pub(crate) fn exact(bound_typevar: BoundTypeVarInstance<'db>, ty: Type<'db>) -> Self {
        Self {
            bound_typevar,
            evidence_lower: Some(ty),
            validity_lower: Type::Never,
            upper: UpperBound::from_clause(ty),
            has_only_gradual_evidence: None,
        }
    }

    /// Returns lower-bound inference evidence without supplying a default for a missing bound.
    pub(crate) fn evidence_lower(&self) -> Option<Type<'db>> {
        self.evidence_lower
    }

    /// Returns one effective upper bound without expanding factored intersections.
    pub(crate) fn as_single_upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        self.upper.as_single_bound(db, env)
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

        let mut upper_bounds = self.upper.iter_evidence().filter_map(materialize_upper);
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
    /// Calculates the [`CandidateSolutions`] that represent the valid solutions for when `self` is
    /// constraint-set assignable to `target`.
    pub(crate) fn assignable_solutions_with_inferable(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> &'db CandidateSolutions<'db> {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _, _, _, _| CandidateSolutions::Unsatisfiable,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn assignable_solutions_impl<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            source: Type<'db>,
            target: Type<'db>,
            inferable: TypeVarSet<'db>,
        ) -> CandidateSolutions<'db> {
            let env = &ProgramEnvironment::from_program(program);
            let when = source.when_constraint_set_assignable_to_owned(db, env, target);
            when.query(|builder, when| {
                let mut storage = builder.storage.borrow_mut();
                CandidateSolutions::compute(
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

#[salsa::tracked(configuration = (pub(in crate::types) IsPossiblyConstraintSetAssignableConfiguration), attempt = ReturnOnly,
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

#[cfg(any(test, feature = "experimental-analysis"))]
pub(super) fn possible_assignability_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<IsPossiblyConstraintSetAssignableConfiguration> {
    is_possibly_constraint_set_assignable::fn_ingredient_(db, db.zalsa())
}

/// Candidate solutions for a constraint set
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum CandidateSolutions<'db> {
    /// There are no solutions to the constraint set
    Unsatisfiable,
    /// The constraint set is trivially satisfied, and places no restrictions on any of the
    /// inferable typevars
    Unconstrained,
    /// The constraint set has a fix set of solutions. Each solution provides a lower and upper
    /// bound for each inferable typevar.
    Constrained(Box<[CandidateSolution<'db>]>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct CandidateSolution<'db> {
    typevars: Box<[PathBound<'db>]>,
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

impl<'db> CandidateSolutions<'db> {
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
                return ControlFlow::Continue(CandidateSolutions::Unconstrained);
            }
            Node::AlwaysFalse => {
                limits.visit_node()?;
                return ControlFlow::Continue(CandidateSolutions::Unsatisfiable);
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
                        return ControlFlow::Continue(Some(CandidateSolutions::Unconstrained));
                    }
                    limits.satisfied_path()?;
                    break;
                }
                Node::AlwaysFalse => {
                    return ControlFlow::Continue(
                        constraints
                            .is_empty()
                            .then_some(CandidateSolutions::Unsatisfiable),
                    );
                }
                Node::Interior(_) => {
                    let interior = storage.interior_node_data(current);
                    if interior.if_uncertain != ALWAYS_FALSE || interior.if_false != ALWAYS_FALSE {
                        return ControlFlow::Continue(None);
                    }

                    let constraint = storage.constraint_data(interior.constraint);
                    match constraint {
                        Constraint::ConcreteLower(lower) => {
                            if !lower.typevar.is_inferable(db, inferable) {
                                return ControlFlow::Continue(None);
                            }
                            if lower.bound.has_typevar(db, env)
                                || lower.bound.has_provisional_marker(db, env)
                            {
                                return ControlFlow::Continue(None);
                            }
                            constraints.push((
                                constraint,
                                source_orders
                                    .get_index_of(&interior.constraint)
                                    .expect("every TDD constraint should have a source order"),
                            ));
                        }

                        Constraint::ConcreteUpper(upper) => {
                            if !upper.typevar.is_inferable(db, inferable) {
                                return ControlFlow::Continue(None);
                            }
                            if upper.bound.has_typevar(db, env)
                                || upper.bound.has_provisional_marker(db, env)
                            {
                                return ControlFlow::Continue(None);
                            }
                            constraints.push((
                                constraint,
                                source_orders
                                    .get_index_of(&interior.constraint)
                                    .expect("every TDD constraint should have a source order"),
                            ));
                        }

                        Constraint::ConcreteEquivalence(equivalence) => {
                            if !equivalence.typevar.is_inferable(db, inferable) {
                                return ControlFlow::Continue(None);
                            }
                            if equivalence.bound.has_typevar(db, env)
                                || equivalence.bound.has_provisional_marker(db, env)
                            {
                                return ControlFlow::Continue(None);
                            }
                            constraints.push((
                                constraint,
                                source_orders
                                    .get_index_of(&interior.constraint)
                                    .expect("every TDD constraint should have a source order"),
                            ));
                        }

                        Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => {
                            return ControlFlow::Continue(None);
                        }
                    }

                    current = interior.if_true;
                }
            }
        }

        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, PathBoundBuilder<'db>> =
            FxIndexMap::default();
        constraints.sort_by_key(|(_, source_order)| *source_order);
        for (constraint, _) in constraints {
            match constraint {
                Constraint::ConcreteLower(lower) => {
                    let bounds = mappings.entry(lower.typevar).or_default();
                    bounds.add_lower(lower.provenance, lower.bound);
                }
                Constraint::ConcreteUpper(upper) => {
                    let bounds = mappings.entry(upper.typevar).or_default();
                    bounds.add_upper(upper.provenance, upper.bound);
                }
                Constraint::ConcreteEquivalence(equivalence) => {
                    let bounds = mappings.entry(equivalence.typevar).or_default();
                    bounds.add_lower(equivalence.provenance, equivalence.bound);
                    bounds.add_upper(equivalence.provenance, equivalence.bound);
                }
                Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => {
                    panic!("typevar constraint should have been filtered out");
                }
            }
        }

        let typevars = mappings
            .drain(..)
            .map(|(bound_typevar, bounds)| bounds.finish(db, env, bound_typevar))
            .collect();
        let candidate = CandidateSolution { typevars };
        ControlFlow::Continue(Some(CandidateSolutions::Constrained(Box::new([candidate]))))
    }

    pub(crate) fn solve(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Solutions<'db> {
        self.solve_with(|_variance, path_bound| {
            CandidateSolutions::default_solve(db, env, builder, inferable, path_bound)
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
            CandidateSolutions::Unsatisfiable => {
                let solutions = SolutionPaths::Complete(Vec::default());
                return Ok(Solutions::Unsatisfiable(solutions));
            }
            CandidateSolutions::Unconstrained => return Ok(Solutions::Unconstrained),
            CandidateSolutions::Constrained(paths) => paths,
        };

        let mut valid_solutions = Vec::with_capacity(paths.len());
        let mut invalid_solutions = Vec::new();
        let mut valid_exceeded_budget = false;
        let mut invalid_exceeded_budget = false;
        for path in paths {
            let Some((solution, path_exceeded_budget)) = Self::solve_path_with(path, &mut choose)
            else {
                continue;
            };
            if solution.is_valid() {
                check_solution(&solution)?;
                valid_exceeded_budget |= path_exceeded_budget;
                valid_solutions.push(solution);
            } else {
                invalid_exceeded_budget |= path_exceeded_budget;
                invalid_solutions.push(solution);
            }
        }

        if !valid_solutions.is_empty() {
            let solutions = SolutionPaths::new(valid_solutions, valid_exceeded_budget);
            return Ok(Solutions::Constrained(solutions));
        }

        for solution in &invalid_solutions {
            check_solution(solution)?;
        }
        let solutions = SolutionPaths::new(invalid_solutions, invalid_exceeded_budget);
        Ok(Solutions::Unsatisfiable(solutions))
    }

    /// Solves one complete path, retaining whether any of its bindings used a fallback.
    /// A later unsatisfiable bound rejects the path even if an earlier bound exhausted its budget.
    fn solve_path_with(
        candidate: &CandidateSolution<'db>,
        choose: &mut impl FnMut(TypeVarVariance, &PathBound<'db>) -> PathBoundSolution<'db>,
    ) -> Option<(Solution<'db>, bool)> {
        let mut solved_typevars = Vec::with_capacity(candidate.typevars.len());
        let mut violations = Vec::new();
        let mut exceeded_budget = false;
        for path_bound in &candidate.typevars {
            let ty = match choose(path_bound.variance(), path_bound) {
                PathBoundSolution::Solved(ty) => Some(ty),
                PathBoundSolution::Unsolved => None,
                PathBoundSolution::Unsatisfiable => return None,
                PathBoundSolution::ViolatesDeclaredUpperBound => {
                    violations.push(SolutionViolation {
                        bound_typevar: path_bound.bound_typevar,
                        argument: path_bound.evidence_lower(),
                        variance: path_bound.variance(),
                        kind: SolutionViolationKind::UpperBound,
                    });
                    None
                }
                PathBoundSolution::ViolatesDeclaredConstraints => {
                    violations.push(SolutionViolation {
                        bound_typevar: path_bound.bound_typevar,
                        argument: path_bound.evidence_lower(),
                        variance: path_bound.variance(),
                        kind: SolutionViolationKind::Constraints,
                    });
                    None
                }
                PathBoundSolution::BudgetExceeded { fallback } => {
                    exceeded_budget = true;
                    fallback
                }
            };
            if let Some(ty) = ty {
                solved_typevars.push(TypeVarSolution {
                    bound_typevar: path_bound.bound_typevar,
                    solution: ty,
                });
            }
        }
        let validity = if violations.is_empty() {
            SolutionValidity::Valid
        } else {
            SolutionValidity::Invalid(violations.into_boxed_slice())
        };
        let solution = Solution {
            solved_typevars,
            validity,
        };
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
        inferable: TypeVarSet<'db>,
        path_bound: &PathBound<'db>,
    ) -> PathBoundSolution<'db> {
        let preliminary = Self::preliminary_solve(db, env, builder, inferable, path_bound);
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
        inferable: TypeVarSet<'db>,
        path_bound: &PathBound<'db>,
    ) -> PathBoundSolution<'db> {
        // Choose a solution type that satisfies the constraints on this path, as well as any upper
        // bound or constraints of the typevar itself.
        // TODO: Handle the upper bound/constraints by conjoining them with the constraint set
        // before solving.

        let bound_typevar = path_bound.bound_typevar;
        let lower = path_bound.effective_lower(db, env);

        match bound_typevar.require_bound_or_constraints(db, env) {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                let declared_upper = bound.top_materialization(db, env);

                // Prefer the lower bound (often the concrete actual type seen) over the
                // upper bound (which may include TypeVar bounds/constraints). The upper bound
                // should only be used as a fallback when no concrete type was inferred.
                if path_bound.evidence_lower.is_some() {
                    if !is_possibly_constraint_set_assignable(
                        db,
                        TypePair::new(db, env.program(db), lower, declared_upper),
                    ) {
                        // Prefer a declared-bound violation when the inferred bounds are also
                        // contradictory, so callers can report the more specific cause.
                        return PathBoundSolution::ViolatesDeclaredUpperBound;
                    }

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

                    return PathBoundSolution::Solved(lower);
                }

                if path_bound.has_upper_evidence() {
                    return IntersectionType::bounded_from_elements(
                        db,
                        env,
                        iter::chain(path_bound.upper.iter_clauses(), [declared_upper]),
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
                let dependent_solution = match (lower, path_bound.as_single_upper_bound(db, env)) {
                    (ty @ Type::TypeVar(_), _) | (_, Some(ty @ Type::TypeVar(_))) => Some(ty),
                    _ => None,
                };
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
                    // Selecting a concrete constraint must not specialize a caller's fixed
                    // typevar: `S & str <= int` may hold for some `S`, but not for every `S`.
                    // A bare dependent solution instead retains that variable's identity;
                    // it does not select one concrete constraint for every caller specialization.
                    if dependent_solution.is_none()
                        && lower
                            .when_assignable_to(db, env, constraint_upper, builder, inferable)
                            .and(db, builder, || {
                                path_bound
                                    .upper
                                    .iter_clauses()
                                    .when_all(db, builder, |upper| {
                                        constraint_lower
                                            .when_assignable_to(db, env, upper, builder, inferable)
                                    })
                            })
                            .is_never_satisfied(db, env)
                    {
                        continue;
                    }
                    // Keep the deferred conjunction too: scope-aware assignability does not
                    // yet record every relationship between inferable type variables.
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
                    return PathBoundSolution::ViolatesDeclaredConstraints;
                };

                if let Some(ty) = dependent_solution {
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
                if multiple_compatible_constraints
                    && path_bound.has_only_gradual_evidence == Some(true)
                {
                    if path_bound.evidence_lower.is_some() {
                        PathBoundSolution::Solved(path_bound.effective_lower(db, env))
                    } else if path_bound.has_upper_evidence() {
                        IntersectionType::bounded_from_elements(
                            db,
                            env,
                            path_bound.upper.iter_clauses(),
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
                !constraint.directly_constrains_inferable_typevar(db, inferable)
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
        match path_assignments_sync(
            self,
            source_order,
            &mut OrdinarySatisfaction { db, env, storage },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }
}

/// The result of solving a constraint set for per-typevar specializations.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Solutions<'db> {
    Unsatisfiable(SolutionPaths<'db>),
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
    fn new(solutions: Vec<Solution<'db>>, exceeded_budget: bool) -> Self {
        if exceeded_budget {
            SolutionPaths::BudgetExceeded(solutions)
        } else {
            SolutionPaths::Complete(solutions)
        }
    }

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum SolutionViolationKind {
    UpperBound,
    Constraints,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct SolutionViolation<'db> {
    pub(crate) bound_typevar: BoundTypeVarInstance<'db>,
    pub(crate) argument: Option<Type<'db>>,
    pub(crate) variance: TypeVarVariance,
    pub(crate) kind: SolutionViolationKind,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum SolutionValidity<'db> {
    /// The solution is valid
    Valid,
    /// The solution satisfies all evidence constraints, but doesn't satisfy the validity
    /// constraints. The violations indicate which declared upper bounds or constraints were not
    /// satisfied.
    Invalid(Box<[SolutionViolation<'db>]>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct Solution<'db> {
    pub(crate) solved_typevars: Vec<TypeVarSolution<'db>>,
    pub(crate) validity: SolutionValidity<'db>,
}

impl<'db> Solution<'db> {
    fn is_valid(&self) -> bool {
        matches!(self.validity, SolutionValidity::Valid)
    }

    pub(crate) fn violations(&self) -> &[SolutionViolation<'db>] {
        match &self.validity {
            SolutionValidity::Valid => &[],
            SolutionValidity::Invalid(violations) => violations,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
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
        let holds = match self {
            ConstraintAssignment::Positive(_) => Some(true),
            ConstraintAssignment::Negative(_) => Some(false),
            ConstraintAssignment::Unconstrained(_) => None,
        };

        std::fmt::from_fn(move |f| {
            let constraint_data = storage.constraint_data(self.constraint());
            constraint_data.display(db, env, holds).fmt(f)
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

    use super::variables::{
        ConcreteUpperBound, ConstraintProvenance, TypeVarRangeBound, UnsatisfiableBound,
    };
    use super::*;

    use indoc::indoc;
    use pretty_assertions::assert_eq;

    use crate::db::tests::{TestDb, setup_db};
    use crate::place::global_symbol;
    use crate::types::generics::ApplySpecialization;
    use crate::types::tuple::TupleType;
    use crate::types::typevar::{
        TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarDefaultEvaluation,
        TypeVarInstance,
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
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, bound_typevar, ty)
    }

    fn create_constraint_set_with_bounds<'db, 'c>(
        db: &'db TestDb,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Option<Type<'db>>,
        upper: Option<Type<'db>>,
    ) -> ConstraintSet<'db, 'c> {
        match (lower, upper) {
            (None, None) => ConstraintSet::from_bool(builder, true),
            (Some(lower), None) => {
                ConstraintSet::constrain_typevar_lower_bound(db, env, builder, typevar, lower)
            }
            (None, Some(upper)) => {
                ConstraintSet::constrain_typevar_upper_bound(db, env, builder, typevar, upper)
            }
            (Some(lower), Some(upper)) => {
                ConstraintSet::constrain_typevar(db, env, builder, typevar, lower, upper)
            }
        }
    }

    #[test]
    fn owned_constraint_type_steps_include_duplicate_nodes() -> anyhow::Result<()> {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let owned = ConstraintSetBuilder::new().into_owned(|builder| {
            let t_int = create_constraint(db, builder, t, KnownClass::Int);
            let u_str = create_constraint(db, builder, u, KnownClass::Str);
            t_int.iff(db, builder, u_str).negate(db, builder)
        });

        // XOR retains positive and negative nodes for one constraint below a distinct root.
        // Inspect their IDs to keep this test independent of variable ordering.
        let Some(inner) = owned.inner.as_deref() else {
            anyhow::bail!("the XOR of independent constraints must retain a diagram");
        };
        assert_eq!(inner.nodes.len(), 3);
        assert_eq!(inner.nodes[0].constraint, inner.nodes[1].constraint);
        assert_ne!(inner.nodes[0].constraint, inner.nodes[2].constraint);
        let leaf_pair = inner.constraints
            [inner.retained_constraint_index(inner.nodes[0].constraint)]
        .type_pair();
        let root_pair = inner.constraints
            [inner.retained_constraint_index(inner.nodes[2].constraint)]
        .type_pair();
        let mut steps = owned.type_steps();
        assert_eq!(steps.next(), Some(Some(leaf_pair)));
        assert_eq!(steps.next(), Some(None));
        assert_eq!(steps.next(), Some(Some(root_pair)));
        assert_eq!(steps.next(), None);

        // Stopping after two steps reaches the duplicate without scanning ahead to the root.
        assert_eq!(
            owned.type_steps().take(2).collect::<Vec<_>>(),
            [Some(leaf_pair), None],
        );
        assert_eq!(
            owned.types().collect::<Vec<_>>(),
            leaf_pair.into_iter().chain(root_pair).collect::<Vec<_>>(),
        );
        Ok(())
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
    ) -> Result<CandidateSolutions<'db>, ProjectionError> {
        CandidateSolutions::compute_bounded(
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

    fn solution<'db>(
        solved_typevars: impl IntoIterator<Item = TypeVarSolution<'db>>,
    ) -> Solution<'db> {
        let solved_typevars = solved_typevars.into_iter().collect();
        Solution {
            solved_typevars,
            validity: SolutionValidity::Valid,
        }
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
        let set =
            ConstraintSet::constrain_typevar_equivalence_bound(db, &env, &builder, t, list_of_u);

        let int = KnownClass::Int.to_instance(db, &env);
        let mapped = set.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Single(u, int)),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        );
        let list_of_int = KnownClass::List.to_specialized_instance(db, &env, &[int]);
        let expected =
            ConstraintSet::constrain_typevar_equivalence_bound(db, &env, &builder, t, list_of_int);

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
        let data = ConcreteUpperBound::new(ConstraintProvenance::Evidence, t, actual_bound);
        let support = storage.intern_constraint_typevars(db, &env, data.into());
        let mentioned = support
            .iter()
            .map(|typevar| storage.typevar_data(typevar))
            .collect::<Vec<_>>();

        assert_eq!(mentioned, vec![t, u]);
        assert!(support.is_complete());

        let builder = ConstraintSetBuilder::new();
        let constraint =
            ConstraintSet::constrain_typevar_upper_bound(db, &env, &builder, t, actual_bound);
        assert!(constraint.mentions_typevar(db, t));
        assert!(constraint.mentions_typevar(db, u));
        assert!(!constraint.mentions_typevar(db, metadata));
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
            let data = TypeVarRangeBound::new(db, ConstraintProvenance::Evidence, t, u);
            let support = storage.intern_constraint_typevars(db, &env, data.into());
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
        let int = known_instance(db, KnownClass::Int);

        let mut upper = UpperBound::from_clause(int);
        upper.add_clause(ConstraintProvenance::Evidence, Type::Never);
        assert_eq!(upper.evidence, FxOrderSet::from_iter([Type::Never]));

        upper.add_clause(ConstraintProvenance::Evidence, int);
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
                upper.add_clause(ConstraintProvenance::Evidence, clause);
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
                upper.add_clause(ConstraintProvenance::Evidence, clause);
            }
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
        upper.add_clause(ConstraintProvenance::Evidence, int);
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
            (
                ConstraintSet::always(&builder),
                CandidateSolutions::Unconstrained,
            ),
            (
                ConstraintSet::never(&builder),
                CandidateSolutions::Unsatisfiable,
            ),
        ] {
            assert_eq!(
                bounded_path_bounds(db, set, inferable, 0, 0),
                Err(ProjectionError::TraversalBudgetExceeded)
            );
            assert_eq!(bounded_path_bounds(db, set, inferable, 0, 1), Ok(expected));
        }

        let set = create_constraint(db, &builder, t, KnownClass::Int);
        let expected = CandidateSolutions::compute(
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
        let ControlFlow::Continue(fast_path) = CandidateSolutions::compute_simple_bound_conjunction(
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
        let ControlFlow::Continue(expected) = CandidateSolutions::compute_with_limits(
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
            has_only_gradual_evidence: None,
        };
        let inferable = TypeVarSet::from_typevars(db, [t]);

        assert_eq!(
            CandidateSolutions::default_solve(db, &env, &builder, inferable, &path_bound),
            PathBoundSolution::Unsolved
        );
        assert_eq!(PathBoundSolution::Unsolved.as_type(), None);
        assert_eq!(
            CandidateSolutions::Constrained(Box::new([CandidateSolution {
                typevars: Box::new([path_bound])
            }]))
            .solve(db, &env, &builder, inferable),
            Solutions::Constrained(SolutionPaths::Complete(vec![solution([])]))
        );
    }

    #[test]
    fn default_solve_distinguishes_invalid_bounds_from_never() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let mut bounds = PathBoundBuilder::default();
        bounds.add_lower(
            ConstraintProvenance::Evidence,
            known_instance(db, KnownClass::Int),
        );
        bounds.add_upper(
            ConstraintProvenance::Evidence,
            known_instance(db, KnownClass::Str),
        );
        let invalid = bounds.finish(db, &env, t);
        let inferable = TypeVarSet::from_typevars(db, [t]);

        assert_eq!(
            CandidateSolutions::preliminary_solve(db, &env, &builder, inferable, &invalid),
            PathBoundSolution::Unsatisfiable
        );
        assert_eq!(
            CandidateSolutions::default_solve(db, &env, &builder, inferable, &invalid),
            PathBoundSolution::Unsatisfiable
        );
        assert_eq!(PathBoundSolution::Unsatisfiable.as_type(), None);
        assert_eq!(
            CandidateSolutions::default_solve(
                db,
                &env,
                &builder,
                inferable,
                &PathBound::exact(t, Type::Never)
            ),
            PathBoundSolution::Solved(Type::Never)
        );
        assert_eq!(
            PathBoundSolution::Solved(Type::Never).as_type(),
            Some(Type::Never)
        );
    }

    #[test]
    fn constrained_solutions_respect_inferable_typevars() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);
        let e = create_typevar(db, "E");
        let builder = ConstraintSetBuilder::new();
        let lower = IntersectionType::from_elements(db, &env, [Type::TypeVar(e), str]);
        let upper = UnionType::from_two_elements(db, &env, Type::TypeVar(e), str);

        for constraints in [[int, str], [str, int]] {
            let t = create_typevar(db, "T").map_bound_or_constraints(db, |_| {
                Some(TypeVarBoundOrConstraints::Constraints(
                    TypeVarConstraints::new(db, constraints.as_slice()),
                ))
            });
            for lower_evidence in [false, true] {
                let mut bounds = PathBoundBuilder::default();
                if lower_evidence {
                    bounds.add_lower(ConstraintProvenance::Evidence, lower);
                } else {
                    bounds.add_upper(ConstraintProvenance::Evidence, upper);
                }
                let path_bound = bounds.finish(db, &env, t);

                // Choosing `int` requires narrowing E for `E & str <= T`, or widening E for
                // `T <= E | str`. Neither choice is available when E belongs to the caller.
                for (inferable, expected) in [
                    (TypeVarSet::from_typevars(db, [t]), str),
                    (TypeVarSet::from_typevars(db, [t, e]), constraints[0]),
                ] {
                    assert_eq!(
                        CandidateSolutions::preliminary_solve(
                            db,
                            &env,
                            &builder,
                            inferable,
                            &path_bound
                        ),
                        PathBoundSolution::Solved(expected)
                    );
                    assert_eq!(
                        CandidateSolutions::default_solve(
                            db,
                            &env,
                            &builder,
                            inferable,
                            &path_bound
                        ),
                        PathBoundSolution::Solved(expected)
                    );
                }
            }
        }
    }

    #[test]
    fn constrained_solutions_preserve_inferable_correlations() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let e = create_typevar(db, "E");
        let list = |element| KnownClass::List.to_specialized_instance(db, &env, &[element]);
        let pair = |elements| Type::tuple(TupleType::heterogeneous(db, &env, elements));
        let list_int = list(known_instance(db, KnownClass::Int));
        let list_str = list(known_instance(db, KnownClass::Str));
        let constraints = [pair([list_int, list_str]), pair([list_str, list_int])];
        let t = create_typevar(db, "T").map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::Constraints(
                TypeVarConstraints::new(db, constraints.as_slice()),
            ))
        });
        let inferable = TypeVarSet::from_typevars(db, [t, e]);
        let builder = ConstraintSetBuilder::new();
        let lower = pair([list(Type::TypeVar(e)); 2]);

        // Each tuple position could be satisfied separately, but invariance requires one E
        // to equal both int and str. Neither declared constraint satisfies the whole path.
        let mut bounds = PathBoundBuilder::default();
        bounds.add_lower(ConstraintProvenance::Evidence, lower);
        assert_eq!(
            CandidateSolutions::default_solve(
                db,
                &env,
                &builder,
                inferable,
                &bounds.finish(db, &env, t)
            ),
            PathBoundSolution::ViolatesDeclaredConstraints
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
        let inferable = TypeVarSet::from_typevars(db, [t, u]);

        for lower in [None, Some(Type::any())] {
            let mut bounds = PathBoundBuilder::default();
            if let Some(lower) = lower {
                bounds.add_lower(ConstraintProvenance::Evidence, lower);
            }
            bounds.add_upper(ConstraintProvenance::Evidence, left);
            bounds.add_upper(ConstraintProvenance::Evidence, right);
            let exhausted = bounds.finish(db, &env, t);
            let expected = PathBoundSolution::BudgetExceeded { fallback: lower };
            assert_eq!(
                CandidateSolutions::preliminary_solve(db, &env, &builder, inferable, &exhausted),
                lower.map_or(expected, PathBoundSolution::Solved)
            );
            assert_eq!(
                CandidateSolutions::default_solve(db, &env, &builder, inferable, &exhausted),
                expected
            );
            assert_eq!(expected.as_type(), lower);

            for reverse in [false, true] {
                let mut paths = vec![
                    CandidateSolution {
                        typevars: vec![exhausted.clone(), PathBound::exact(u, str)]
                            .into_boxed_slice(),
                    },
                    CandidateSolution {
                        typevars: vec![PathBound::exact(t, int)].into_boxed_slice(),
                    },
                ];
                let mut recovered = lower
                    .map(|ty| binding(t, ty))
                    .into_iter()
                    .collect::<Vec<_>>();
                recovered.push(binding(u, str));
                let mut expected_paths = vec![solution(recovered), solution([binding(t, int)])];
                if reverse {
                    paths.reverse();
                    expected_paths.reverse();
                }
                assert_eq!(
                    CandidateSolutions::Constrained(paths.into_boxed_slice())
                        .solve(db, &env, &builder, inferable),
                    Solutions::Constrained(SolutionPaths::BudgetExceeded(expected_paths))
                );
            }

            // A later contradiction rejects the entire path, including its exhausted binding.
            let mut invalid = PathBoundBuilder::default();
            invalid.add_lower(ConstraintProvenance::Evidence, int);
            invalid.add_upper(ConstraintProvenance::Evidence, str);
            let invalid = invalid.finish(db, &env, u);
            for invalid_first in [false, true] {
                let mut rejected = vec![exhausted.clone(), invalid.clone()];
                if invalid_first {
                    rejected.reverse();
                }
                let paths = CandidateSolutions::Constrained(Box::new([
                    CandidateSolution {
                        typevars: rejected.into_boxed_slice(),
                    },
                    CandidateSolution {
                        typevars: Box::new([PathBound::exact(t, int)]),
                    },
                ]));
                assert_eq!(
                    paths.solve(db, &env, &builder, inferable),
                    Solutions::Constrained(SolutionPaths::Complete(vec![solution([binding(
                        t, int
                    )])]))
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
        let mut bounds = PathBoundBuilder::default();
        for upper in gradual_upper {
            bounds.add_upper(ConstraintProvenance::Evidence, upper);
        }
        let exhausted = bounds.finish(db, &env, constrained);
        let inferable = TypeVarSet::from_typevars(db, [constrained]);
        assert_eq!(exhausted.has_only_gradual_evidence, Some(true));
        assert_eq!(
            CandidateSolutions::preliminary_solve(db, &env, &builder, inferable, &exhausted),
            PathBoundSolution::BudgetExceeded { fallback: None }
        );
        assert_eq!(
            CandidateSolutions::default_solve(db, &env, &builder, inferable, &exhausted),
            PathBoundSolution::BudgetExceeded { fallback: None }
        );
        Ok(())
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
        ConstraintProvenance,
        Option<Type<'db>>,
        Option<Type<'db>>,
    );

    impl<'db> PermutedConstraint<'db> {
        fn constraints(
            self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
        ) -> impl Iterator<Item = Result<Constraint<'db>, UnsatisfiableBound>> {
            let PermutedConstraint(typevar, provenance, lower, upper) = self;
            iter::chain(
                lower.into_iter().flat_map(move |lower| {
                    Constraint::new_lower_bound(db, provenance, typevar, lower)
                }),
                upper.into_iter().flat_map(move |upper| {
                    Constraint::new_upper_bound(db, env, provenance, typevar, upper)
                }),
            )
        }

        fn node(
            self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            storage: &mut ConstraintSetStorage<'db>,
        ) -> NodeId {
            let constraints = self.constraints(db, env);
            let (node, _) = Constraint::new_nodes(db, env, storage, constraints);
            node
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
                for constraint in atoms[index].constraints(db, &env).filter_map(Result::ok) {
                    storage.intern_constraint(db, &env, constraint);
                }
            }

            let node = build_bdd(&mut storage);
            let source_order = atoms
                .iter()
                .flat_map(|atom| atom.constraints(db, &env).filter_map(Result::ok))
                .fold(None, |source_order, constraint| {
                    let constraint = storage.intern_constraint(db, &env, constraint);
                    let constraint_source_order = storage.constraint_source_order(constraint);
                    storage.ordered_source_order(source_order, Some(constraint_source_order))
                });
            drop(storage);

            let set = ConstraintSet::from_node(&builder, node, source_order);
            let solutions = set.solutions(db, &env, inferable);
            let mut merged = FxHashMap::default();
            if let Ok(Solutions::Constrained(paths)) = &solutions {
                for path in paths.as_slice() {
                    for binding in &path.solved_typevars {
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
                Ok(Solutions::Unsatisfiable(_)) => String::from("unsatisfiable"),
                Ok(Solutions::Unconstrained) => String::from("unconstrained"),
                Ok(Solutions::Constrained(paths)) => paths
                    .as_slice()
                    .iter()
                    .map(|path| {
                        path.solved_typevars
                            .iter()
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
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(str), None),
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(int), None),
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
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(str), None),
            PermutedConstraint(u, ConstraintProvenance::Evidence, Some(bytes), None),
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(int), None),
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
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(str), None),
            PermutedConstraint(u, ConstraintProvenance::Evidence, Some(bytes), None),
            PermutedConstraint(x, ConstraintProvenance::Evidence, Some(int), None),
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
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(str), None),
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(int), None),
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
            PermutedConstraint(t, ConstraintProvenance::Evidence, None, Some(list_u)),
            PermutedConstraint(u, ConstraintProvenance::Evidence, None, Some(int)),
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(list_int), None),
            PermutedConstraint(v, ConstraintProvenance::Evidence, Some(bytes), None),
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
            PermutedConstraint(t, ConstraintProvenance::Evidence, None, Some(int)),
            PermutedConstraint(t, ConstraintProvenance::Evidence, None, Some(str)),
            PermutedConstraint(u, ConstraintProvenance::Evidence, Some(bytes), None),
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
            PermutedConstraint(t, ConstraintProvenance::Evidence, None, Some(int)),
            PermutedConstraint(t, ConstraintProvenance::Evidence, None, Some(str)),
            PermutedConstraint(t, ConstraintProvenance::Evidence, Some(int), None),
            PermutedConstraint(u, ConstraintProvenance::Evidence, None, Some(int)),
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

        for (lower, upper) in [(Some(u), None), (None, Some(u)), (Some(u), Some(u))] {
            let original = ConstraintSetBuilder::new().into_owned(|builder| {
                create_constraint_set_with_bounds(db, &env, builder, t, lower, upper)
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
    fn owned_retirement_profile_bounds_merged_supports_and_shared_arenas() -> anyhow::Result<()> {
        let db = setup_db();
        let env = db.program_environment();
        let word_bits = usize::BITS as usize;
        for count in [
            1,
            word_bits - 1,
            word_bits,
            word_bits + 1,
            2 * word_bits + 1,
        ] {
            let typevars: Vec<_> = (0..count)
                .map(|index| {
                    BoundTypeVarInstance::synthetic(
                        &db,
                        &env,
                        Name::new(format!("T{index}")),
                        TypeVarVariance::Invariant,
                    )
                })
                .collect();
            let owned = ConstraintSetBuilder::new().into_owned(|builder| {
                let mut combined = ConstraintSet::from_bool(builder, true);
                for typevar in &typevars {
                    let next = create_constraint(&db, builder, *typevar, KnownClass::Int);
                    combined = combined.and(&db, builder, || next);
                }
                combined
            });
            let inner = owned
                .inner
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("ordinary conjunction must remain nonterminal"))?;
            assert_eq!(inner.typevars.len(), count);
            let words_per_support = count.div_ceil(word_bits);
            assert!(
                inner
                    .supports
                    .iter()
                    .all(|support| support.words().len() <= words_per_support)
            );
            assert!(
                inner
                    .supports
                    .iter()
                    .any(|support| support.iter().count() == count)
            );
            let scanned_words: usize = inner
                .supports
                .iter()
                .map(|support| support.words().len())
                .sum();
            let support_bound = inner.supports.len() * words_per_support;
            assert!(support_bound >= scanned_words);
            let quote = owned
                .retirement_work()
                .ok_or_else(|| anyhow::anyhow!("fixture retirement quote must fit"))?;
            assert!(quote >= support_bound);

            let shared = owned.clone();
            assert!(
                shared
                    .inner
                    .as_ref()
                    .is_some_and(|other| Arc::ptr_eq(inner, other))
            );
            assert_eq!(shared.retirement_work(), Some(quote));
            drop(owned);
            assert_eq!(shared.retirement_work(), Some(quote));
        }
        assert_eq!(OwnedConstraintSet::always().retirement_work(), Some(1));
        assert_eq!(OwnedConstraintSet::default().retirement_work(), Some(1));
        Ok(())
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
            solution([TypeVarSolution {
                bound_typevar: u,
                solution: known_instance(db, KnownClass::Int),
            }]),
            solution([TypeVarSolution {
                bound_typevar: u,
                solution: known_instance(db, KnownClass::Str),
            }]),
        ])));

        let owned = ConstraintSetBuilder::new().into_owned(|builder| {
            let u_t = ConstraintSet::constrain_typevar_equivalence_bound(
                db,
                &env,
                builder,
                u,
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
                create_constraint_set_with_bounds(db, &env, &builder, typevar, lower, upper)
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
                let later = create_constraint_set_with_bounds(db, &env, builder, t, lower, upper);
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

struct UniqueConstraintScan {
    seen: FxHashSet<NodeId>,
    pending: SmallVec<[NodeId; 8]>,
    phase: UniqueConstraintPhase,
}

#[derive(Clone, Copy)]
enum UniqueConstraintPhase {
    Pop,
    Check { node: NodeId },
    Read { node: NodeId },
    Emit { data: InteriorNodeData },
    Push { data: InteriorNodeData },
    Done,
}

impl UniqueConstraintScan {
    fn new(root: NodeId) -> Self {
        Self {
            seen: FxHashSet::default(),
            pending: SmallVec::from_slice(&[root]),
            phase: UniqueConstraintPhase::Pop,
        }
    }

    fn advance_with<C: TddControl>(
        &mut self,
        storage: &ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<Option<ConstraintId>>, TddError<C::Error>> {
        admit_path_work(PathWork::Advance(PathAdvance::UniqueNode), control)?;
        match self.phase {
            UniqueConstraintPhase::Pop => {
                self.phase = self
                    .pending
                    .pop()
                    .map_or(UniqueConstraintPhase::Done, |node| {
                        UniqueConstraintPhase::Check { node }
                    });
            }
            UniqueConstraintPhase::Check { node } => {
                if node.is_terminal() {
                    self.phase = UniqueConstraintPhase::Pop;
                } else {
                    admit_path_work(PathWork::Access(PathTable::UniqueNodes), control)?;
                    if self.seen.contains(&node) {
                        self.phase = UniqueConstraintPhase::Pop;
                    } else {
                        if self.seen.len() == self.seen.capacity() {
                            let required = self
                                .seen
                                .len()
                                .checked_add(1)
                                .ok_or(TddError::CapacityExhausted)?;
                            let mut plan = sequence_growth::<NodeId, C::Error>(
                                self.seen.capacity(),
                                required,
                            )?;
                            plan.relocation_units = self.seen.len();
                            control.admit(TddWork::Grow {
                                allocation: AllocationKind::UniqueConstraintSeen,
                                plan,
                            })?;
                            self.seen.reserve(plan.requested_capacity - self.seen.len());
                        }
                        self.seen.insert(node);
                        self.phase = UniqueConstraintPhase::Read { node };
                    }
                }
            }
            UniqueConstraintPhase::Read { node } => {
                self.phase = UniqueConstraintPhase::Emit {
                    data: storage.interior_node_data(node),
                };
            }
            UniqueConstraintPhase::Emit { data } => {
                // The ordinary callback runs before extending the child stack.
                self.phase = UniqueConstraintPhase::Push { data };
                return Ok(ControlFlow::Break(Some(data.constraint)));
            }
            UniqueConstraintPhase::Push { data } => {
                reserve_smallvec(
                    &mut self.pending,
                    3,
                    AllocationKind::UniqueConstraintStack,
                    control,
                )?;
                self.pending
                    .extend([data.if_false, data.if_uncertain, data.if_true]);
                self.phase = UniqueConstraintPhase::Pop;
            }
            UniqueConstraintPhase::Done => return Ok(ControlFlow::Break(None)),
        }
        Ok(ControlFlow::Continue(()))
    }
}

// Source-order sidecars share interned subtrees. Revisiting a subtree cannot contribute
// an earlier occurrence of any constraint, and can expand a small DAG exponentially.
struct SourceOrderScan {
    visited: FxHashSet<SourceOrderId>,
    pending: Vec<SourceOrderId>,
    result: FxIndexSet<ConstraintId>,
    phase: SourceOrderPhase,
}

#[derive(Clone, Copy)]
enum SourceOrderPhase {
    Seed {
        root: Option<SourceOrderId>,
    },
    Pop,
    Check {
        current: SourceOrderId,
    },
    Read {
        current: SourceOrderId,
    },
    Push {
        left: SourceOrderId,
        right: SourceOrderId,
    },
    Insert {
        constraint: ConstraintId,
    },
    Done,
}

impl SourceOrderScan {
    fn new(root: Option<SourceOrderId>) -> Self {
        Self {
            visited: FxHashSet::default(),
            pending: Vec::new(),
            result: FxIndexSet::default(),
            phase: SourceOrderPhase::Seed { root },
        }
    }

    fn advance_with<C: TddControl>(
        &mut self,
        storage: &ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<()>, TddError<C::Error>> {
        admit_path_work(PathWork::Advance(PathAdvance::SourceOrder), control)?;
        match self.phase {
            SourceOrderPhase::Seed { root } => {
                if let Some(root) = root {
                    reserve_vec(
                        &mut self.pending,
                        1,
                        AllocationKind::SourceOrderScanStack,
                        control,
                    )?;
                    self.pending.push(root);
                }
                self.phase = SourceOrderPhase::Pop;
            }
            SourceOrderPhase::Pop => {
                self.phase = self
                    .pending
                    .pop()
                    .map_or(SourceOrderPhase::Done, |current| SourceOrderPhase::Check {
                        current,
                    });
            }
            SourceOrderPhase::Check { current } => {
                admit_path_work(PathWork::Access(PathTable::SourceOrderSeen), control)?;
                if self.visited.contains(&current) {
                    self.phase = SourceOrderPhase::Pop;
                } else {
                    if self.visited.len() == self.visited.capacity() {
                        let required = self
                            .visited
                            .len()
                            .checked_add(1)
                            .ok_or(TddError::CapacityExhausted)?;
                        let mut plan = sequence_growth::<SourceOrderId, C::Error>(
                            self.visited.capacity(),
                            required,
                        )?;
                        plan.relocation_units = self.visited.len();
                        control.admit(TddWork::Grow {
                            allocation: AllocationKind::SourceOrderScanSeen,
                            plan,
                        })?;
                        self.visited
                            .reserve(plan.requested_capacity - self.visited.len());
                    }
                    self.visited.insert(current);
                    self.phase = SourceOrderPhase::Read { current };
                }
            }
            SourceOrderPhase::Read { current } => {
                self.phase = match storage.source_order_data(current) {
                    SourceOrder::Ordered(left, right) => SourceOrderPhase::Push { left, right },
                    SourceOrder::Constraint(constraint) => SourceOrderPhase::Insert { constraint },
                };
            }
            SourceOrderPhase::Push { left, right } => {
                reserve_vec(
                    &mut self.pending,
                    2,
                    AllocationKind::SourceOrderScanStack,
                    control,
                )?;
                self.pending.extend([right, left]);
                self.phase = SourceOrderPhase::Pop;
            }
            SourceOrderPhase::Insert { constraint } => {
                admit_path_work(PathWork::Access(PathTable::SourceOrderResult), control)?;
                if !self.result.contains(&constraint) {
                    if self.result.len() == self.result.capacity() {
                        let required = self
                            .result
                            .len()
                            .checked_add(1)
                            .ok_or(TddError::CapacityExhausted)?;
                        let mut plan = sequence_growth::<ConstraintId, C::Error>(
                            self.result.capacity(),
                            required,
                        )?;
                        plan.relocation_units = self.result.len();
                        control.admit(TddWork::Grow {
                            allocation: AllocationKind::SourceOrderScanResult,
                            plan,
                        })?;
                        self.result
                            .reserve(plan.requested_capacity - self.result.len());
                    }
                    self.result.insert(constraint);
                }
                self.phase = SourceOrderPhase::Pop;
            }
            SourceOrderPhase::Done => return Ok(ControlFlow::Break(())),
        }
        Ok(ControlFlow::Continue(()))
    }
}

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
struct SingleConjunctionScan {
    phase: SingleConjunctionPhase,
}

#[derive(Clone, Copy)]
enum SingleConjunctionPhase {
    Current(NodeId),
    Done(bool),
}

impl SingleConjunctionScan {
    fn new(root: NodeId) -> Self {
        Self {
            phase: SingleConjunctionPhase::Current(root),
        }
    }

    fn advance_with<C: TddControl>(
        &mut self,
        storage: &ConstraintSetStorage<'_>,
        control: &mut C,
    ) -> Result<ControlFlow<bool>, TddError<C::Error>> {
        admit_path_work(PathWork::Advance(PathAdvance::ConjunctionShape), control)?;
        match self.phase {
            SingleConjunctionPhase::Done(result) => return Ok(ControlFlow::Break(result)),
            SingleConjunctionPhase::Current(node) => {
                self.phase = match node.node() {
                    Node::AlwaysTrue => SingleConjunctionPhase::Done(true),
                    Node::AlwaysFalse => SingleConjunctionPhase::Done(false),
                    Node::Interior(_) => {
                        let data = storage.interior_node_data(node);
                        if (data.if_true != ALWAYS_FALSE && data.if_false != ALWAYS_FALSE)
                            || data.if_uncertain != ALWAYS_FALSE
                        {
                            SingleConjunctionPhase::Done(false)
                        } else {
                            SingleConjunctionPhase::Current(if data.if_true != ALWAYS_FALSE {
                                data.if_true
                            } else {
                                data.if_false
                            })
                        }
                    }
                };
            }
        }
        Ok(ControlFlow::Continue(()))
    }
}

fn independent_pair_skip_with<C: TddControl>(
    storage: &ConstraintSetStorage<'_>,
    existing: ConstraintId,
    current: ConstraintId,
    independent: &FxHashSet<TypeVarId>,
    control: &mut C,
) -> Result<bool, TddError<C::Error>> {
    let existing = storage.constraint_support(existing);
    let current = storage.constraint_support(current);
    admit_path_work(
        PathWork::SupportScan {
            words: existing.words().len().min(current.words().len()),
            typevars: 0,
        },
        control,
    )?;
    if existing.overlaps_with(current) {
        return Ok(false);
    }
    admit_path_work(
        PathWork::SupportScan {
            words: existing.words().len(),
            typevars: 0,
        },
        control,
    )?;
    admit_path_work(
        PathWork::SupportScan {
            words: current.words().len(),
            typevars: 0,
        },
        control,
    )?;
    for typevar in existing.iter().chain(current.iter()) {
        admit_path_work(
            PathWork::SupportScan {
                words: 0,
                typevars: 1,
            },
            control,
        )?;
        admit_path_work(PathWork::Access(PathTable::IndependentTypevars), control)?;
        if independent.contains(&typevar) {
            return Ok(existing.is_complete() && current.is_complete());
        }
    }
    Ok(false)
}

fn extend_dependent_support_with<C: TddControl>(
    storage: &ConstraintSetStorage<'_>,
    constraint: ConstraintId,
    dependent: &mut FxHashSet<TypeVarId>,
    control: &mut C,
) -> Result<(), TddError<C::Error>> {
    let support = storage.constraint_support(constraint);
    admit_path_work(
        PathWork::SupportScan {
            words: support.words().len(),
            typevars: 0,
        },
        control,
    )?;
    for typevar in support.iter() {
        admit_path_work(
            PathWork::SupportScan {
                words: 0,
                typevars: 1,
            },
            control,
        )?;
        admit_path_work(PathWork::Access(PathTable::DependentTypevars), control)?;
        if !dependent.contains(&typevar) {
            paths::reserve_path_typevars_with(dependent, PathTypevarSet::Dependent, control)?;
            dependent.insert(typevar);
        }
    }
    Ok(())
}
