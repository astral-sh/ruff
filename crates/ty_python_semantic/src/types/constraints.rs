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
//! An atomic constraint restricts the specialization of one or two variables. When collecting
//! constraints for inference, only inferable variables can receive bounds. A fixed variable can
//! still occur as a symbolic type in a bound. For example, comparing fixed `U` against inferable
//! `T` for `U ≤ T` produces a concrete lower bound on `T`. A type-variable range would give both
//! `T` a lower bound and `U` an upper bound, so it requires both variables to be inferable.
//!
//! You can build up more complex constraint sets using union, intersection, and negation. We use
//! a ternary decision diagram (TDD), as described in §11.2 of [Duboc's thesis][duboc], to represent
//! a constraint set.
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

use std::borrow::Cow;
use std::cell::{Cell, LazyCell, RefCell};
use std::cmp::Ordering;
use std::convert::Infallible;
use std::fmt::{Debug, Display};
use std::iter;
use std::marker::PhantomData;
use std::ops::{ControlFlow, Range};
use std::sync::{Arc, LazyLock};

use ruff_index::{Idx, IndexVec, newtype_index};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::Program;
use ty_python_core::rank::RankBitBox;
use ty_static::EnvVars;

use crate::types::constraints::projection::{ProjectionError, SolutionBudget};
use crate::types::constraints::support::{Support, SupportId};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::relation::TypeVarEvaluation;
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarConstraints, TypeVarDomain, TypeVarSet};
use crate::types::visitor::{
    NonAtomicType, TypeKind, TypeVisitor, any_over_type_expanding_aliases,
    may_contain_typevar_matching, walk_non_atomic_type,
};
use crate::types::{
    BoundTypeVarInstance, IntersectionType, Type, TypeVarBoundOrConstraints, TypeVarVariance,
    UnionType,
};
use crate::{Db, FxIndexMap, FxIndexSet, FxOrderSet, ProgramEnvironment};

mod owned;
pub(crate) mod paths;
pub(crate) mod projection;
pub(crate) mod resolution;
mod sequents;
mod solutions;
mod support;
mod variables;

use owned::OwnedConstraintSetBuilder;
use paths::PathAssignments;
use solutions::{Polarity, SolutionWalker};
pub(crate) use variables::ConstraintProvenance;
use variables::{AtomicConstraint, Constraint, UnspecializedTypeVarPolicy};

impl<'db> TypeVarSet<'db> {
    /// Returns the binders brought into scope by each `ParamSpec` in this set. A captured
    /// generic callable brings its own binders into scope when comparing its parameters
    /// with other bounds, such as the arguments forwarded by `fn(*args, **kwargs)`.
    ///
    /// TODO: We anticipate that this method can be removed as part of the scoped
    /// existentials work [1]. For example, suppose `P` captures the parameters
    /// of `def identity[T](x: T, /) -> T` when we match it against `Callable[P, R]`.
    /// We could express that match as `∃ T. (P = (x: T, /) ∧ T ≤ R)`.
    ///
    /// If the capture retains that existential scope, we can open it when checking
    /// a forwarded call. The argument then constrains the same `T` that determines
    /// the callback's return type, without us having to search the constraints to
    /// rediscover which type variables belong to the capture.
    ///
    /// [1]: https://github.com/astral-sh/ruff/pull/29027
    fn captured_typevars<I: IntoIterator<Item = AtomicConstraintId>>(
        self,
        db: &'db dyn Db,
        storage: &ConstraintSetStorage<'db>,
        constraints: impl Fn() -> I,
    ) -> Self {
        let mut contexts = SmallVec::<[GenericContext<'db>; 1]>::new();
        let mut seen_paramspecs = FxHashSet::default();
        let mut owners = self.explicit_typevars();
        while owners.iter(db).any(|typevar| typevar.is_paramspec(db)) {
            let start = contexts.len();
            for id in constraints() {
                if let Some((parameter, Type::Callable(captured))) =
                    storage.atomic_constraint_data(id).as_concrete()
                    && parameter.is_paramspec(db)
                    && parameter.is_inferable(db, owners)
                {
                    contexts.extend(
                        captured
                            .signatures(db)
                            .iter()
                            .filter_map(|signature| signature.generic_context),
                    );
                }
            }
            // A captured signature can bind a ParamSpec that captures another generic signature.
            // Only newly discovered ParamSpecs need another pass over the constraints.
            owners = Self::from_typevars(
                db,
                contexts[start..]
                    .iter()
                    .flat_map(|context| context.variables(db))
                    .filter(|var| var.is_paramspec(db) && seen_paramspecs.insert(var.identity(db))),
            );
        }
        Self::from_contexts(db, &contexts)
    }

    /// Returns the variables in this finite set that are absent from `other`.
    pub(super) fn difference(self, db: &'db dyn Db, other: Self) -> Self {
        if other == Self::None {
            return self;
        }
        Self::from_typevars(db, self.iter(db).filter(|var| !var.is_inferable(db, other)))
    }
}

/// An extension trait for building constraint sets from [`Option`] values.
pub(crate) trait OptionConstraintsExtension<T> {
    /// Returns a constraint set that is always satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_none_or<'db, 'c>(
        self,
        db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c>;

    /// Returns a constraint set that is never satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_some_and<'db, 'c>(
        self,
        db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c>;
}

impl<T> OptionConstraintsExtension<T> for Option<T> {
    fn when_none_or<'db, 'c>(
        self,
        _db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::always(build.builder),
        }
    }

    fn when_some_and<'db, 'c>(
        self,
        _db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        f: impl FnOnce(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::never(build.builder),
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
        build: Construction<'_, 'c, 'db>,
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
        build: Construction<'_, 'c, 'db>,
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
        build: Construction<'_, 'c, 'db>,
        f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let result = NodeId::tree_fold(build, self.map(f), ALWAYS_FALSE, ALWAYS_TRUE, NodeId::or);
        ConstraintSet::from_result(build.builder, result)
    }

    fn when_all<'db, 'c>(
        self,
        _db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        f: impl FnMut(T) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let result = NodeId::tree_fold(build, self.map(f), ALWAYS_TRUE, ALWAYS_FALSE, NodeId::and);
        ConstraintSet::from_result(build.builder, result)
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
/// [`load`][Construction::load] to remap it into an existing builder.
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

    /// Loads this constraint set into a new builder, invokes a callback with that builder, and
    /// returns the result.
    ///
    /// This is more efficient than [`Construction::load`] when this is the only set you
    /// need to load into the new builder.
    pub(crate) fn query<F, R>(&self, f: F) -> R
    where
        F: for<'c> FnOnce(&'c ConstraintSetBuilder<'db>, ConstraintSet<'db, 'c>) -> R,
    {
        let builder = ConstraintSetBuilder::new();
        if let Some(inner) = &self.inner {
            builder.storage.borrow_mut().compacted = Some(Arc::clone(inner));
        }
        let set = ConstraintSet::from_node(&builder, self.node, self.source_order);
        f(&builder, set)
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
pub enum ConstraintSet<'db, 'c> {
    Valid(ConstraintDiagram<'db, 'c>),
    /// No partial diagram or inference evidence may be used.
    BudgetExceeded,
}

#[derive(Clone, Copy)]
pub struct ConstraintDiagram<'db, 'c> {
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
        Self::Valid(ConstraintDiagram {
            node,
            source_order,
            builder,
            _invariant: PhantomData,
        })
    }

    fn from_result(
        builder: &'c ConstraintSetBuilder<'db>,
        result: Result<(NodeId, Option<SourceOrderId>), ProjectionError>,
    ) -> Self {
        match result {
            Ok((node, order)) => Self::from_node(builder, node, order),
            Err(_) => Self::BudgetExceeded,
        }
    }

    pub(super) fn diagram(self) -> Result<ConstraintDiagram<'db, 'c>, ProjectionError> {
        match self {
            Self::Valid(diagram) => Ok(diagram),
            Self::BudgetExceeded => Err(ProjectionError::ConstructionBudgetExceeded),
        }
    }

    pub(super) fn is_budget_exceeded(self) -> bool {
        matches!(self, Self::BudgetExceeded)
    }

    /// No further conjuncts need to be evaluated.
    pub(super) fn short_circuits_and(self) -> bool {
        self.is_budget_exceeded() || self.is_trivially_never_satisfied()
    }

    /// No further alternatives need to be evaluated.
    pub(super) fn short_circuits_or(self) -> bool {
        self.is_budget_exceeded() || self.is_trivially_always_satisfied()
    }

    pub(super) fn check_budget(self) -> Result<Self, ProjectionError> {
        self.diagram().map(Self::Valid)
    }

    pub(super) fn never(builder: &'c ConstraintSetBuilder<'db>) -> Self {
        Self::from_node(builder, ALWAYS_FALSE, None)
    }

    pub(super) fn always(builder: &'c ConstraintSetBuilder<'db>) -> Self {
        Self::from_node(builder, ALWAYS_TRUE, None)
    }

    pub(super) fn from_bool(builder: &'c ConstraintSetBuilder<'db>, b: bool) -> Self {
        if b {
            Self::always(builder)
        } else {
            Self::never(builder)
        }
    }

    /// Returns a constraint set that constrains a typevar to an explicit range of types.
    #[expect(clippy::too_many_arguments)]
    pub(crate) fn constrain_typevar(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, 'c, 'db>,
        provenance: ConstraintProvenance,
        inferable: TypeVarSet<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Self {
        if lower == upper {
            return Self::constrain_typevar_equivalence_bound(
                db, env, build, provenance, inferable, typevar, lower,
            );
        }

        let mut storage = build.builder.storage.borrow_mut();
        let constraints = iter::chain(
            AtomicConstraint::new_lower_bound(db, provenance, typevar, lower, inferable),
            AtomicConstraint::new_upper_bound(db, env, provenance, typevar, upper, inferable),
        );
        Self::from_result(
            build.builder,
            Constraint::new_nodes(db, env, &mut storage, constraints, build.budget),
        )
    }

    /// Returns a constraint set that constrains a typevar to be a supertype of `lower`.
    pub(crate) fn constrain_typevar_lower_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, 'c, 'db>,
        provenance: ConstraintProvenance,
        inferable: TypeVarSet<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
    ) -> Self {
        let mut storage = build.builder.storage.borrow_mut();
        let constraints =
            AtomicConstraint::new_lower_bound(db, provenance, typevar, lower, inferable);
        Self::from_result(
            build.builder,
            Constraint::new_nodes(db, env, &mut storage, constraints, build.budget),
        )
    }

    /// Returns a constraint set that constrains a typevar to be a subtype of `upper`.
    pub(crate) fn constrain_typevar_upper_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, 'c, 'db>,
        provenance: ConstraintProvenance,
        inferable: TypeVarSet<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        let mut storage = build.builder.storage.borrow_mut();
        let constraints =
            AtomicConstraint::new_upper_bound(db, env, provenance, typevar, upper, inferable);
        Self::from_result(
            build.builder,
            Constraint::new_nodes(db, env, &mut storage, constraints, build.budget),
        )
    }

    /// Returns a constraint set that constrains a typevar to be equivalent to `bound`.
    pub(crate) fn constrain_typevar_equivalence_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, 'c, 'db>,
        provenance: ConstraintProvenance,
        inferable: TypeVarSet<'db>,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        let mut storage = build.builder.storage.borrow_mut();
        // Intern typevar first so that if the resulting bound happens to be a
        // TypeVarEquivalenceBound, we'll intern the left/right typevars in a builder-specific
        // stable order.
        storage.intern_typevar(db, typevar);
        let constraints =
            AtomicConstraint::new_equivalence_bound(db, env, provenance, typevar, bound, inferable);
        Self::from_result(
            build.builder,
            Constraint::new_nodes(db, env, &mut storage, constraints, build.budget),
        )
    }

    /// Verifies that this constraint set was created by `builder`
    #[track_caller]
    fn verify_builder(self, builder: &'c ConstraintSetBuilder<'db>) {
        if let Self::Valid(diagram) = self {
            debug_assert!(std::ptr::eq(diagram.builder, builder));
        }
    }

    /// Returns whether this constraint set never holds, without checking the type variables'
    /// declared bounds or constraints. Use [`Self::try_has_no_valid_solutions`] to include those.
    ///
    /// Checks for conflicting inferred bounds on the variables in `inferable`. For a budgeted
    /// relation, use [`Self::try_is_never_satisfied`] to propagate exhaustion.
    pub(crate) fn is_never_satisfied(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
    ) -> bool {
        self.try_is_never_satisfied(db, env, inferable, &ConstructionBudget::Unlimited)
            .expect("exact relation")
    }

    pub(super) fn try_is_never_satisfied(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: &ConstructionBudget,
    ) -> Result<bool, ProjectionError> {
        let limits = SolutionLimits::unbounded(budget);
        self.is_unsatisfiable(db, env, inferable, Polarity::Positive, limits)
    }

    /// Check whether this diagram (or its negation) has no satisfying assignments.
    fn is_unsatisfiable(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        polarity: Polarity,
        limits: SolutionLimits<'_>,
    ) -> Result<bool, ProjectionError> {
        let diagram = self.diagram()?;
        match diagram.node.node() {
            Node::AlwaysTrue => return Ok(polarity == Polarity::Negative),
            Node::AlwaysFalse => return Ok(polarity == Polarity::Positive),
            Node::Interior(_) => {}
        }
        diagram.node.is_unsatisfiable(
            db,
            env,
            &mut diagram.builder.storage.borrow_mut(),
            inferable,
            diagram.source_order,
            polarity,
            limits,
        )
    }

    /// Like [`Self::is_never_satisfied`], but returns `None` if the default solution budget is
    /// exhausted before the result is known.
    pub(super) fn is_never_satisfied_bounded(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Option<bool> {
        let limits =
            SolutionLimits::bounded(SolutionBudget::default(), &ConstructionBudget::Unlimited);
        self.is_unsatisfiable(db, env, inferable, Polarity::Positive, limits)
            .ok()
    }

    /// Returns whether no specialization satisfying the type variables' upper bounds and
    /// constraints can satisfy this constraint set.
    ///
    /// Unlike [`Self::is_never_satisfied`], this validates solutions against the type variables'
    /// upper bounds and constraints. For example, `T = int` is not contradictory by itself, but has
    /// no valid solution if `T` has an upper bound of `str`.
    ///
    /// Exhausting the construction budget returns an error. An inconclusive search within
    /// the solver's own limits returns `Ok(false)` because it cannot rule out a valid solution.
    pub(crate) fn try_has_no_valid_solutions(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        construction: &ConstructionBudget,
    ) -> Result<bool, ProjectionError> {
        // For overlap, every mentioned type variable can choose a valid specialization.
        let inferable = TypeVarSet::All;
        if self.try_is_never_satisfied(db, env, inferable, construction)? {
            return Ok(true);
        }

        // Comparing invariant arguments like `Literal["a", "b", ...]` and `T | U`
        // can produce a distinct specialization for every partition of the literals
        // between T and U. A single valid solution establishes overlap.
        match self.try_solution_count_up_to(db, env, inferable, 1, construction) {
            Err(error @ ProjectionError::ConstructionBudgetExceeded) => Err(error),
            result => Ok(matches!(result, Ok(Some(0)))),
        }
    }

    /// Counts paths accepted by the default selector, up to `limit`.
    pub(super) fn try_solution_count_up_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        limit: usize,
        construction: &ConstructionBudget,
    ) -> Result<Option<usize>, ProjectionError> {
        let diagram = self.diagram()?;
        let count = |solutions| match solutions {
            Solutions::Unconstrained => None,
            Solutions::Unsatisfiable(_) => Some(0),
            Solutions::Constrained(paths) => Some(paths.as_slice().len().min(limit)),
        };
        let budget = SolutionBudget::default();
        let candidates = CandidateSolutions::compute(
            db,
            env,
            self,
            inferable,
            SolutionLimits::bounded(budget, construction),
            UnspecializedTypeVarPolicy::Ignore,
            CandidateSearch::FirstValid(limit),
        )?;
        let build = diagram.builder.with_budget(construction);
        let result = candidates.solve_with_budget(db, budget, |_variance, bounds| {
            CandidateSolutions::try_solve_with_preference(db, env, build, bounds, true)
        });
        if let Err(error @ ProjectionError::ConstructionBudgetExceeded) = result {
            return Err(error);
        }
        let selected = result.ok().and_then(count);

        // Selection can reject candidates after the walker has validated their declared
        // domains. If the walk reached its limit, we only know the count when selection
        // confirms that many solutions. Otherwise check all paths with the usual budget.
        if selected != Some(limit)
            && let CandidateSolutions::Constrained(paths) = &candidates
            && paths
                .iter()
                .filter(|path| matches!(path.validity, SolutionValidity::Valid))
                .count()
                >= limit
        {
            return self
                .solutions_during_construction(db, env, inferable, construction)
                .map(count);
        }
        Ok(selected)
    }

    /// Returns whether this constraint set is the `never` terminal.
    ///
    /// A nonterminal constraint set can also never be satisfied, so `false` does not prove that
    /// the set is satisfiable. Use [`Self::is_never_satisfied`] when false negatives are not
    /// acceptable.
    pub(crate) fn is_trivially_never_satisfied(self) -> bool {
        matches!(self, Self::Valid(diagram) if diagram.node == ALWAYS_FALSE)
    }

    /// Returns whether this constraint set always holds, by checking that its negation is never
    /// satisfied with the given inferable type variables. For a budgeted relation, use
    /// [`Self::try_is_always_satisfied`] to propagate exhaustion.
    #[inline]
    pub(crate) fn is_always_satisfied(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
    ) -> bool {
        self.try_is_always_satisfied(db, env, inferable, &ConstructionBudget::Unlimited)
            .expect("exact relation")
    }

    pub(super) fn try_is_always_satisfied(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        budget: &ConstructionBudget,
    ) -> Result<bool, ProjectionError> {
        let limits = SolutionLimits::unbounded(budget);
        self.is_unsatisfiable(db, env, inferable, Polarity::Negative, limits)
    }

    /// Checks a relation collected with all variables inferable in a disposable builder.
    ///
    /// On an all-negative path, the only rule that can cause a conflict is a tautology.
    /// Every other sequent needs a positive premise, and only positive bounds contribute
    /// to candidate solutions. In a shared builder, discovering those sequents can still
    /// intern constraints whose order matters to later bounded searches.
    pub(super) fn is_always_satisfied_in_isolation(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        let diagram = self.diagram().expect("exact relation");
        if !diagram.node.is_terminal() {
            let storage = diagram.builder.storage.borrow();
            if let Some(atoms) = diagram.node.all_negative_path_to_false(&storage)
                && atoms
                    .iter()
                    .all(|&atom| !storage.atomic_constraint_data(atom).is_tautology(db))
            {
                return false;
            }
        }
        self.is_always_satisfied(db, env, TypeVarSet::All)
    }

    /// Like [`Self::is_always_satisfied`], but returns `None` if the default solution budget is
    /// exhausted before the result is known.
    pub(super) fn is_always_satisfied_bounded(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Option<bool> {
        let limits =
            SolutionLimits::bounded(SolutionBudget::default(), &ConstructionBudget::Unlimited);
        self.is_unsatisfiable(db, env, inferable, Polarity::Negative, limits)
            .ok()
    }

    /// Returns whether this constraint set is the `always` terminal.
    ///
    /// A nonterminal constraint set can also always be satisfied, so `false` does not prove that
    /// the set is not always satisfied. Use [`Self::is_always_satisfied`] when false negatives are
    /// not acceptable.
    pub(crate) fn is_trivially_always_satisfied(self) -> bool {
        matches!(self, Self::Valid(diagram) if diagram.node == ALWAYS_TRUE)
    }

    /// Checks the scope supplied to the internal solver test helpers. A fixed variable may
    /// appear inside a bound, but it cannot be the subject of a reachable constraint.
    pub(super) fn has_inferable_subjects(
        self,
        db: &'db dyn Db,
        inferable: TypeVarSet<'db>,
    ) -> bool {
        let diagram = self.diagram().expect("completed relation");
        let storage = diagram.builder.storage.borrow();
        let constraints = diagram.node.atomic_constraints(&storage);
        let captures = inferable.captured_typevars(db, &storage, || constraints.iter().copied());
        let scope = inferable.merge(db, captures);
        constraints.iter().all(|&id| {
            storage
                .atomic_constraint_data(id)
                .subjects()
                .all(|subject| subject.is_inferable(db, scope))
        })
    }

    /// Returns the constraints under which `lhs` is a subtype of `rhs`, assuming that the
    /// constraints in this constraint set hold.
    pub(crate) fn implies_subtype_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, 'c, 'db>,
        lhs: Type<'db>,
        rhs: Type<'db>,
    ) -> Self {
        self.implies(db, build, || {
            lhs.when_constraint_set_subtype_of(db, env, rhs, build, TypeVarSet::All)
        })
    }

    /// Both inputs have already been evaluated. Propagate exhaustion even when
    /// the other operand is a terminal that would have short-circuited a lazy operation.
    fn combine(
        self,
        build: Construction<'_, 'c, 'db>,
        other: Self,
        identity: Option<NodeId>,
        apply: impl FnOnce(
            NodeId,
            &mut ConstraintSetStorage<'db>,
            NodeId,
            &ConstructionBudget,
        ) -> Result<NodeId, ProjectionError>,
    ) -> Self {
        self.verify_builder(build.builder);
        other.verify_builder(build.builder);
        let (Self::Valid(left), Self::Valid(right)) = (self, other) else {
            return Self::BudgetExceeded;
        };
        if Some(left.node) == identity && left.source_order.is_none() {
            return other;
        }
        if Some(right.node) == identity && right.source_order.is_none() {
            return self;
        }
        let mut storage = build.builder.storage.borrow_mut();
        let Ok(node) = apply(left.node, &mut storage, right.node, build.budget) else {
            return Self::BudgetExceeded;
        };
        let order = storage.ordered_source_order(left.source_order, right.source_order);
        Self::from_node(build.builder, node, order)
    }

    /// Updates this constraint set to hold the union of itself and another constraint set.
    ///
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn union(
        &mut self,
        _db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        other: Self,
    ) -> Self {
        *self = self.combine(build, other, Some(ALWAYS_FALSE), NodeId::or);
        *self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    ///
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn intersect(
        &mut self,
        _db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        other: Self,
    ) -> Self {
        *self = self.combine(build, other, Some(ALWAYS_TRUE), NodeId::and);
        *self
    }

    /// Returns the negation of this constraint set.
    pub(crate) fn negate(self, _db: &'db dyn Db, build: Construction<'_, 'c, 'db>) -> Self {
        self.verify_builder(build.builder);
        let Self::Valid(diagram) = self else {
            return self;
        };
        let node = diagram
            .node
            .negate(&mut build.builder.storage.borrow_mut(), build.budget);
        Self::from_result(build.builder, node.map(|node| (node, diagram.source_order)))
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
        build: Construction<'_, 'c, 'db>,
        other: impl FnOnce() -> Self,
    ) -> Self {
        self.verify_builder(build.builder);
        if self.short_circuits_and() {
            return self;
        }
        let other = other();
        // Protocol member comparisons often succeed without adding any constraints.
        if matches!(self, Self::Valid(left) if left.node == ALWAYS_TRUE && left.source_order.is_none())
        {
            other.verify_builder(build.builder);
            return other;
        }
        self.intersect(db, build, other)
    }

    /// Returns the union of this constraint set and another. The other constraint set is provided
    /// as a thunk, to implement short-circuiting: the thunk is not forced if the constraint set is
    /// already saturated.
    ///
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn or(
        mut self,
        db: &'db dyn Db,
        build: Construction<'_, 'c, 'db>,
        other: impl FnOnce() -> Self,
    ) -> Self {
        self.verify_builder(build.builder);
        if !self.short_circuits_or() {
            self.union(db, build, other());
        }
        self
    }

    /// Returns a constraint set encoding that this constraint set implies another.
    ///
    /// In the result's source order, `self` will appear before `other`.
    pub(crate) fn implies(
        self,
        db: &'db dyn Db,
        builder: Construction<'_, 'c, 'db>,
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
        build: Construction<'_, 'c, 'db>,
        other: Self,
    ) -> Self {
        self.combine(build, other, None, NodeId::iff)
    }

    /// Returns whether the two constraint sets hold for exactly the same assignments.
    pub(super) fn is_equivalent_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Self,
        inferable: TypeVarSet<'db>,
        budget: &ConstructionBudget,
    ) -> Result<bool, ProjectionError> {
        let left = self.diagram()?;
        let right = other.diagram()?;
        other.verify_builder(left.builder);
        let builder = left.builder.with_budget(budget);
        // Look for a counterexample in each direction. Building the full equivalence diagram
        // combines both sets of paths before the solver can rule either direction out.
        let is_subset = |left: Self, right: Self| {
            left.and(db, builder, || right.negate(db, builder))
                .try_is_never_satisfied(db, env, inferable, budget)
        };
        Ok(left.node == right.node || (is_subset(self, other)? && is_subset(other, self)?))
    }

    /// Projects out callable-local typevars that the outer solve cannot recover from a captured
    /// `ParamSpec`. A failed prefix check or a relation between two `ParamSpec` variables can
    /// produce constraints without a captured signature.
    pub(super) fn reduce_uncaptured_typevars(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        outer: TypeVarSet<'db>,
        mut candidates: TypeVarSet<'db>,
        budget: &ConstructionBudget,
    ) -> Self {
        // A local can occur free in another captured parameter list. Projecting it out
        // removes that capture too, so check which binders the caller can still reach.
        while let Self::Valid(diagram) = self
            && !diagram.node.is_terminal()
            && candidates != outer
        {
            let retained = outer.merge(db, diagram.captured_typevars(db, outer));
            let unreachable = candidates.difference(db, retained);
            if unreachable == TypeVarSet::None {
                break;
            }

            // Project a ParamSpec before its binders so its signature can transfer to a
            // surviving ParamSpec. If every unreachable variable is itself captured,
            // there is a cycle and we project the whole unreachable set together.
            let captured = diagram.captured_typevars(db, candidates);
            let roots = unreachable.difference(db, captured);
            let to_remove = if roots == TypeVarSet::None {
                unreachable
            } else {
                roots
            };
            let body_scope = candidates.merge(db, captured);
            (self, candidates) = diagram.project_typevars(db, env, to_remove, body_scope, budget);
            candidates = candidates.difference(db, to_remove);
        }
        self
    }

    /// Reduces the set of inferable typevars for this constraint set. You provide the typevars that
    /// were inferable when this constraint set was created, and which should be abstracted away.
    /// Those typevars will be removed from the constraint set, and the constraint set will return
    /// true whenever there was _any_ specialization of those typevars that returned true before.
    /// `inferable` includes all callable-local typevars that were inferable while collecting the body.
    pub(crate) fn reduce_inferable(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, 'c, 'db>,
        to_remove: TypeVarSet<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Self {
        self.verify_builder(build.builder);
        let Self::Valid(diagram) = self else {
            return self;
        };
        if to_remove == TypeVarSet::None {
            return self;
        }
        let scope = inferable.merge(db, diagram.captured_typevars(db, inferable));
        let (projected, candidates) =
            diagram.project_typevars(db, env, to_remove, scope, build.budget);
        // Once we know which captures survive projection, eliminate any binders
        // that are no longer in scope through a surviving ParamSpec.
        projected.reduce_uncaptured_typevars(db, env, inferable, candidates, build.budget)
    }
}

impl<'db, 'c> ConstraintDiagram<'db, 'c> {
    /// Returns whether this constraint set mentions the given type-variable identity.
    pub(super) fn mentions_typevar(self, typevar: BoundTypeVarInstance<'db>) -> bool {
        let storage = self.builder.storage.borrow();
        storage
            .node_support(self.node)
            .is_some_and(|support| support.iter().any(|id| storage.typevar_data(id) == typevar))
    }

    fn captured_typevars(self, db: &'db dyn Db, inferable: TypeVarSet<'db>) -> TypeVarSet<'db> {
        let storage = self.builder.storage.borrow();
        let constraints = LazyCell::new(|| self.node.atomic_constraints(&storage));
        inferable.captured_typevars(db, &storage, || constraints.iter().copied())
    }

    /// Returns the projection and the variables in scope during it, including any
    /// binders introduced while deriving constraints.
    fn project_typevars(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        to_remove: TypeVarSet<'db>,
        inferable: TypeVarSet<'db>,
        budget: &ConstructionBudget,
    ) -> (ConstraintSet<'db, 'c>, TypeVarSet<'db>) {
        let builder = self.builder;
        let mut storage = builder.storage.borrow_mut();
        // A quantified body is false if no assignment to its locals or free variables can
        // satisfy it.
        if storage
            .node_support(self.node)
            .is_some_and(Support::is_complete)
        {
            match self.node.is_unsatisfiable(
                db,
                env,
                &mut storage,
                inferable,
                self.source_order,
                Polarity::Positive,
                SolutionLimits::bounded(SolutionBudget::default(), budget),
            ) {
                Ok(true) => return (ConstraintSet::never(builder), inferable),
                Err(ProjectionError::ConstructionBudgetExceeded) => {
                    return (ConstraintSet::BudgetExceeded, inferable);
                }
                // An inconclusive satisfiability check still permits projection.
                _ => {}
            }
        }
        let Ok(projection) = self.node.exists(
            db,
            env,
            &mut storage,
            inferable,
            to_remove,
            self.source_order,
            budget,
        ) else {
            return (ConstraintSet::BudgetExceeded, inferable);
        };
        // The eliminated typevars must also leave the source-order history. Otherwise recursive
        // relations can re-import each other's quantified constraints after their live graphs have
        // stabilized. Keep the original order of the remaining entries and append derived facts.
        let source_order = storage
            .calculate_source_orders(self.source_order)
            .into_iter()
            .fold(None, |source_order, constraint| {
                if storage.constraint_mentions_typevars(db, constraint.into_inner(), to_remove) {
                    return source_order;
                }
                let constraint_source_order = storage.atomic_constraint_source_order(constraint);
                storage.ordered_source_order(source_order, Some(constraint_source_order))
            });
        let source_order =
            storage.ordered_source_order(source_order, projection.derived_source_order);
        (
            ConstraintSet::from_node(builder, projection.node, source_order),
            projection.scope,
        )
    }
}

impl<'db, 'c> ConstraintSet<'db, 'c> {
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
        build: Construction<'_, 'c, 'db>,
        to_remove: TypeVarSet<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Self {
        self.verify_builder(build.builder);
        if to_remove == TypeVarSet::None {
            return self;
        }

        // Universal and existential quantification are duals. Reusing existential abstraction
        // also keeps this operation on its cached, single-pass implementation.
        self.negate(db, build)
            .reduce_inferable(db, env, build, to_remove, inferable)
            .negate(db, build)
    }

    pub(crate) fn display(
        self,
        db: &'db dyn Db,
        env: &'c ProgramEnvironment<'db>,
    ) -> impl Display + 'c {
        std::fmt::from_fn(move |f| {
            let Self::Valid(diagram) = self else {
                return f.write_str("BudgetExceeded");
            };
            let storage = diagram.builder.storage.borrow();
            diagram.node.display(db, env, &storage).fmt(f)
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
            let Self::Valid(diagram) = self else {
                return f.write_str("BudgetExceeded");
            };
            let storage = diagram.builder.storage.borrow();
            diagram.node.display_graph(db, env, &storage, prefix).fmt(f)
        })
    }
}

impl Debug for ConstraintSet<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Valid(diagram) = self else {
            return f.write_str("BudgetExceeded");
        };
        f.debug_struct("ConstraintSet")
            .field("node", &diagram.node)
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
    // Terminal constraint sets need none of the decision diagram's arenas or caches.
    storage: LazyCell<Box<RefCell<ConstraintSetStorage<'db>>>>,
}

type ExistsCacheKey<'db> = (
    NodeId,
    TypeVarSet<'db>,
    TypeVarSet<'db>,
    Option<SourceOrderId>,
);

#[derive(Clone, Copy, Debug)]
struct Projection<'db> {
    node: NodeId,
    derived_source_order: Option<SourceOrderId>,
    /// Includes binders from intermediate captures that may still occur in `node`.
    scope: TypeVarSet<'db>,
}

#[derive(Debug, Default)]
struct ConstraintSetStorage<'db> {
    /// Compacted owned storage overlaid onto this builder. This is used by
    /// [`OwnedConstraintSet::query`] to create a [`ConstraintSetBuilder`] that is initially a
    /// read-only view of the owned constraint set's storage.
    ///
    /// IDs below the overlay split points are looked up in this storage; newly interned entries
    /// are stored in the dense local arenas below.
    compacted: Option<Arc<OwnedConstraintSetInner<'db>>>,

    /// Raw paths from the last bounded projection in this builder, before solution selection.
    last_projection: Option<Box<(projection::ProjectionKey<'db>, CandidateSolutions<'db>)>>,

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
    nodes: IndexVec<NodeId, BuilderNode>,

    supports: IndexVec<SupportId, Support>,
    constraint_supports: IndexVec<ConstraintId, SupportId>,

    /// The fewest atoms that must be true to satisfy each node, computed lazily for coverage
    /// checks in builders without compacted storage.
    min_positive_atoms: IndexVec<NodeId, u8>,

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
    /// Pairs whose sequent maps are empty need no work on subsequent walks in the same
    /// inference scope. A different scope may allow the pair to imply new constraints.
    empty_sequent_pairs: FxHashSet<(
        Program<'db>,
        TypeVarSet<'db>,
        AtomicConstraintId,
        AtomicConstraintId,
    )>,
    /// Avoid repeatedly walking deep constraint bounds without imposing Salsa-query overhead on
    /// the many shallow bounds that are cheap to walk once.
    constraint_bound_depth_cache: FxHashMap<AtomicConstraintId, (u16, u16)>,
    source_order_cache: FxHashMap<SourceOrder, SourceOrderId>,
    /// Only caches completed top-level results. Recursive results depend on active path
    /// assignments and must not use this cache. A BDD's satisfiability does not depend on the
    /// source order used to traverse it.
    never_satisfied_cache: FxHashMap<(NodeId, TypeVarSet<'db>), bool>,

    negate_cache: FxHashMap<NodeId, NodeId>,
    or_cache: FxHashMap<(NodeId, NodeId), NodeId>,
    and_cache: FxHashMap<(NodeId, NodeId), NodeId>,
    /// Existential abstraction derives new constraints in source order and returns their
    /// source-order sidecar, so distinct orderings of the same BDD must not share a cache entry.
    exists_cache: FxHashMap<ExistsCacheKey<'db>, Projection<'db>>,
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
                .zip(compacted.constraints.iter().cloned())
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

    #[cfg(test)]
    pub(super) fn allocated_nodes(&self) -> usize {
        self.storage.borrow().nodes.len()
    }

    pub(super) fn with_budget<'a, 'c>(
        &'c self,
        budget: &'a ConstructionBudget,
    ) -> Construction<'a, 'c, 'db> {
        Construction {
            builder: self,
            budget,
        }
    }

    pub(crate) fn unlimited(&self) -> Construction<'static, '_, 'db> {
        self.with_budget(&ConstructionBudget::Unlimited)
    }

    pub(super) fn try_into_owned(
        self,
        f: impl for<'c> FnOnce(Construction<'c, 'c, 'db>) -> ConstraintSet<'db, 'c>,
    ) -> Option<OwnedConstraintSet<'db>> {
        let budget = ConstructionBudget::default();
        let diagram = f(self.with_budget(&budget)).diagram().ok()?;
        let (node, order) = (diagram.node, diagram.source_order);
        Some(self.finish(node, order))
    }

    /// Creates an [`OwnedConstraintSet`], consuming this builder in the process. You provide a
    /// callback that constructs a [`ConstraintSet`]. We then package that constraint set up with
    /// the storage arenas from this builder.
    pub(crate) fn into_owned(
        self,
        f: impl for<'c> FnOnce(Construction<'c, 'c, 'db>) -> ConstraintSet<'db, 'c>,
    ) -> OwnedConstraintSet<'db> {
        let diagram = f(self.unlimited())
            .diagram()
            .expect("unlimited construction");
        let (node, order) = (diagram.node, diagram.source_order);
        self.finish(node, order)
    }

    fn finish(self, node: NodeId, source_order: Option<SourceOrderId>) -> OwnedConstraintSet<'db> {
        // NOTE: We do not store any of the builder's memoization caches in the result. Owned
        // constraint sets can only be used by adding them to a new builder. Operation caches from
        // the original builder aren't relevant to the new builder, and don't need to be retained.
        let root = match node.node() {
            Node::Interior(interior) => interior,
            Node::AlwaysTrue | Node::AlwaysFalse => {
                return OwnedConstraintSet {
                    node,
                    source_order: None,
                    inner: None,
                };
            }
        };
        let source_order = source_order.expect("non-terminal BDD should have source_order");
        let storage = self.storage.take();
        OwnedConstraintSetBuilder::build(storage, root, source_order)
    }
}

/// Limits uncached AND/OR pairs and node visits when projecting out local type variables.
/// Separately cached solver relations are constructed independently. Importing them uses
/// the caller's allowance.
pub(crate) enum ConstructionBudget {
    Unlimited,
    Limited(Cell<usize>),
}

impl Default for ConstructionBudget {
    fn default() -> Self {
        Self::Limited(Cell::new(SolutionBudget::default().visits))
    }
}

impl ConstructionBudget {
    fn visit(&self) -> Result<(), ProjectionError> {
        if let Self::Limited(remaining) = self {
            remaining.set(
                remaining
                    .get()
                    .checked_sub(1)
                    .ok_or(ProjectionError::ConstructionBudgetExceeded)?,
            );
        }
        Ok(())
    }
}

/// The storage outlives each construction attempt. Only the attempt borrows its budget.
#[derive(Clone, Copy)]
pub(crate) struct Construction<'a, 'c, 'db> {
    pub(super) builder: &'c ConstraintSetBuilder<'db>,
    pub(super) budget: &'a ConstructionBudget,
}

impl<'c, 'db> Construction<'_, 'c, 'db> {
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
    pub(crate) fn load(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: &OwnedConstraintSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if other.node.is_terminal() {
            return ConstraintSet::from_node(self.builder, other.node, None);
        }
        let mut storage = self.builder.storage.borrow_mut();
        let result = storage.load(db, env, other, self.budget);
        ConstraintSet::from_result(self.builder, result)
    }

    /// Loads an owned constraint set, replacing the provenance of every constraint.
    pub(crate) fn load_with_provenance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: &OwnedConstraintSet<'db>,
        provenance: ConstraintProvenance,
    ) -> ConstraintSet<'db, 'c> {
        if other.node.is_terminal() {
            return ConstraintSet::from_node(self.builder, other.node, None);
        }
        let mut storage = self.builder.storage.borrow_mut();
        let result = storage.load_with_provenance(db, env, other, Some(provenance), self.budget);
        ConstraintSet::from_result(self.builder, result)
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

    /// Interns the free type variables in a constraint bound in a stable order.
    fn intern_mentioned_typevars_in_type(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        support: &mut Support,
    ) {
        let (typevars, skipped_lazy) = ty.free_typevars(db, env);
        *support |= &Support::from_typevar_set(db, self, typevars);
        if skipped_lazy {
            support.mark_incomplete();
        }
    }

    /// Interns all of the typevars mentioned in a constraint in a stable order.
    fn intern_constraint_typevars(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraint: &Constraint<'db>,
    ) -> Support {
        let mut support = Support::default();
        match constraint {
            Constraint::Atomic(AtomicConstraint::ConcreteLower(constraint)) => {
                support.insert(self.intern_typevar(db, constraint.typevar));
                self.intern_mentioned_typevars_in_type(db, env, constraint.bound, &mut support);
            }
            Constraint::Atomic(AtomicConstraint::ConcreteUpper(constraint)) => {
                support.insert(self.intern_typevar(db, constraint.typevar));
                self.intern_mentioned_typevars_in_type(db, env, constraint.bound, &mut support);
            }
            Constraint::Atomic(AtomicConstraint::ConcreteEquivalence(constraint)) => {
                support.insert(self.intern_typevar(db, constraint.typevar));
                self.intern_mentioned_typevars_in_type(db, env, constraint.bound, &mut support);
            }
            Constraint::Atomic(AtomicConstraint::TypeVarRange(constraint)) => {
                support.insert(self.intern_typevar(db, constraint.left));
                support.insert(self.intern_typevar(db, constraint.right));
            }
            Constraint::Atomic(AtomicConstraint::TypeVarEquivalence(constraint)) => {
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
        self.ensure_overlay_identity_caches();
        if let Some(id) = self.constraint_cache.get(&data) {
            return *id;
        }
        let support = self.intern_constraint_typevars(db, env, &data);
        let support_id = self.intern_support(support);
        let id = self.constraints.push(data.clone());
        self.constraint_supports.push(support_id);
        let id = self.adjusted_constraint_id(id);
        self.constraint_cache.insert(data, id);
        id
    }

    fn intern_atomic_constraint(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        data: AtomicConstraint<'db>,
    ) -> AtomicConstraintId {
        self.intern_constraint(db, env, data.into())
            .expect_atomic(self)
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

        let id = self.nodes.push(BuilderNode { data, support });
        let id = self.adjusted_node_id(id);
        self.node_cache.insert(data, id);
        id
    }

    /// Tries to prove that `left` needs fewer true atoms than `right`.
    ///
    /// On any accepting path, setting just the atoms on guarded true edges to true satisfies
    /// the diagram. Ordering ensures that no atom appears twice on the path. If the minimum
    /// for `left` is smaller, such an assignment witnesses that `right` cannot cover `left`.
    fn requires_fewer_positive_atoms(&mut self, left: NodeId, right: NodeId) -> bool {
        if self.compacted.is_some() || left.is_terminal() || right.is_terminal() {
            return false;
        }

        // Children precede parents in the local arena. Extend the table up to the nodes we
        // need, bounding the work per comparison. Saturation preserves the ordering whenever
        // the two counts differ after truncation.
        let minima = &mut self.min_positive_atoms;
        let end = left.index().max(right.index()) + 1;
        for node in self
            .nodes
            .iter()
            .take(end)
            .skip(minima.len())
            .take(COVERAGE_BUDGET)
        {
            let minimum = |branch| match branch {
                ALWAYS_TRUE => 0,
                ALWAYS_FALSE => u8::MAX,
                _ => minima[branch],
            };
            let node = node.data;
            let count = minimum(node.if_true)
                .saturating_add(1)
                .min(minimum(node.if_uncertain))
                .min(minimum(node.if_false));
            minima.push(count);
        }
        minima.len() >= end && minima[left] < minima[right]
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

    fn constraint_data(&self, constraint: ConstraintId) -> &Constraint<'db> {
        if let Some(compacted) = &self.compacted {
            let index = constraint.index();
            let split = compacted.constraint_indices.len();
            if index < split {
                let compacted_index = compacted.retained_constraint_index(constraint);
                return &compacted.constraints[compacted_index];
            }
            return &self.constraints[ConstraintId::from_usize(index - split)];
        }
        &self.constraints[constraint]
    }

    fn atomic_constraint_data(&self, constraint: AtomicConstraintId) -> AtomicConstraint<'db> {
        #[expect(irrefutable_let_patterns)]
        let Constraint::Atomic(atomic) = self.constraint_data(constraint.into_inner()) else {
            panic!("atomic constraint should be atomic");
        };
        *atomic
    }

    fn cached_constraint_bound_depth(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraint_id: AtomicConstraintId,
    ) -> (u16, u16) {
        if let Some(depth) = self.constraint_bound_depth_cache.get(&constraint_id) {
            return *depth;
        }

        let constraint = self.atomic_constraint_data(constraint_id);
        let depth = constraint.bound_depth(db, env);
        self.constraint_bound_depth_cache
            .insert(constraint_id, depth);
        depth
    }

    fn interior_node_data(&self, node: NodeId) -> InteriorNodeData {
        if let Some(compacted) = &self.compacted {
            let index = node.index();
            let split = compacted.node_indices.len();
            if index < split {
                let compacted_index = compacted.retained_node_index(node);
                return compacted.nodes[compacted_index];
            }
            return self.nodes[NodeId::from_usize(index - split)].data;
        }
        self.nodes[node].data
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
        let constraint_data = self.constraint_data(constraint);
        match constraint_data {
            Constraint::Atomic(_) => self.intern_source_order(SourceOrder::AtomicConstraint(
                AtomicConstraintId(constraint),
            )),
        }
    }

    fn atomic_constraint_source_order(&mut self, constraint: AtomicConstraintId) -> SourceOrderId {
        self.intern_source_order(SourceOrder::AtomicConstraint(constraint))
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
    ) -> FxIndexSet<AtomicConstraintId> {
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
                SourceOrder::AtomicConstraint(constraint) => {
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
            return Some(self.nodes[NodeId::from_usize(index - split)].support);
        }
        Some(self.nodes[node].support)
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
        budget: &ConstructionBudget,
    ) -> Result<(NodeId, Option<SourceOrderId>), ProjectionError> {
        self.load_with_provenance(db, env, other, None, budget)
    }

    fn load_with_provenance(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: &OwnedConstraintSet<'db>,
        provenance: Option<ConstraintProvenance>,
        budget: &ConstructionBudget,
    ) -> Result<(NodeId, Option<SourceOrderId>), ProjectionError> {
        fn rebuild_node<'db>(
            storage: &mut ConstraintSetStorage<'db>,
            inner: &OwnedConstraintSetInner<'db>,
            constraints: &[(NodeId, Option<SourceOrderId>)],
            cache: &mut FxHashMap<NodeId, NodeId>,
            old_node: NodeId,
            budget: &ConstructionBudget,
        ) -> Result<NodeId, ProjectionError> {
            if old_node.is_terminal() {
                return Ok(old_node);
            }
            if let Some(remapped) = cache.get(&old_node) {
                return Ok(*remapped);
            }

            let old_node_index = inner.retained_node_index(old_node);
            let old_interior = inner.nodes[old_node_index];
            let mut rebuild = |node| rebuild_node(storage, inner, constraints, cache, node, budget);
            let if_true = rebuild(old_interior.if_true)?;
            let if_uncertain = rebuild(old_interior.if_uncertain)?;
            let if_false = rebuild(old_interior.if_false)?;
            let old_constraint_index = inner.retained_constraint_index(old_interior.constraint);
            let (condition, _) = constraints[old_constraint_index];
            let remapped =
                condition.ite_uncertain(storage, if_true, if_uncertain, if_false, budget)?;

            cache.insert(old_node, remapped);
            Ok(remapped)
        }

        if other.node.is_terminal() {
            return Ok((other.node, None));
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
                let Constraint::Atomic(atomic) = old_constraint;
                let atomic =
                    provenance.map_or(*atomic, |provenance| atomic.with_provenance(provenance));
                Constraint::Atomic(atomic).new_node(db, env, self)
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
                SourceOrder::AtomicConstraint(old_constraint) => {
                    let old_constraint_index =
                        inner.retained_constraint_index(old_constraint.into_inner());
                    let (_, constraint_source_order) = constraints[old_constraint_index];
                    source_orders[i] = constraint_source_order;
                }
            }
        }

        // Maps NodeIds in the OwnedConstraintSet to the corresponding NodeIds in this builder.
        let mut cache = FxHashMap::default();
        let node = rebuild_node(self, inner, &constraints, &mut cache, other.node, budget)?;
        let old_source_order = other
            .source_order
            .expect("non-terminal constraint set should have a source_order");
        let source_order = source_orders[old_source_order.index()];
        Ok((node, source_order))
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

impl ConstraintId {
    fn as_atomic(self, storage: &ConstraintSetStorage<'_>) -> Option<AtomicConstraintId> {
        match storage.constraint_data(self) {
            Constraint::Atomic(_) => Some(AtomicConstraintId(self)),
            #[expect(unreachable_patterns)]
            _ => None,
        }
    }

    #[track_caller]
    fn expect_atomic(self, storage: &ConstraintSetStorage<'_>) -> AtomicConstraintId {
        match storage.constraint_data(self) {
            Constraint::Atomic(_) => AtomicConstraintId(self),
            #[expect(unreachable_patterns)]
            _ => panic!("constraint should be atomic"),
        }
    }
}

/// A [`ConstraintId`] that we have verified refers to an atomic constraint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
struct AtomicConstraintId(ConstraintId);

impl AtomicConstraintId {
    fn into_inner(self) -> ConstraintId {
        self.0
    }

    fn when_true(self) -> Assignment<Self> {
        Assignment::Positive(self)
    }

    fn when_false(self) -> Assignment<Self> {
        Assignment::Negative(self)
    }

    fn when_unconstrained(self) -> Assignment<Self> {
        Assignment::Unconstrained(self)
    }
}

impl Idx for AtomicConstraintId {
    fn new(value: usize) -> Self {
        AtomicConstraintId(ConstraintId::new(value))
    }

    fn index(self) -> usize {
        self.0.index()
    }
}

#[newtype_index]
#[derive(get_size2::GetSize)]
struct SourceOrderId;

/// The nodes of the DAG that defines source ordering for a constraint set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum SourceOrder {
    Ordered(SourceOrderId, SourceOrderId),
    AtomicConstraint(AtomicConstraintId),
}

/// A disjunction of lower-bound clauses accumulated for one typevar.
///
/// The same type can be both evidence and a validity restriction. Keep each clause's
/// provenance and deduplicate types only after selecting the clauses for a union.
#[derive(Default)]
struct LowerBound<'db> {
    clauses: SmallVec<[(ConstraintProvenance, Type<'db>); 1]>,
}

impl<'db> LowerBound<'db> {
    fn add_clause(&mut self, provenance: ConstraintProvenance, ty: Type<'db>) {
        // `Never` imposes no lower bound on a valid solution but can still be evidence.
        if provenance.is_evidence() || !ty.is_never() {
            self.clauses.push((provenance, ty));
        }
    }

    fn iter_matching(
        &self,
        include: impl Fn(ConstraintProvenance) -> bool,
    ) -> impl Iterator<Item = Type<'db>> {
        self.clauses
            .iter()
            .filter_map(move |&(provenance, ty)| include(provenance).then_some(ty))
    }

    fn iter_evidence_without_validity(&self) -> impl Iterator<Item = Type<'db>> {
        self.iter_matching(|provenance| {
            provenance.is_evidence() && !provenance.contains(ConstraintProvenance::VALIDITY)
        })
    }

    /// Returns `None` when no clause matches, preserving explicit evidence of `Never`.
    fn union_matching(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        include: impl Fn(ConstraintProvenance) -> bool,
    ) -> Option<Type<'db>> {
        let mut bounds = self.iter_matching(include);
        if self.clauses.len() <= 1 {
            return bounds.next();
        }
        let bounds: FxIndexSet<_> = bounds.collect();
        (!bounds.is_empty()).then(|| UnionType::from_elements(db, env, bounds))
    }

    fn evidence(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: EvidenceSource,
    ) -> Option<Type<'db>> {
        // Prefer the declared type as written by the caller. Exclude declarations
        // derived using validity constraints when forming the declared candidate.
        self.union_matching(db, env, |provenance| {
            source.includes(provenance)
                && !(matches!(source, EvidenceSource::Declared)
                    && provenance.contains(ConstraintProvenance::VALIDITY))
        })
    }

    fn validity(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.union_matching(db, env, |provenance| !provenance.is_evidence())
            .unwrap_or(Type::Never)
    }
}

/// A factored conjunction of upper-bound clauses accumulated for one typevar.
///
/// Each clause is stored along with its provenance. Clauses may be unions, keeping bounds such as
/// `(A | B) & (C | D)` factored rather than distributing them into the DNF representation used by
/// [`Type`].
///
/// The absence of validity bounds represents an unconstrained validity upper bound of `object`.
/// This avoids allocating or checking the intersection identity on every path. An explicit
/// evidence bound of `object` remains meaningful.
///
/// Redundant clauses are retained to preserve evidence even when a validity restriction is
/// stronger. Consumers that require one effective bound can recover it with
/// [`UpperBound::as_single_bound`] without eagerly expanding intersections of unions.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
struct UpperBound<'db> {
    clauses: FxOrderSet<(ConstraintProvenance, Type<'db>)>,
}

impl<'db> UpperBound<'db> {
    #[cfg(test)]
    fn unconstrained() -> Self {
        Self::default()
    }

    /// Creates an upper bound from one explicit evidence clause.
    fn from_clause(clause: Type<'db>) -> Self {
        let mut upper = Self::default();
        upper.add_clause(ConstraintProvenance::INFERRED, clause);
        upper
    }

    fn iter_matching(
        &self,
        include: impl Fn(ConstraintProvenance) -> bool + Clone,
    ) -> impl Iterator<Item = Type<'db>> + Clone {
        self.clauses
            .iter()
            .filter_map(move |&(provenance, ty)| include(provenance).then_some(ty))
    }

    fn iter_clauses(&self, source: EvidenceSource) -> impl Iterator<Item = Type<'db>> + Clone + '_ {
        self.iter_evidence(source).chain(self.iter_validity())
    }

    fn iter_evidence(
        &self,
        source: EvidenceSource,
    ) -> impl Iterator<Item = Type<'db>> + Clone + '_ {
        self.iter_matching(move |provenance| source.includes(provenance))
    }

    fn iter_evidence_without_validity(
        &self,
        source: EvidenceSource,
    ) -> impl Iterator<Item = Type<'db>> + Clone + '_ {
        self.iter_matching(move |provenance| {
            source.includes(provenance) && !provenance.contains(ConstraintProvenance::VALIDITY)
        })
    }

    fn iter_validity(&self) -> impl Iterator<Item = Type<'db>> + Clone + '_ {
        self.iter_matching(|provenance| provenance == ConstraintProvenance::VALIDITY)
    }

    fn has_evidence_without_validity(&self, source: EvidenceSource) -> bool {
        self.iter_evidence_without_validity(source).next().is_some()
    }

    fn has_inference(&self, source: EvidenceSource) -> bool {
        self.iter_evidence(source).next().is_some()
    }

    /// Returns an existing upper-bound clause if every other clause is redundant with it.
    ///
    /// This preserves constrained type variables without distributing unions: expanding
    /// `S & (int | str)` into `(S & int) | (S & str)` would otherwise lose `S` as the single
    /// effective bound. Returns `None` instead of materializing intersections when no existing
    /// clause dominates the others. An unconstrained validity bound remains distinct from an
    /// explicit evidence bound of `object`.
    fn as_single_bound(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        let mut clauses = self.iter_clauses(EvidenceSource::All).peekable();
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
        if provenance == ConstraintProvenance::VALIDITY && ty.is_object() {
            return;
        }

        if self.clauses.contains(&(provenance, Type::Never)) {
            return;
        }

        // Unlike `Never`, bottom-materialized divergent markers must not absorb other clauses.
        if ty == Type::Never {
            self.clauses.retain(|&(existing, _)| existing != provenance);
        }

        self.clauses.insert((provenance, ty));
    }

    fn shrink_to_fit(&mut self) {
        // Canonicalize the order of the provenance set before memoizing.
        self.clauses
            .sort_by_key(|&(provenance, _)| provenance.bits());
        self.clauses.shrink_to_fit();
    }

    /// Try one common specialization for the unbounded variables in the bound comparison.
    ///
    /// For example, `A | T | U <= A & T & U` has a solution with `T = U = A`. Asking the
    /// solver to relate each variable to all the others can recursively recreate the same
    /// compatibility problem.
    fn has_common_specialization(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inferable: TypeVarSet<'db>,
        source: EvidenceSource,
        lower: UnionType<'db>,
        construction: &ConstructionBudget,
    ) -> Result<bool, ProjectionError> {
        let is_unbounded_variable = |variable: BoundTypeVarInstance<'db>| {
            variable.is_inferable(db, inferable)
                && !variable.is_paramspec(db)
                && !variable.is_typevartuple(db)
                && variable.typevar(db).bound_or_constraints(db, env).is_none()
        };
        let mut saw_variable = false;
        let mut fixed_members = SmallVec::<[Type<'db>; 2]>::new();
        for &element in lower.elements(db) {
            if element.as_typevar().is_some_and(is_unbounded_variable) {
                saw_variable = true;
            } else {
                fixed_members.push(element);
            }
        }
        if !saw_variable
            || fixed_members.iter().any(|&element| {
                may_contain_typevar_matching(db, env, element, is_unbounded_variable)
            })
        {
            return Ok(false);
        }

        // Assigning the union of the fixed members to each direct variable makes the entire
        // union equal to that witness.
        let witness = UnionType::from_elements(db, env, fixed_members);
        let accepts_witness = |mut clause: Type<'db>| {
            if clause.as_typevar().is_some_and(is_unbounded_variable) {
                return Ok(true);
            }

            // Try the same witness inside upper clauses. For example, when
            // R implements SupportsMul[R, R], setting T = U = R proves
            // R | T ≤ SupportsMul[T, U]. Solving this relation recursively can
            // keep nesting SupportsMul in its own result type.
            let (variables, _) = clause.free_typevars(db, env);
            if let Some(context) = GenericContext::try_from_typevar_instances(
                db,
                env,
                variables.iter(db).filter(|&var| is_unbounded_variable(var)),
            ) {
                clause =
                    clause.apply_specialization(db, context.repeat_specialization(db, witness));
            }
            witness
                .when_constraint_set_assignable_to_owned(db, env, clause, TypeVarSet::None)
                .query(|_, when| {
                    when.try_is_always_satisfied(db, env, TypeVarSet::None, construction)
                })
        };
        for clause in self.iter_clauses(source) {
            if let Type::Intersection(intersection) = clause
                && intersection.negative(db).is_empty()
            {
                for member in intersection.iter_positive(db) {
                    if !accepts_witness(member)? {
                        return Ok(false);
                    }
                }
            } else if !accepts_witness(clause)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Returns whether `lower` can satisfy all upper clauses together.
    #[expect(clippy::too_many_arguments)]
    fn permits_lower_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        source: EvidenceSource,
        lower: Type<'db>,
        construction: &ConstructionBudget,
    ) -> Result<bool, ProjectionError> {
        if self.iter_clauses(source).next().is_none() {
            return Ok(true);
        }

        // TODO: Detect recursive bound comparisons that keep growing, so termination
        // doesn't depend on finding a common specialization here.
        if inferable != TypeVarSet::None
            && let Type::Union(lower) = lower
            && self.has_common_specialization(db, env, inferable, source, lower, construction)?
        {
            return Ok(true);
        }

        // A lower bound such as `T & ~str` can fit `int` even when `T` itself includes
        // strings. Keep the exclusion when checking whether all the bounds can hold together.
        let when_assignable = |clause| {
            lower.when_constraint_set_assignable_to_owned_with_evaluation(
                db,
                env,
                clause,
                inferable,
                TypeVarEvaluation::LazyForSolver,
                ConstraintProvenance::INFERRED,
            )
        };
        let mut always = true;
        for clause in self.iter_clauses(source) {
            if !when_assignable(clause)
                .query(|_, when| when.try_is_always_satisfied(db, env, inferable, construction))?
            {
                always = false;
                break;
            }
        }
        if always {
            return Ok(true);
        }

        let mut node = ALWAYS_TRUE;
        let mut source_order = None;
        for clause in self.iter_clauses(source) {
            let when_clause = when_assignable(clause);
            let (clause_node, clause_source_order) =
                storage.load(db, env, &when_clause, construction)?;
            node = node.and(storage, clause_node, construction)?;
            source_order = storage.ordered_source_order(source_order, clause_source_order);
            if node == ALWAYS_FALSE {
                break;
            }
        }
        Ok(!node.is_unsatisfiable(
            db,
            env,
            storage,
            inferable,
            source_order,
            Polarity::Positive,
            SolutionLimits::unbounded(construction),
        )?)
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

                let non_atomic = match TypeKind::from(ty) {
                    TypeKind::Atomic => return,
                    // A non-generic nominal instance is an opaque leaf. Its class literal
                    // identifies the leaf but does not add nested type structure.
                    TypeKind::NonAtomic(NonAtomicType::NominalInstance(instance))
                        if !instance.class(db, self.env).is_generic() =>
                    {
                        return;
                    }
                    TypeKind::NonAtomic(non_atomic) => non_atomic,
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

impl ConstraintId {
    fn when_true(self) -> Assignment<Self> {
        Assignment::Positive(self)
    }

    fn when_false(self) -> Assignment<Self> {
        Assignment::Negative(self)
    }

    fn when_unconstrained(self) -> Assignment<Self> {
        Assignment::Unconstrained(self)
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

const COVERAGE_BUDGET: usize = 64;

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
        // Keep the proof bounded and avoid building new nodes. Speculative intersections can trigger
        // further coverage checks and expand a compact disjunction exponentially.
        // When the false guard is empty, skipping a failed proof for the true guard cannot
        // affect the second proof through their shared visit budget.
        if if_uncertain != ALWAYS_FALSE
            && !(if_false == ALWAYS_FALSE
                && storage.requires_fewer_positive_atoms(if_true, if_uncertain))
        {
            let mut remaining_visits = COVERAGE_BUDGET;
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
        constraint: Assignment<ConstraintId>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let constraint_id = constraint.constraint();
        let node = match constraint {
            Assignment::Positive(constraint) => {
                NodeId::with_uncertain(storage, constraint, ALWAYS_TRUE, ALWAYS_FALSE, ALWAYS_FALSE)
            }
            Assignment::Negative(constraint) => {
                NodeId::with_uncertain(storage, constraint, ALWAYS_FALSE, ALWAYS_FALSE, ALWAYS_TRUE)
            }
            // The result holds regardless of the constraint's truth value, so only
            // `if_uncertain` needs to be `ALWAYS_TRUE` — `n? 0: 1: 0`. It would also be
            // correct to use `n? 1: 1: 1` (i.e., `ALWAYS_TRUE` for all outgoing edges), but
            // that would throw away some of the efficiency gains this representation gives us.
            Assignment::Unconstrained(constraint) => {
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

    /// Look for a counterexample by falsifying each atom. When the false branch
    /// has no disjunct of its own, only the uncertain branch can still be true.
    /// Stop if following an edge would require combining branches.
    fn all_negative_path_to_false(
        self,
        storage: &ConstraintSetStorage<'_>,
    ) -> Option<SmallVec<[AtomicConstraintId; 4]>> {
        let mut atoms = SmallVec::new();
        let mut node = self;
        while let Node::Interior(_) = node.node() {
            let interior = storage.interior_node_data(node);
            if interior.if_false != ALWAYS_FALSE {
                return None;
            }
            atoms.push(interior.constraint.as_atomic(storage)?);
            node = interior.if_uncertain;
        }
        (node == ALWAYS_FALSE).then_some(atoms)
    }

    /// Budget exhaustion leaves satisfiability unknown and must not be cached as a result.
    #[expect(clippy::too_many_arguments)]
    fn is_unsatisfiable<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        polarity: Polarity,
        mut limits: SolutionLimits<'_>,
    ) -> Result<bool, ProjectionError> {
        /// Checks whether this BDD is a single conjunction, where either (a) every constraint is
        /// positive lower-bound-only, or (b) every constraint is a positive upper-bound-only. If
        /// so, `object` or `Never` respectively is a valid solution regardless of the contents of
        /// the constraints.
        fn simple_conjunction_is_satisfiable(
            storage: &mut ConstraintSetStorage<'_>,
            mut node: NodeId,
            limits: &mut SolutionLimits<'_>,
        ) -> Result<bool, ProjectionError> {
            let mut found_lower = false;
            let mut found_upper = false;
            loop {
                limits.visit_node().continue_ok()?;
                match node.node() {
                    Node::AlwaysTrue => return Ok(true),
                    Node::AlwaysFalse => return Ok(false),

                    Node::Interior(_) => {
                        let interior = storage.interior_node_data(node);

                        if interior.if_false != ALWAYS_FALSE
                            || interior.if_uncertain != ALWAYS_FALSE
                        {
                            // Not a single conjunction
                            return Ok(false);
                        }

                        let constraint = storage.constraint_data(interior.constraint);
                        #[expect(irrefutable_let_patterns)]
                        let Constraint::Atomic(constraint) = constraint else {
                            return Ok(false);
                        };
                        found_lower |= constraint.provides_lower();
                        found_upper |= constraint.provides_upper();
                        if found_lower && found_upper {
                            // Might be a single conjunction, but doesn't contain _only_
                            // lower-bound-only or upper-bound-only constraints
                            return Ok(false);
                        }

                        node = interior.if_true;
                    }
                }
            }
        }

        match self.node() {
            Node::AlwaysTrue => return Ok(polarity == Polarity::Negative),
            Node::AlwaysFalse => return Ok(polarity == Polarity::Positive),
            Node::Interior(_) => {}
        }

        let key = (self, inferable);
        if polarity == Polarity::Positive {
            if let Some(result) = storage.never_satisfied_cache.get(&key) {
                return Ok(*result);
            }
            if simple_conjunction_is_satisfiable(storage, self, &mut limits)? {
                storage.never_satisfied_cache.insert(key, false);
                return Ok(false);
            }
        }

        let orders = storage.calculate_source_orders(source_order);
        let (mut walker, mut path) =
            SolutionWalker::for_node(db, env, storage, orders, inferable, limits, self);
        let result = walker
            .is_never_satisfied(db, env, storage, &mut path, polarity)
            .continue_ok()?;
        if polarity == Polarity::Positive {
            storage.never_satisfied_cache.insert(key, result);
        }
        Ok(result)
    }

    /// Returns the negation of this BDD.
    fn negate(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        budget: &ConstructionBudget,
    ) -> Result<Self, ProjectionError> {
        match self.node() {
            Node::AlwaysTrue => Ok(ALWAYS_FALSE),
            Node::AlwaysFalse => Ok(ALWAYS_TRUE),
            Node::Interior(interior) => interior.negate(storage, budget),
        }
    }

    /// Returns the `or` or union of two BDDs.
    fn or(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        other: Self,
        budget: &ConstructionBudget,
    ) -> Result<Self, ProjectionError> {
        match (self.node(), other.node()) {
            (Node::AlwaysTrue, _) | (_, Node::AlwaysTrue) => Ok(ALWAYS_TRUE),
            (Node::AlwaysFalse, _) => Ok(other),
            (_, Node::AlwaysFalse) => Ok(self),
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.or(storage, other_interior, budget)
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
    fn tree_fold<'db, 'c>(
        build: Construction<'_, 'c, 'db>,
        nodes: impl Iterator<Item = ConstraintSet<'db, 'c>>,
        zero: Self,
        one: Self,
        mut combine: impl FnMut(
            Self,
            &mut ConstraintSetStorage<'_>,
            Self,
            &ConstructionBudget,
        ) -> Result<Self, ProjectionError>,
    ) -> Result<(Self, Option<SourceOrderId>), ProjectionError> {
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
        let Construction { builder, budget } = build;
        for set in nodes {
            set.verify_builder(builder);
            let ConstraintDiagram {
                node, source_order, ..
            } = set.diagram()?;
            if node == one {
                return Ok((node, source_order));
            }

            let (mut node, mut source_order, mut depth) = (node, source_order, 0);
            while accumulator
                .last()
                .is_some_and(|(_, _, existing)| *existing == depth)
            {
                let (existing_node, existing_source_order, _) =
                    accumulator.pop().expect("accumulator should not be empty");
                depth += 1;
                // Keep the tree shape, but avoid borrowing storage for an identity.
                if existing_node == zero && existing_source_order.is_none() {
                    continue;
                }
                let mut storage = builder.storage.borrow_mut();
                node = combine(existing_node, &mut storage, node, budget)?;
                source_order = storage.ordered_source_order(existing_source_order, source_order);
                if node == one {
                    return Ok((node, source_order));
                }
            }
            accumulator.push((node, source_order, depth));
        }

        // At this point, we've consumed all of the iterator. The length of the accumulator will be
        // the same as the number of 1 bits in the length of the iterator. We do a final fold to
        // produce the overall result.
        let mut storage = builder.storage.borrow_mut();
        accumulator.into_iter().try_fold(
            (zero, None),
            |(result_node, result_source_order), (node, source_order, _)| {
                Ok((
                    combine(result_node, &mut storage, node, budget)?,
                    storage.ordered_source_order(result_source_order, source_order),
                ))
            },
        )
    }

    /// Returns the `and` or intersection of two BDDs.
    fn and(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        other: Self,
        budget: &ConstructionBudget,
    ) -> Result<Self, ProjectionError> {
        if self == other {
            return Ok(self);
        }
        match (self.node(), other.node()) {
            (Node::AlwaysFalse, _) | (_, Node::AlwaysFalse) => Ok(ALWAYS_FALSE),
            (Node::AlwaysTrue, _) => Ok(other),
            (_, Node::AlwaysTrue) => Ok(self),
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.and(storage, other_interior, budget)
            }
        }
    }

    /// Returns a new BDD that evaluates to `true` when both input BDDs evaluate to the same
    /// result.
    fn iff(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        other: Self,
        budget: &ConstructionBudget,
    ) -> Result<Self, ProjectionError> {
        // iff(a, b) = (a ∧ b) ∨ (¬a ∧ ¬b)
        let a_and_b = self.and(storage, other, budget)?;
        let not_a = self.negate(storage, budget)?;
        let not_b = other.negate(storage, budget)?;
        let not_a_and_not_b = not_a.and(storage, not_b, budget)?;
        a_and_b.or(storage, not_a_and_not_b, budget)
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
        budget: &ConstructionBudget,
    ) -> Result<Self, ProjectionError> {
        if uncertain_node == ALWAYS_TRUE {
            return Ok(ALWAYS_TRUE);
        }

        match self.node() {
            Node::AlwaysTrue => then_node.or(storage, uncertain_node, budget),
            Node::AlwaysFalse => else_node.or(storage, uncertain_node, budget),
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
                    return Ok(NodeId::with_uncertain(
                        storage,
                        interior.constraint,
                        then_node,
                        uncertain_node,
                        else_node,
                    ));
                }

                // For compound conditions, or when the new builder's variable ordering requires
                // one of the branches to move above `self`, fall back to the semantic expansion:
                // `(self ∧ then_node) ∨ uncertain_node ∨ (¬self ∧ else_node)`.
                let if_true = self.and(storage, then_node, budget)?;
                let if_true_or_uncertain = if_true.or(storage, uncertain_node, budget)?;
                let negated = self.negate(storage, budget)?;
                let if_false = negated.and(storage, else_node, budget)?;
                if_true_or_uncertain.or(storage, if_false, budget)
            }
        }
    }

    /// Returns a new BDD that is the _existential abstraction_ of `self` for a set of typevars.
    /// The result will return true whenever `self` returns true for _any_ assignment of those
    /// typevars. The result will not contain any constraints that mention those typevars.
    #[expect(clippy::too_many_arguments)]
    fn exists<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        bound_typevars: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        construction: &ConstructionBudget,
    ) -> Result<Projection<'db>, ProjectionError> {
        let unchanged = Projection {
            node: self,
            derived_source_order: None,
            scope: inferable,
        };
        if bound_typevars == TypeVarSet::None {
            return Ok(unchanged);
        }

        let Node::Interior(interior) = self.node() else {
            return Ok(unchanged);
        };

        let key = (self, inferable, bound_typevars, source_order);
        if let Some(result) = storage.exists_cache.get(&key) {
            return Ok(*result);
        }

        let result = interior.exists_inner(
            db,
            env,
            storage,
            inferable,
            bound_typevars,
            source_order,
            construction,
        )?;

        storage.exists_cache.insert(key, result);
        Ok(result)
    }

    fn atomic_constraints(
        self,
        storage: &ConstraintSetStorage<'_>,
    ) -> SmallVec<[AtomicConstraintId; 8]> {
        let mut constraints = SmallVec::new();
        self.for_each_unique_constraint(storage, &mut |id| {
            if let Some(atomic) = id.as_atomic(storage) {
                constraints.push(atomic);
            }
        });
        constraints
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

    fn path_assignments<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_orders: &FxIndexSet<AtomicConstraintId>,
    ) -> PathAssignments {
        match self.node() {
            Node::AlwaysFalse | Node::AlwaysTrue => PathAssignments::default(),
            Node::Interior(interior) => interior.path_assignments(db, env, storage, source_orders),
        }
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

/// An interior node and its type variable support in a builder's local arena.
#[derive(Debug)]
struct BuilderNode {
    data: InteriorNodeData,
    support: SupportId,
}

/// Accumulates validity and evidence bounds for a single typevar on one TDD path.
///
/// Separate lower-bound unions preserve inference evidence even when a wider validity restriction
/// determines the effective minimum. Upper clauses retain their individual provenance and stay
/// factored to avoid distributing intersections over unions.
#[derive(Default)]
struct CandidateTypeVarSolver<'db> {
    lower: LowerBound<'db>,
    upper: UpperBound<'db>,
    has_declared_evidence: bool,
    has_declared_lower_evidence: bool,
}

impl<'db> CandidateTypeVarSolver<'db> {
    fn add_constraint(
        &mut self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
        constraint: AtomicConstraint<'db>,
    ) {
        match constraint {
            AtomicConstraint::ConcreteLower(lower) => {
                debug_assert!(bound_typevar.is_same_typevar_as(db, lower.typevar));
                self.add_lower(lower.provenance, lower.bound);
            }
            AtomicConstraint::ConcreteUpper(upper) => {
                debug_assert!(bound_typevar.is_same_typevar_as(db, upper.typevar));
                self.add_upper(upper.provenance, upper.bound);
            }
            AtomicConstraint::ConcreteEquivalence(equivalence) => {
                debug_assert!(bound_typevar.is_same_typevar_as(db, equivalence.typevar));
                self.add_lower(equivalence.provenance, equivalence.bound);
                self.add_upper(equivalence.provenance, equivalence.bound);
            }
            AtomicConstraint::TypeVarRange(bound) => {
                if bound_typevar.is_same_typevar_as(db, bound.left) {
                    self.add_upper(bound.provenance, Type::TypeVar(bound.right));
                } else if bound_typevar.is_same_typevar_as(db, bound.right) {
                    self.add_lower(bound.provenance, Type::TypeVar(bound.left));
                } else {
                    panic!("typevar should match one side or the other");
                }
            }
            AtomicConstraint::TypeVarEquivalence(bound) => {
                if bound_typevar.is_same_typevar_as(db, bound.left) {
                    self.add_lower(bound.provenance, Type::TypeVar(bound.right));
                    self.add_upper(bound.provenance, Type::TypeVar(bound.right));
                } else if bound_typevar.is_same_typevar_as(db, bound.right) {
                    self.add_lower(bound.provenance, Type::TypeVar(bound.left));
                    self.add_upper(bound.provenance, Type::TypeVar(bound.left));
                } else {
                    panic!("typevar should match one side or the other");
                }
            }
        }
    }

    fn add_lower(&mut self, provenance: ConstraintProvenance, ty: Type<'db>) {
        self.has_declared_evidence |= provenance.contains(ConstraintProvenance::DECLARED);
        self.has_declared_lower_evidence |= provenance.contains(ConstraintProvenance::DECLARED);
        self.lower.add_clause(provenance, ty);
    }

    fn add_upper(&mut self, provenance: ConstraintProvenance, ty: Type<'db>) {
        self.has_declared_evidence |= provenance.contains(ConstraintProvenance::DECLARED);
        self.upper.add_clause(provenance, ty);
    }

    /// Finalizes the lower and upper bounds that have been accumulated for this typevar. Returns
    /// `None` if the accumulated bounds are not compatible with each other (e.g. if the lower
    /// bound is not assignable to the upper bound).
    fn finish(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
        inferable: TypeVarSet<'db>,
        construction: &ConstructionBudget,
    ) -> Result<Option<CandidateTypeVarSolution<'db>>, ProjectionError> {
        let Self {
            lower,
            mut upper,
            has_declared_evidence,
            has_declared_lower_evidence,
        } = self;

        // Classify the original evidence bounds before aggregation, as gradual and static argument
        // evidence may collapse into a single gradual union.
        //
        // Note that we only compute this flag for constrained type variables.
        let has_only_non_concrete_evidence =
            bound_typevar.typevar(db).is_constrained(db).then(|| {
                let mut evidence = lower
                    .iter_evidence_without_validity()
                    .chain(upper.iter_evidence_without_validity(EvidenceSource::All))
                    .filter(|ty| !ty.has_unspecialized_type_var(db, env))
                    .peekable();

                evidence.peek().is_some()
                    && evidence.all(|ty| match ty {
                        // A fixed `U: (bool, int)` must promote to `int` for `T: (int, object)`.
                        // Check each variable before lower bounds combine into a union like `U | V`.
                        Type::TypeVar(peer) => {
                            peer.is_inferable(db, inferable)
                                || !peer.typevar(db).is_constrained(db)
                                || peer.can_specialize_typevar(db, env, bound_typevar)
                        }
                        _ => ty.bottom_materialization(db, env) != ty.top_materialization(db, env),
                    })
            });

        let evidence_lower = lower.evidence(db, env, EvidenceSource::All);
        let (inferred_lower, declared_lower) = if has_declared_lower_evidence {
            (
                lower.evidence(db, env, EvidenceSource::Inferred),
                lower.evidence(db, env, EvidenceSource::Declared),
            )
        } else {
            (evidence_lower, None)
        };
        let validity_lower = lower.validity(db, env);
        upper.shrink_to_fit();

        let range = CandidateTypeVarSolution {
            bound_typevar,
            // A projected candidate can be checked again after its path walk has ended.
            // Retain any binders introduced by captured generic callables for those checks.
            inferable,
            evidence_lower,
            inferred_lower,
            declared_lower,
            validity_lower,
            upper,
            has_declared_evidence,
            has_only_non_concrete_evidence,
            selected_declared_constraint: None,
        };
        let lower = range.effective_lower(db, env, EvidenceSource::All);
        // With no lower bound there is no lower endpoint to compare. In particular, the
        // ordinary `Never` used to represent missing evidence is outside the ParamSpec domain.
        Ok((lower.is_never()
            || range.upper.permits_lower_bound(
                db,
                env,
                storage,
                inferable,
                EvidenceSource::All,
                lower,
                construction,
            )?)
        .then_some(range))
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

/// The evidence used to choose a type variable's specialization.
///
/// A candidate solution for a given type variable may prefer bounds derived from inferred evidence,
/// declared evidence, or both. Note that validity constraints apply regardless of the preferred
/// evidence source.
#[derive(Clone, Copy)]
enum EvidenceSource {
    All,
    Inferred,
    Declared,
}

impl EvidenceSource {
    fn includes(self, provenance: ConstraintProvenance) -> bool {
        let evidence = provenance.difference(ConstraintProvenance::VALIDITY);

        match self {
            Self::All => evidence.is_evidence(),
            Self::Inferred => evidence == ConstraintProvenance::INFERRED,
            Self::Declared => evidence == ConstraintProvenance::DECLARED,
        }
    }
}

/// The range of possible solutions for one of the typevars in a [`CandidateSolution`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct CandidateTypeVarSolution<'db> {
    pub(crate) bound_typevar: BoundTypeVarInstance<'db>,
    inferable: TypeVarSet<'db>,
    evidence_lower: Option<Type<'db>>,
    inferred_lower: Option<Type<'db>>,
    declared_lower: Option<Type<'db>>,
    validity_lower: Type<'db>,
    upper: UpperBound<'db>,
    /// Whether any bound originates from declared evidence.
    has_declared_evidence: bool,
    /// Whether all evidence is gradual or from type variables eligible for a family solution.
    has_only_non_concrete_evidence: Option<bool>,
    /// The declared constraint selected for this range, if the typevar is constrained.
    selected_declared_constraint: Option<Type<'db>>,
}

impl<'db> CandidateTypeVarSolution<'db> {
    pub(crate) fn from_equivalence(
        bound_typevar: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Self {
        Self {
            bound_typevar,
            inferable,
            evidence_lower: Some(ty),
            inferred_lower: Some(ty),
            declared_lower: None,
            validity_lower: Type::Never,
            upper: UpperBound::from_clause(ty),
            has_declared_evidence: false,
            has_only_non_concrete_evidence: None,
            selected_declared_constraint: None,
        }
    }

    pub(crate) fn as_exact(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        let lower = self.inference_lower()?;
        if self.has_upper_inference()
            && let Some(upper) = self.as_single_upper_bound(db, env)
            && lower.is_equivalent_to(db, env, upper)
        {
            Some(self.selected_declared_constraint.unwrap_or(lower))
        } else {
            None
        }
    }

    /// Returns lower-bound inference evidence without supplying a default for a missing bound.
    pub(crate) fn inference_lower(&self) -> Option<Type<'db>> {
        self.evidence_lower
    }

    fn lower_bound(&self, source: EvidenceSource) -> Option<Type<'db>> {
        match source {
            EvidenceSource::All => self.evidence_lower,
            EvidenceSource::Inferred => self.inferred_lower,
            EvidenceSource::Declared => self.declared_lower,
        }
    }

    fn declared_candidate(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> PathBoundSolution<'db> {
        // Avoid restricting the range of a gradual declared type using validity bounds, preferring
        // the declared type as written by the caller.
        let candidate = match self.lower_bound(EvidenceSource::Declared) {
            Some(lower) => lower,

            None if self
                .upper
                .has_evidence_without_validity(EvidenceSource::Declared) =>
            {
                let upper_bounds = self
                    .upper
                    .iter_evidence_without_validity(EvidenceSource::Declared);

                let candidate =
                    UpperBound::single_bound_from_iterator(db, env, upper_bounds.clone())
                        .or_else(|| IntersectionType::bounded_from_elements(db, env, upper_bounds));

                match candidate {
                    Some(candidate) => candidate,
                    None => return PathBoundSolution::BudgetExceeded { fallback: None },
                }
            }

            None => return PathBoundSolution::Unsolved,
        };

        // Static candidates are promoted to the selected type variable constraint.
        if self.selected_declared_constraint.is_some() && candidate.is_fully_static(db, env) {
            return PathBoundSolution::Unsolved;
        }

        PathBoundSolution::Solved(candidate)
    }

    /// The assignability relations that place a candidate between its lower and upper bounds.
    fn candidate_relations(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        candidate: Type<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> impl Iterator<Item = (Type<'db>, Type<'db>)> {
        let specialize =
            move |bound: Type<'db>| bound.apply_optional_specialization(db, specialization);
        let lower = (!self.bound_typevar.is_paramspec(db))
            .then(|| self.effective_lower(db, env, EvidenceSource::All))
            .map(|bound| (specialize(bound), candidate));
        let uppers = self
            .upper
            .iter_clauses(EvidenceSource::All)
            .map(move |bound| (candidate, specialize(bound)));
        lower.into_iter().chain(uppers)
    }

    /// Checks whether a given candidate solution satisfies this path.
    pub(super) fn accepts_candidate(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
        candidate: Type<'db>,
    ) -> bool {
        self.try_accepts_candidate(db, env, builder.unlimited(), candidate)
            .expect("unlimited construction")
    }

    fn try_accepts_candidate(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: Construction<'_, '_, 'db>,
        candidate: Type<'db>,
    ) -> Result<bool, ProjectionError> {
        let inferable = self.inferable;
        Ok(!self
            .candidate_relations(db, env, candidate, None)
            .when_all(db, builder, |(lower, upper)| {
                lower.when_assignable_to(db, env, upper, builder, inferable.explicit_typevars())
            })
            .try_is_never_satisfied(db, env, inferable, builder.budget)?)
    }

    /// Returns one effective upper bound without expanding factored intersections.
    fn as_single_upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        self.upper.as_single_bound(db, env)
    }

    fn effective_lower(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: EvidenceSource,
    ) -> Type<'db> {
        let Some(lower) = self.lower_bound(source) else {
            return self.validity_lower;
        };
        if self.validity_lower.is_never() {
            lower
        } else {
            UnionType::from_elements(db, env, [lower, self.validity_lower])
        }
    }

    fn variance(&self) -> TypeVarVariance {
        match (self.has_lower_inference(), self.has_upper_inference()) {
            (false, true) => TypeVarVariance::Covariant,
            (true, false) => TypeVarVariance::Contravariant,
            (true, true) => TypeVarVariance::Invariant,
            (false, false) => TypeVarVariance::Bivariant,
        }
    }

    fn has_lower_inference(&self) -> bool {
        self.evidence_lower.is_some()
    }

    fn has_upper_inference(&self) -> bool {
        self.upper.has_inference(EvidenceSource::All)
    }

    /// Restricts the range of a gradual solution by the upper bounds inferred for this constraint.
    /// Returns `None` if constructing an intersection exceeds the solution budget.
    fn restrict_gradual_solution(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: EvidenceSource,
        solution: Type<'db>,
    ) -> Option<Type<'db>> {
        if self.selected_declared_constraint.is_some()
            || self.lower_bound(source).is_none()
            || !self.upper.has_evidence_without_validity(source)
            || solution.bottom_materialization(db, env) == solution.top_materialization(db, env)
        {
            return Some(solution);
        }

        // Unresolved type-variable relationships must not escape into the specialization.
        if solution.has_typevar(db, env) || solution.has_unspecialized_type_var(db, env) {
            return Some(solution);
        }

        let upper_bounds = self.upper.iter_evidence_without_validity(source);

        // `Divergent` is not safely reflexive, so we cannot intersect identical bounds.
        if UpperBound::single_bound_from_iterator(db, env, upper_bounds.clone()) == Some(solution) {
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

        let mut upper_bounds = upper_bounds.filter_map(materialize_upper);
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
    /// Collects assignability bounds for inference. If constructing the relation or collecting
    /// all its paths exceeds the budget, inference proceeds without these bounds.
    pub(crate) fn assignable_solutions_with_inferable(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> &'db CandidateSolutions<'db> {
        assignable_solutions_impl(
            db,
            env.program(db),
            self,
            target,
            inferable,
            UnspecializedTypeVarPolicy::Ignore,
        )
    }

    /// Like [`Self::assignable_solutions_with_inferable`], but preserves unspecialized type
    /// variables in the solution set.
    pub(crate) fn assignable_solutions_for_argument_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> &'db CandidateSolutions<'db> {
        assignable_solutions_impl(
            db,
            env.program(db),
            self,
            target,
            inferable,
            UnspecializedTypeVarPolicy::Retain,
        )
    }
}

#[salsa::tracked(
    returns(ref),
    cycle_initial=|_, _, _, _, _, _, _, _| CandidateSolutions::Unsatisfiable,
    heap_size=ruff_memory_usage::heap_size,
)]
fn assignable_solutions_impl<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    source: Type<'db>,
    target: Type<'db>,
    inferable: TypeVarSet<'db>,
    unspecialized_type_var_policy: UnspecializedTypeVarPolicy,
) -> CandidateSolutions<'db> {
    let env = &ProgramEnvironment::from_program(program);
    let Some(when) =
        source.inference_relation(db, env, target, inferable, ConstraintProvenance::INFERRED)
    else {
        return CandidateSolutions::Unconstrained;
    };
    when.query(|_, when| {
        CandidateSolutions::compute(
            db,
            env,
            when,
            inferable,
            SolutionLimits::bounded(SolutionBudget::default(), &ConstructionBudget::default()),
            unspecialized_type_var_policy,
            CandidateSearch::All(ReportDiagnostics::Yes),
        )
        .unwrap_or(CandidateSolutions::Unconstrained)
    })
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
    typevars: Box<[CandidateTypeVarSolution<'db>]>,
    validity: SolutionValidity<'db>,
}

impl<'db> CandidateSolution<'db> {
    /// Variables whose bounds fail with all sibling arguments fixed to this specialization.
    fn incompatible_typevars(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: Specialization<'db>,
    ) -> impl Iterator<Item = BoundTypeVarInstance<'db>> {
        self.typevars.iter().filter_map(move |bounds| {
            let variable = bounds.bound_typevar;
            let fits = arguments.get(db, variable).is_some_and(|ty| {
                bounds
                    .candidate_relations(db, env, ty, Some(arguments))
                    .all(|(lower, upper)| {
                        !lower.has_provisional_marker(db, env)
                            && !upper.has_provisional_marker(db, env)
                            && lower.is_assignable_to(db, env, upper)
                    })
            });
            (!fits).then_some(variable)
        })
    }
}

/// Whether or not to report diagnostics when the constraint solver encounters unsatisfied bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReportDiagnostics {
    Yes,
    No,
}

impl ReportDiagnostics {
    pub(super) const fn is_yes(self) -> bool {
        matches!(self, ReportDiagnostics::Yes)
    }
}

/// Nested satisfiability checks can have independent traversal limits but must share
/// the construction budget of the comparison that initiated them.
struct SolutionLimits<'a> {
    traversal: Option<BoundedSolutionLimits>,
    construction: &'a ConstructionBudget,
}

struct BoundedSolutionLimits {
    remaining_paths: usize,
    remaining_visits: usize,
}

impl<'a> SolutionLimits<'a> {
    /// Leaves traversal unrestricted while retaining the caller's construction budget.
    fn unbounded(construction: &'a ConstructionBudget) -> Self {
        Self {
            traversal: None,
            construction,
        }
    }

    fn bounded(budget: SolutionBudget, construction: &'a ConstructionBudget) -> Self {
        Self {
            traversal: Some(BoundedSolutionLimits {
                remaining_paths: budget.paths,
                remaining_visits: budget.visits,
            }),
            construction,
        }
    }

    fn visit_node(&mut self) -> ControlFlow<ProjectionError> {
        if let Some(limits) = &mut self.traversal {
            let Some(remaining) = limits.remaining_visits.checked_sub(1) else {
                return ControlFlow::Break(ProjectionError::TraversalBudgetExceeded);
            };
            limits.remaining_visits = remaining;
        }
        ControlFlow::Continue(())
    }

    fn satisfied_path(&mut self) -> ControlFlow<ProjectionError> {
        if let Some(limits) = &mut self.traversal {
            let Some(remaining) = limits.remaining_paths.checked_sub(1) else {
                return ControlFlow::Break(ProjectionError::PathBudgetExceeded);
            };
            limits.remaining_paths = remaining;
        }
        ControlFlow::Continue(())
    }
}

/// Propagate a failed construction through a visitor that may also break on success.
fn continue_if_ok<T, B: From<ProjectionError>>(
    result: Result<T, ProjectionError>,
) -> ControlFlow<B, T> {
    match result {
        Ok(value) => ControlFlow::Continue(value),
        Err(error) => ControlFlow::Break(error.into()),
    }
}

#[derive(Clone, Copy)]
enum CandidateSearch {
    /// Collect the complete family of candidate solutions.
    All(ReportDiagnostics),
    /// Stop after a terminal brings the number of valid candidates to this limit.
    FirstValid(usize),
}

impl<'db> CandidateSolutions<'db> {
    /// Collects the lower and upper bounds for each type variable on each path in source order.
    ///
    /// Visits include the concrete-conjunction fast path and both BDD walks. The path limit
    /// counts materialized constrained paths; an unconstrained or unsatisfiable result needs no
    /// path allowance. No partially collected family is returned when either limit is exhausted.
    fn compute(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        set: ConstraintSet<'db, '_>,
        inferable: TypeVarSet<'db>,
        mut limits: SolutionLimits<'_>,
        unspecialized_type_var_policy: UnspecializedTypeVarPolicy,
        search: CandidateSearch,
    ) -> Result<Self, ProjectionError> {
        let diagram = set.diagram()?;
        let storage = &mut *diagram.builder.storage.borrow_mut();
        let node = diagram.node;
        let source_order = diagram.source_order;
        let mut source_orders = None;
        if let Some(path_bounds) = Self::compute_simple_bound_conjunction(
            db,
            env,
            storage,
            source_order,
            &mut source_orders,
            node,
            inferable,
            &mut limits,
            unspecialized_type_var_policy,
            match search {
                CandidateSearch::All(report_diagnostics) => report_diagnostics,
                CandidateSearch::FirstValid(_) => ReportDiagnostics::Yes,
            },
        )
        .continue_ok()?
        {
            return Ok(path_bounds);
        }

        let source_orders =
            source_orders.unwrap_or_else(|| storage.calculate_source_orders(source_order));
        let (walker, mut path) =
            SolutionWalker::for_node(db, env, storage, source_orders, inferable, limits, node);
        let mut walker = walker
            .with_unspecialized_type_var_policy(unspecialized_type_var_policy)
            .with_search(search);
        let node_support = storage.node_support(node).cloned();
        walker
            .visit_node(
                db,
                env,
                storage,
                &mut path,
                node_support.as_ref(),
                Polarity::Positive,
                node,
            )
            .continue_ok()?;
        Ok(walker.finish())
    }

    /// Solves a constraint set when it represents a _simple conjunction_: it can be expressed as a
    /// single conjunction of constraints, and the lower/upper bounds of those constraints do not
    /// mention any other typevars. That lets us use a fast path that doesn't require constructing
    /// a [`PathAssignments`] or its sequent map.
    ///
    /// We do still have to consider the declared upper bound/constraints of the typevars in the
    /// constraint set. To be eligible for this fast path, the solution cannot mention any
    /// _constrained_ typevars, since those introduce a disjunction. And like the constraints in
    /// the BDD itself, the declared upper bound of each type cannot mention any other typevars.
    #[expect(clippy::too_many_arguments)]
    fn compute_simple_bound_conjunction(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_order: Option<SourceOrderId>,
        source_orders: &mut Option<FxIndexSet<AtomicConstraintId>>,
        node: NodeId,
        inferable: TypeVarSet<'db>,
        limits: &mut SolutionLimits<'_>,
        unspecialized_type_var_policy: UnspecializedTypeVarPolicy,
        report_diagnostics: ReportDiagnostics,
    ) -> ControlFlow<ProjectionError, Option<Self>> {
        let mentions_typevar =
            |bound| any_over_type_expanding_aliases(db, env, bound, Type::is_type_var);
        let bound_is_eligible = |constraint: AtomicConstraint<'db>, bound| {
            constraint.provides_candidate_bound(db, env, Some(bound), unspecialized_type_var_policy)
                && !mentions_typevar(bound)
        };

        let mut constraints = SmallVec::<[_; 1]>::new();
        let mut current = node;
        loop {
            limits.visit_node()?;
            match current.node() {
                Node::AlwaysTrue => {
                    if constraints.is_empty() {
                        return ControlFlow::Continue(Some(CandidateSolutions::Unconstrained));
                    }
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

                    let Some(constraint_id) = interior.constraint.as_atomic(storage) else {
                        return ControlFlow::Continue(None);
                    };
                    let constraint = storage.atomic_constraint_data(constraint_id);
                    let Some((typevar, bound)) = constraint.as_concrete() else {
                        return ControlFlow::Continue(None);
                    };

                    if !typevar.is_inferable(db, inferable) || !bound_is_eligible(constraint, bound)
                    {
                        return ControlFlow::Continue(None);
                    }

                    // No other bound can precede or follow a lone constraint.
                    let order = if constraints.is_empty() && interior.if_true == ALWAYS_TRUE {
                        0
                    } else {
                        source_orders
                            .get_or_insert_with(|| storage.calculate_source_orders(source_order))
                            .get_index_of(&constraint_id)
                            .expect("every TDD constraint should have a source order")
                    };
                    constraints.push((typevar, constraint, order));
                    current = interior.if_true;
                }
            }
        }

        let mut mappings: FxIndexMap<BoundTypeVarInstance<'db>, CandidateTypeVarSolver<'db>> =
            FxIndexMap::default();
        constraints.sort_by_key(|(_, _, order)| *order);
        for (typevar, constraint, _) in constraints {
            mappings
                .entry(typevar)
                .or_default()
                .add_constraint(db, typevar, constraint);
        }

        // Add the declared upper bounds of each typevar in this fast-path solution. This fast path
        // only engages for _simple conjunctions_. That means we should give up if we find any
        // _constrained_ typevars, since those introduce a disjunction into the solution.
        //
        // Unlike the more general `SolutionWalker`, here we only have to consider the typevars
        // that are actually mentioned in the solution we found, rather than incorporating the
        // validity constraints of _every_ typevar. For this fast path, we have already ensured
        // that the solution does not depend on any cross-typevar relationships, so the validity
        // bounds of unmentioned typevars cannot affect the solution of the typevars we have
        // evidence for.
        for (bound_typevar, bounds) in &mut mappings {
            let bound = match bound_typevar.typevar(db).bound_or_constraints(db, env) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound,
                // A constrained typevar introduces a disjunction over its declared constraints,
                // which means we can't engage this fast path.
                Some(TypeVarBoundOrConstraints::Constraints(_)) => {
                    return ControlFlow::Continue(None);
                }
                // An unconstrained typevar does not add any additional constraints on its
                // solutions, other than the evidence we already have in the BDD.
                None => continue,
            };

            if mentions_typevar(bound) {
                // If this declared upper bound mentions other typevars, we don't have a simple
                // conjunction that's eligible for this fast path.
                return ControlFlow::Continue(None);
            }

            bounds.add_upper(ConstraintProvenance::VALIDITY, bound);
        }

        limits.satisfied_path()?;
        let mut typevars = Vec::with_capacity(mappings.len());
        for (bound_typevar, bounds) in mappings {
            let Some(solution) = continue_if_ok(bounds.finish(
                db,
                env,
                storage,
                bound_typevar,
                inferable,
                limits.construction,
            ))?
            else {
                return ControlFlow::Continue(match report_diagnostics {
                    ReportDiagnostics::No => Some(Self::Unsatisfiable),
                    // If any of the typevars doesn't have a solution (either because the constraint
                    // set is overly restrictive, or because a candidate solution doesn't satisfy
                    // the declared upper bounds), fall back on the slow path to construct useful
                    // diagnostics.
                    ReportDiagnostics::Yes => None,
                });
            };
            typevars.push(solution);
        }

        let candidate = CandidateSolution {
            typevars: typevars.into_boxed_slice(),
            validity: SolutionValidity::Valid,
        };
        ControlFlow::Continue(Some(CandidateSolutions::Constrained(Box::new([candidate]))))
    }

    pub(crate) fn solve(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Solutions<'db> {
        self.solve_with(|_variance, path_bound| {
            CandidateSolutions::default_solve(db, env, builder, path_bound)
        })
    }

    pub(crate) fn solve_for_argument_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
    ) -> Solutions<'db> {
        self.solve_with(|_variance, path_bound| {
            match Self::preliminary_solve(db, env, builder, path_bound) {
                PathBoundSolution::Solved(ty) if ty.is_provisional_marker() => {
                    PathBoundSolution::Unsolved
                }
                solution => solution,
            }
        })
    }

    /// Widen the inferred arguments for the next pass only when one viable path permits
    /// all the chosen promotions. If one path requires `T` to stay narrow and another
    /// requires `U` to stay narrow, both can still permit widening a third variable.
    pub(super) fn promote_for_argument_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original: Specialization<'db>,
        widest: Specialization<'db>,
    ) -> Specialization<'db> {
        let paths = match self {
            Self::Unconstrained => return widest,
            Self::Unsatisfiable => return original,
            Self::Constrained(paths) => paths,
        };
        let context = original.generic_context(db);
        if original == widest
            || paths.len().saturating_mul(context.len(db)) > SolutionBudget::default().type_terms
        {
            return original;
        }

        let accepts = |path: &CandidateSolution<'db>, arguments| {
            path.incompatible_typevars(db, env, arguments)
                .next()
                .is_none()
        };
        let viable = paths
            .iter()
            .filter(|path| matches!(path.validity, SolutionValidity::Valid));
        let mut combined = Cow::Borrowed(original.types(db));
        let mut common = Cow::Borrowed(widest.types(db));
        'paths: for path in viable.clone() {
            // Start with all promotions so related variables (e.g. `T = U`) can widen together.
            let mut selected = Cow::Borrowed(widest.types(db));
            for variable in path.incompatible_typevars(db, env, widest) {
                let Some(index) = context.variable_index(db, variable.identity(db)) else {
                    continue 'paths;
                };
                selected.to_mut()[index] = original.types(db)[index];
            }
            let Cow::Owned(types) = selected else {
                return widest;
            };
            let selected = context.specialize(db, types);
            // Restoring one variable can invalidate a dependent variable's promotion.
            if selected == original || !accepts(path, selected) {
                continue;
            }
            for (index, &ty) in selected.types(db).iter().enumerate() {
                if ty == original.types(db)[index] {
                    common.to_mut()[index] = ty;
                } else {
                    combined.to_mut()[index] = ty;
                }
            }
        }

        // If the proposals conflict, try just the promotions they share. Dropping a promotion
        // can invalidate a dependent bound, so this combination also needs a complete path.
        for types in [combined, common] {
            let candidate = context.specialize(db, types);
            if candidate == original || viable.clone().any(|path| accepts(path, candidate)) {
                return candidate;
            }
        }
        original
    }

    /// Solves each path by applying a per-typevar solver function, collecting retained solutions.
    ///
    /// A genuinely unsolved variable does not invalidate a path. Budget exhaustion also retains
    /// the path's available bindings, but marks the resulting path family as incomplete.
    pub(crate) fn solve_with(
        &self,
        mut choose: impl FnMut(
            TypeVarVariance,
            &CandidateTypeVarSolution<'db>,
        ) -> PathBoundSolution<'db>,
    ) -> Solutions<'db> {
        let Ok(solutions) = self.try_solve_with(
            |variance, bounds| Ok(choose(variance, bounds)),
            |_| Ok::<(), Infallible>(()),
        );
        solutions
    }

    /// Checks each retained solution before collecting it or solving the next path.
    fn try_solve_with<E>(
        &self,
        mut choose: impl FnMut(
            TypeVarVariance,
            &CandidateTypeVarSolution<'db>,
        ) -> Result<PathBoundSolution<'db>, E>,
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
            let Some((solution, path_exceeded_budget)) = Self::solve_path_with(path, &mut choose)?
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
    fn solve_path_with<E>(
        candidate: &CandidateSolution<'db>,
        choose: &mut impl FnMut(
            TypeVarVariance,
            &CandidateTypeVarSolution<'db>,
        ) -> Result<PathBoundSolution<'db>, E>,
    ) -> Result<Option<(Solution<'db>, bool)>, E> {
        let mut solved_typevars = Vec::with_capacity(candidate.typevars.len());
        let mut exceeded_budget = false;
        for path_bound in &candidate.typevars {
            let ty = match choose(path_bound.variance(), path_bound)? {
                PathBoundSolution::Solved(ty) => Some(ty),
                PathBoundSolution::Unsolved => None,
                PathBoundSolution::Unsatisfiable => return Ok(None),
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
        let solution = Solution {
            solved_typevars,
            validity: candidate.validity.clone(),
        };
        Ok(Some((solution, exceeded_budget)))
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
        path_bound: &CandidateTypeVarSolution<'db>,
    ) -> PathBoundSolution<'db> {
        Self::solve_with_preference(db, env, builder, path_bound, true)
    }

    pub(crate) fn solve_with_preference(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
        path_bound: &CandidateTypeVarSolution<'db>,
        prefer_declared: bool,
    ) -> PathBoundSolution<'db> {
        Self::try_solve_with_preference(db, env, builder.unlimited(), path_bound, prefer_declared)
            .expect("unlimited construction")
    }

    fn try_solve_with_preference(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: Construction<'_, '_, 'db>,
        path_bound: &CandidateTypeVarSolution<'db>,
        prefer_declared: bool,
    ) -> Result<PathBoundSolution<'db>, ProjectionError> {
        let default_solution =
            Self::solve_from_evidence(db, env, builder, path_bound, EvidenceSource::All)?;
        let PathBoundSolution::Solved(restricted) = default_solution else {
            return Ok(default_solution);
        };

        // A single parameter list must satisfy all of its lower bounds. A union of captured
        // parameter lists represents neither that common signature nor alternative runtime calls.
        // TODO: Construct a common behavioral supertype where possible.
        if path_bound.bound_typevar.domain(db) == TypeVarDomain::ParameterSignature
            && restricted.is_union()
        {
            return Ok(PathBoundSolution::Unsatisfiable);
        }

        if !path_bound.has_declared_evidence {
            return Ok(default_solution);
        }

        let inferred = match Self::solve_from_evidence(
            db,
            env,
            builder,
            path_bound,
            EvidenceSource::Inferred,
        )? {
            PathBoundSolution::Solved(inferred) => Some(inferred),
            PathBoundSolution::Unsolved => None,
            PathBoundSolution::Unsatisfiable | PathBoundSolution::BudgetExceeded { .. } => {
                return Ok(default_solution);
            }
        }
        .filter(|candidate| !candidate.has_provisional_marker(db, env));

        let declared = match path_bound.declared_candidate(db, env) {
            PathBoundSolution::Solved(declared) => Some(declared),
            PathBoundSolution::Unsolved | PathBoundSolution::Unsatisfiable => None,
            PathBoundSolution::BudgetExceeded { .. } => return Ok(default_solution),
        }
        .filter(|candidate| !candidate.has_provisional_marker(db, env));

        // Prefer the declared candidate when it is mutually assignable with the inferred
        // candidate. Otherwise, prefer the inferred candidate.
        let candidates = match (inferred, declared) {
            (Some(inferred), Some(declared))
                if prefer_declared
                    && inferred.is_assignable_to(db, env, declared)
                    && declared.is_assignable_to(db, env, inferred) =>
            {
                [Some(declared), Some(inferred)]
            }
            (Some(inferred), _) => [Some(inferred), None],
            (None, declared) => [declared, None],
        };

        for candidate in candidates.into_iter().flatten() {
            if path_bound.try_accepts_candidate(db, env, builder, candidate)? {
                return Ok(PathBoundSolution::Solved(candidate));
            }
        }

        Ok(default_solution)
    }

    /// Selects a preliminary solution to use as type context during generic call inference.
    ///
    /// Unlike [`Self::default_solve`], the range of a gradual solution is not restricted by inferred
    /// upper bounds, as the inferred types may not have stabilized yet.
    pub(crate) fn preliminary_solve(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &ConstraintSetBuilder<'db>,
        path_bound: &CandidateTypeVarSolution<'db>,
    ) -> PathBoundSolution<'db> {
        Self::solve_bounds(
            db,
            env,
            builder.unlimited(),
            path_bound,
            EvidenceSource::All,
        )
        .expect("unlimited construction")
    }

    fn solve_bounds(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, '_, 'db>,
        path_bound: &CandidateTypeVarSolution<'db>,
        source: EvidenceSource,
    ) -> Result<PathBoundSolution<'db>, ProjectionError> {
        // If this BDD path matches a single declared constraint, that declared constraint is our
        // solution.
        if let Some(selected) = path_bound.selected_declared_constraint {
            return Ok(PathBoundSolution::Solved(selected));
        }

        // Prefer the lower bound (often the concrete actual type seen) over the
        // upper bound (which may include TypeVar bounds/constraints). The upper bound
        // should only be used as a fallback when no concrete type was inferred.
        if path_bound.lower_bound(source).is_some() {
            let lower = path_bound.effective_lower(db, env, source);
            let mut storage = build.builder.storage.borrow_mut();
            return if path_bound.upper.permits_lower_bound(
                db,
                env,
                &mut storage,
                path_bound.inferable,
                source,
                lower,
                build.budget,
            )? {
                Ok(PathBoundSolution::Solved(lower))
            } else {
                Ok(PathBoundSolution::Unsatisfiable)
            };
        }

        if path_bound.upper.has_inference(source) {
            // Evidence determines whether to infer a solution, while validity restricts
            // which evidence-compatible solution is permitted. Top-materialize validity
            // bounds so that their gradual elements do not become part of the result.
            let upper_bounds = path_bound.upper.iter_evidence(source).chain(
                path_bound
                    .upper
                    .iter_validity()
                    .map(|bound| bound.top_materialization(db, env)),
            );
            if let Some(upper) =
                UpperBound::single_bound_from_iterator(db, env, upper_bounds.clone())
            {
                return Ok(PathBoundSolution::Solved(upper));
            }
            return Ok(
                IntersectionType::bounded_from_elements(db, env, upper_bounds).map_or(
                    PathBoundSolution::BudgetExceeded { fallback: None },
                    PathBoundSolution::Solved,
                ),
            );
        }

        Ok(PathBoundSolution::Unsolved)
    }

    fn solve_from_evidence(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: Construction<'_, '_, 'db>,
        path_bound: &CandidateTypeVarSolution<'db>,
        source: EvidenceSource,
    ) -> Result<PathBoundSolution<'db>, ProjectionError> {
        let preliminary = Self::solve_bounds(db, env, builder, path_bound, source)?;
        let PathBoundSolution::Solved(solution) = preliminary else {
            return Ok(preliminary);
        };

        let Some(restricted) = path_bound.restrict_gradual_solution(db, env, source, solution)
        else {
            return Ok(PathBoundSolution::BudgetExceeded {
                fallback: Some(solution),
            });
        };

        // An empty gradual range makes the constraint path unsatisfiable.
        if restricted.is_never() && !solution.is_never() {
            return Ok(PathBoundSolution::Unsatisfiable);
        }

        Ok(PathBoundSolution::Solved(restricted))
    }
}

impl InteriorNode {
    fn node(self) -> NodeId {
        self.0
    }

    fn negate(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        budget: &ConstructionBudget,
    ) -> Result<NodeId, ProjectionError> {
        let key = self.node();
        if let Some(result) = storage.negate_cache.get(&key) {
            return Ok(*result);
        }

        // negate(n ? C : U : D) = n ? negate(or(C, U)) : 0 : negate(or(D, U))
        //
        // The uncertain branch U is absorbed into C and D via union before negation. The result's
        // uncertain branch is always zero. When U = 0 (the common case), this degenerates to the
        // standard binary BDD leaf-swap: n ? negate(C) : 0 : negate(D).
        let interior = storage.interior_node_data(self.node());
        let not_true = interior.if_true.negate(storage, budget)?;
        let not_uncertain = interior.if_uncertain.negate(storage, budget)?;
        let not_false = interior.if_false.negate(storage, budget)?;
        let if_true = not_true.and(storage, not_uncertain, budget)?;
        let if_false = not_false.and(storage, not_uncertain, budget)?;
        let result = NodeId::new(storage, interior.constraint, if_true, if_false);

        storage.negate_cache.insert(key, result);
        Ok(result)
    }

    fn or(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        other: Self,
        budget: &ConstructionBudget,
    ) -> Result<NodeId, ProjectionError> {
        let key = (self.node(), other.node());
        if let Some(result) = storage.or_cache.get(&key) {
            return Ok(*result);
        }
        budget.visit()?;

        let left = storage.interior_node_data(self.node());
        let right = storage.interior_node_data(other.node());
        let result = match left.constraint.ordering().cmp(&right.constraint.ordering()) {
            Ordering::Equal => {
                let if_true = left.if_true.or(storage, right.if_true, budget)?;
                let if_uncertain = left.if_uncertain.or(storage, right.if_uncertain, budget)?;
                let if_false = left.if_false.or(storage, right.if_false, budget)?;
                NodeId::with_uncertain(storage, left.constraint, if_true, if_uncertain, if_false)
            }
            // This is from Frisch's original description of TDDs. If self < other, we check self
            // first. Instead of distributing other into the if_true and if_false branches, we
            // "park" it in the if_uncertain branch. That causes us to only evaluate other "lazily"
            // when needed.
            Ordering::Less => {
                let if_uncertain = left.if_uncertain.or(storage, other.node(), budget)?;
                NodeId::with_uncertain(
                    storage,
                    left.constraint,
                    left.if_true,
                    if_uncertain,
                    left.if_false,
                )
            }
            // Ditto above but for the other variable ordering
            Ordering::Greater => {
                let if_uncertain = self.node().or(storage, right.if_uncertain, budget)?;
                NodeId::with_uncertain(
                    storage,
                    right.constraint,
                    right.if_true,
                    if_uncertain,
                    right.if_false,
                )
            }
        };

        storage.or_cache.insert(key, result);
        Ok(result)
    }

    fn and(
        self,
        storage: &mut ConstraintSetStorage<'_>,
        other: Self,
        budget: &ConstructionBudget,
    ) -> Result<NodeId, ProjectionError> {
        let key = (self.node(), other.node());
        if let Some(result) = storage.and_cache.get(&key) {
            return Ok(*result);
        }
        budget.visit()?;

        let left = storage.interior_node_data(self.node());
        let right = storage.interior_node_data(other.node());
        let result = match left.constraint.ordering().cmp(&right.constraint.ordering()) {
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
                let other_if_true = right.if_true.or(storage, right.if_uncertain, budget)?;
                let true_from_true = left.if_true.and(storage, other_if_true, budget)?;
                let true_from_uncertain = left.if_uncertain.and(storage, right.if_true, budget)?;
                let if_true = true_from_true.or(storage, true_from_uncertain, budget)?;
                let if_uncertain = left.if_uncertain.and(storage, right.if_uncertain, budget)?;
                let other_if_false = right.if_uncertain.or(storage, right.if_false, budget)?;
                let false_from_false = left.if_false.and(storage, other_if_false, budget)?;
                let false_from_uncertain =
                    left.if_uncertain.and(storage, right.if_false, budget)?;
                let if_false = false_from_false.or(storage, false_from_uncertain, budget)?;
                NodeId::with_uncertain(storage, left.constraint, if_true, if_uncertain, if_false)
            }
            Ordering::Less => {
                let if_true = left.if_true.and(storage, other.node(), budget)?;
                let if_uncertain = left.if_uncertain.and(storage, other.node(), budget)?;
                let if_false = left.if_false.and(storage, other.node(), budget)?;
                NodeId::with_uncertain(storage, left.constraint, if_true, if_uncertain, if_false)
            }
            Ordering::Greater => {
                let if_true = self.node().and(storage, right.if_true, budget)?;
                let if_uncertain = self.node().and(storage, right.if_uncertain, budget)?;
                let if_false = self.node().and(storage, right.if_false, budget)?;
                NodeId::with_uncertain(storage, right.constraint, if_true, if_uncertain, if_false)
            }
        };

        storage.and_cache.insert(key, result);
        Ok(result)
    }

    #[expect(clippy::too_many_arguments)]
    fn exists_inner<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        bound_typevars: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        construction: &ConstructionBudget,
    ) -> Result<Projection<'db>, ProjectionError> {
        self.abstract_inner(
            db,
            env,
            storage,
            inferable,
            source_order,
            construction,
            // Remove any node that constrains one of `bound_typevars`, or that has a lower/upper
            // bound that mentions one of them. Removed constraints are still added to `path`, so
            // the sequent map can propagate any derived constraints that do not mention the
            // quantified typevars.
            &mut |storage: &ConstraintSetStorage<'_>, constraint| {
                storage.constraint_mentions_typevars(db, constraint, bound_typevars)
            },
        )
        .continue_ok()
    }

    #[expect(clippy::too_many_arguments)]
    fn abstract_inner<'db, F>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        inferable: TypeVarSet<'db>,
        source_order: Option<SourceOrderId>,
        construction: &ConstructionBudget,
        should_remove: F,
    ) -> ControlFlow<ProjectionError, Projection<'db>>
    where
        F: FnMut(&ConstraintSetStorage<'_>, ConstraintId) -> bool,
    {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Disposition {
            Keep,
            Remove,
        }

        struct AbstractVisitor<'a, F> {
            should_remove: F,
            construction: &'a ConstructionBudget,
        }

        impl<F> PathVisitor for AbstractVisitor<'_, F>
        where
            F: FnMut(&ConstraintSetStorage<'_>, ConstraintId) -> bool,
        {
            type Result = (NodeId, Option<SourceOrderId>);
            type Interior = (Disposition, ConstraintId);
            type Break = ProjectionError;

            fn visit_node(&mut self) -> ControlFlow<Self::Break> {
                // A shared subtree can be visited on many paths without constructing new nodes.
                continue_if_ok(self.construction.visit())
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
                            if (self.should_remove)(storage, assignment.constraint().into_inner()) {
                                continue;
                            }
                            let (assignment, assignment_source_order) =
                                Node::new_satisfied_constraint(storage, assignment.into_inner());
                            result =
                                continue_if_ok(result.and(storage, assignment, self.construction))?;
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
                        let node = continue_if_ok(guard.ite_uncertain(
                            storage,
                            if_true,
                            if_uncertain,
                            if_false,
                            self.construction,
                        ))?;
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
                        let node =
                            continue_if_ok(if_true.or(storage, if_uncertain, self.construction))?;
                        let node = continue_if_ok(node.or(storage, if_false, self.construction))?;
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

        let source_orders = storage.calculate_source_orders(source_order);
        let mut path = self.path_assignments(db, env, storage, &source_orders);
        let mut visitor = AbstractVisitor {
            should_remove,
            construction,
        };
        let (node, derived_source_order) =
            path.visit(db, env, storage, inferable, self.node(), &mut visitor)?;
        let derived_source_order =
            path.projection_source_order(storage, source_order, derived_source_order);
        // Comparing lower and upper callable bounds can capture a generic signature in
        // a ParamSpec being projected out. Other derived facts can still refer to its
        // binders, so include these intermediate captures when checking what left scope.
        //
        // TODO: We shouldn't have to inspect the sequent map to figure out which
        // type variables are local to a callable comparison.
        //
        // For example, suppose we're trying to eliminate a ParamSpec `P`. While
        // comparing its bounds, we encounter a generic callback `[T](x: T) -> T`.
        // We can infer both that `P` captures `(x: T)` and that `T <= R`, where `R`
        // is the expected return type. When we eliminate `P`, we can lose the capture
        // but keep `T <= R`. The outer solver then sees a bound on `T` with no way
        // to tell that `T` belonged to the callback. We look through the sequents
        // here to find the lost capture so we know to eliminate `T` too.
        //
        // With scoped existentials [1], we'll keep the constraints from the callback
        // comparison under a BDD node that says “there exists a T such that ...”.
        // Quantifying over P will add another existential node. The capture and
        // `T <= R` will both still be there, under the node that binds T. At that
        // point, we won't need this search for lost captures.
        //
        // [1] https://github.com/astral-sh/ruff/pull/29027
        let scope = if derived_source_order.is_some() {
            let captured = inferable.captured_typevars(db, storage, || path.implied_constraints());
            inferable.merge(db, captured)
        } else {
            inferable
        };
        ControlFlow::Continue(Projection {
            node,
            derived_source_order,
            scope,
        })
    }

    fn path_assignments<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        source_orders: &FxIndexSet<AtomicConstraintId>,
    ) -> PathAssignments {
        // Distinct nodes can test the same constraint, and the source order can include
        // constraints that were simplified out of the diagram. Record each reachable
        // constraint once at its source position. `PathAssignments` needs this order
        // when constructing non-commutative sequent pairs.
        let mut constraints: SmallVec<[_; 8]> = SmallVec::from_elem(None, source_orders.len());
        self.node()
            .for_each_unique_constraint(storage, &mut |constraint| {
                if let Some(constraint) = constraint.as_atomic(storage) {
                    let index = source_orders
                        .get_index_of(&constraint)
                        .expect("every BDD constraint should have a source-order entry");
                    constraints[index] = Some(constraint);
                }
            });

        let bound_is_concrete = |bound: Type<'db>| {
            !bound.has_typevar(db, env)
                && !bound.has_unspecialized_type_var(db, env)
                && bound.bottom_materialization(db, env) == bound.top_materialization(db, env)
        };

        let mut independent_typevars = FxHashSet::default();
        let mut dependent_typevars = FxHashSet::default();
        for &constraint_id in constraints.iter().flatten() {
            let constraint = storage.atomic_constraint_data(constraint_id);
            if let Some((typevar, bound)) = constraint.as_concrete()
                && bound_is_concrete(bound)
            {
                let typevar = storage.typevar_id(db, typevar);
                independent_typevars.insert(typevar);
            } else {
                dependent_typevars.extend(
                    storage
                        .constraint_support(constraint_id.into_inner())
                        .iter(),
                );
            }
        }

        independent_typevars.retain(|typevar| !dependent_typevars.contains(typevar));

        PathAssignments::new(constraints.into_iter().flatten(), independent_typevars)
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

/// Inference evidence that independently excludes every declared constraint.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ConstraintFailureEvidence<'db> {
    Lower(Type<'db>),
    /// Upper bounds that must all hold on the same solution path.
    Upper(Box<[Type<'db>]>),
    /// Upper-bound evidence excluded every constraint, but its individual bounds aren't available.
    UpperUnknown,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum SolutionViolationKind<'db> {
    UpperBound(Option<Type<'db>>),
    Constraints {
        constraints: TypeVarConstraints<'db>,
        evidence: ConstraintFailureEvidence<'db>,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct SolutionViolation<'db> {
    pub(crate) bound_typevar: BoundTypeVarInstance<'db>,
    pub(crate) variance: TypeVarVariance,
    pub(crate) kind: SolutionViolationKind<'db>,
}

impl<'db> SolutionViolation<'db> {
    fn evidence_types(&self) -> &[Type<'db>] {
        match &self.kind {
            SolutionViolationKind::UpperBound(argument) => argument.as_slice(),
            SolutionViolationKind::Constraints { evidence, .. } => match evidence {
                ConstraintFailureEvidence::Lower(lower) => std::slice::from_ref(lower),
                ConstraintFailureEvidence::Upper(upper) => upper,
                ConstraintFailureEvidence::UpperUnknown => &[],
            },
        }
    }
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
pub(crate) enum Assignment<T> {
    Positive(T),
    Negative(T),
    Unconstrained(T),
}

impl<T> Assignment<T> {
    fn constraint(self) -> T {
        match self {
            Assignment::Positive(constraint) => constraint,
            Assignment::Negative(constraint) => constraint,
            Assignment::Unconstrained(constraint) => constraint,
        }
    }

    #[expect(clippy::wrong_self_convention)]
    fn as_constrained(self) -> Option<T> {
        match self {
            Assignment::Positive(constraint) | Assignment::Negative(constraint) => Some(constraint),
            Assignment::Unconstrained(_) => None,
        }
    }

    fn map<U>(self, f: impl FnOnce(T) -> U) -> Assignment<U> {
        match self {
            Assignment::Positive(constraint) => Assignment::Positive(f(constraint)),
            Assignment::Negative(constraint) => Assignment::Negative(f(constraint)),
            Assignment::Unconstrained(constraint) => Assignment::Unconstrained(f(constraint)),
        }
    }

    fn negated(self) -> Self {
        match self {
            Assignment::Positive(constraint) => Assignment::Negative(constraint),
            Assignment::Negative(constraint) => Assignment::Positive(constraint),
            // "This constraint can go either way" is symmetric under negation.
            Assignment::Unconstrained(constraint) => Assignment::Unconstrained(constraint),
        }
    }
}

impl Assignment<ConstraintId> {
    fn display<'db, 'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        storage: &'a ConstraintSetStorage<'db>,
    ) -> impl Display + 'a {
        let holds = match self {
            Assignment::Positive(_) => Some(true),
            Assignment::Negative(_) => Some(false),
            Assignment::Unconstrained(_) => None,
        };

        std::fmt::from_fn(move |f| {
            let constraint_data = storage.constraint_data(self.constraint());
            constraint_data.display(db, env, holds).fmt(f)
        })
    }
}

impl Assignment<AtomicConstraintId> {
    fn into_inner(self) -> Assignment<ConstraintId> {
        self.map(AtomicConstraintId::into_inner)
    }
}

/// A visitor for walking the paths of a BDD.
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
    fn visit_node(&mut self) -> ControlFlow<Self::Break>;

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

/// A single clause in the DNF representation of a BDD
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SatisfiedClause {
    constraints: Vec<Assignment<ConstraintId>>,
}

impl SatisfiedClause {
    fn push(&mut self, constraint: Assignment<ConstraintId>) {
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
    use itertools::Itertools;
    use pretty_assertions::assert_eq;

    use crate::db::tests::{TestDb, setup_db};
    use crate::place::global_symbol;
    use crate::types::generics::GenericContext;
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
        build: Construction<'_, 'c, 'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
        bound: KnownClass,
    ) -> ConstraintSet<'db, 'c> {
        let env = db.program_environment();
        let ty = bound.to_instance(db, &env);
        ConstraintSet::constrain_typevar_equivalence_bound(
            db,
            &env,
            build,
            ConstraintProvenance::INFERRED,
            TypeVarSet::All,
            bound_typevar,
            ty,
        )
    }

    #[test]
    fn lazy_negation_shares_the_construction_budget() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let [a, b, c, d, excluded] = ["A", "B", "C", "D", "E"]
            .map(|name| create_constraint(db, build, create_typevar(db, name), KnownClass::Int));
        let left = a.and(db, build, || b);
        let right = c.and(db, build, || d);
        // Exclude the all-negative shortcut. Walking the negation then has to
        // combine a conjunction's true edge with the uncertain edge.
        let set = left
            .or(db, build, || right)
            .or(db, build, || excluded.negate(db, build));
        let budget = ConstructionBudget::Limited(Cell::new(0));
        assert_eq!(
            set.try_is_always_satisfied(db, &env, TypeVarSet::All, &budget),
            Err(ProjectionError::ConstructionBudgetExceeded)
        );
        assert!(!set.is_always_satisfied(db, &env, TypeVarSet::All));
    }

    #[test]
    fn nested_combinations_share_an_allowance_and_preserve_exhaustion() {
        let db = setup_db();
        let db = &db;
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let [a, b, c] = ["A", "B", "C"]
            .map(|name| create_constraint(db, build, create_typevar(db, name), KnownClass::Int));
        let budget = ConstructionBudget::Limited(Cell::new(1));
        let build = builder.with_budget(&budget);
        let exhausted = a.and(db, build, || b.or(db, build, || c));
        assert_matches!(exhausted, ConstraintSet::BudgetExceeded);
        assert_matches!(
            exhausted.or(db, build, || unreachable!()),
            ConstraintSet::BudgetExceeded
        );
        assert_matches!(
            ConstraintSet::always(&builder).and(db, build, || exhausted),
            ConstraintSet::BudgetExceeded
        );
        let mut never = ConstraintSet::never(&builder);
        assert!(
            never
                .and(db, build, || unreachable!())
                .is_trivially_never_satisfied()
        );
        assert_matches!(
            never.intersect(db, build, exhausted),
            ConstraintSet::BudgetExceeded
        );
        // Completed subexpressions cost nothing even with no remaining visits.
        assert!(!b.or(db, build, || c).is_budget_exceeded());
        let fresh = ConstructionBudget::default();
        let build = builder.with_budget(&fresh);
        assert!(
            !a.and(db, build, || b.or(db, build, || c))
                .is_budget_exceeded()
        );
    }

    #[test]
    fn invariant_tuple_overlap_propagates_exhaustion() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let list = |elements| {
            let tuple = Type::heterogeneous_tuple(db, &env, elements);
            KnownClass::List.to_specialized_instance(db, &env, &[tuple])
        };
        let left = list(["A", "B"].map(|name| Type::TypeVar(create_typevar(db, name))));
        let right = list([KnownClass::Int, KnownClass::Str].map(|class| known_instance(db, class)));
        let builder = ConstraintSetBuilder::new();
        let budget = ConstructionBudget::Limited(Cell::new(0));
        let disjoint = |build| left.when_disjoint_from(db, &env, right, build, TypeVarSet::None);
        assert_matches!(
            disjoint(builder.with_budget(&budget)),
            ConstraintSet::BudgetExceeded
        );
        assert!(disjoint(builder.unlimited()).is_trivially_never_satisfied());
    }

    #[test]
    fn eliminating_a_typevar_charges_visits_even_without_new_pairs() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let set = create_constraint(db, build, t, KnownClass::Int);
        let remove = TypeVarSet::from_typevars(db, [t]);
        let project = |build| set.reduce_inferable(db, &env, build, remove, remove);
        let budget = ConstructionBudget::Limited(Cell::new(1));
        assert_matches!(
            project(builder.with_budget(&budget)),
            ConstraintSet::BudgetExceeded
        );
        assert!(project(build).is_trivially_always_satisfied());
    }

    #[test]
    fn equivalence_propagates_exhaustion_from_a_temporary_intersection() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let [a, b] = ["A", "B"]
            .map(|name| create_constraint(db, build, create_typevar(db, name), KnownClass::Int));
        let budget = ConstructionBudget::Limited(Cell::new(0));
        assert_eq!(
            a.is_equivalent_to(db, &env, b, TypeVarSet::All, &budget),
            Err(ProjectionError::ConstructionBudgetExceeded)
        );
        assert_eq!(
            a.is_equivalent_to(db, &env, b, TypeVarSet::All, build.budget),
            Ok(false)
        );
    }

    #[test]
    fn importing_separated_alternatives_respects_the_construction_budget() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let vars = ["A", "B", "C", "D"].map(|name| create_typevar(db, name));
        let owned = ConstraintSetBuilder::new().into_owned(|build| {
            vars.into_iter().when_all(db, build, |var| {
                create_constraint(db, build, var, KnownClass::Int).or(db, build, || {
                    create_constraint(db, build, var, KnownClass::Str)
                })
            })
        });
        let fresh = ConstraintSetBuilder::new();
        let budget = || ConstructionBudget::Limited(Cell::new(16));
        assert!(
            !fresh
                .with_budget(&budget())
                .load(db, &env, &owned)
                .is_budget_exceeded()
        );

        // Separating the two choices for every variable expands the same diagram on import.
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        for var in vars {
            create_constraint(db, build, var, KnownClass::Int);
        }
        assert_matches!(
            builder.with_budget(&budget()).load(db, &env, &owned),
            ConstraintSet::BudgetExceeded
        );

        // A failed import must leave the builder usable and preserve every combination.
        let loaded = build.load(db, &env, &owned);
        let inferable = TypeVarSet::from_typevars(db, vars);
        assert_matches!(
            loaded.solutions(db, &env, inferable),
            Ok(Solutions::Constrained(SolutionPaths::Complete(paths))) if paths.len() == 1 << vars.len()
        );
    }

    fn create_constraint_set_with_bounds<'db, 'c>(
        db: &'db TestDb,
        env: &ProgramEnvironment<'db>,
        build: Construction<'_, 'c, 'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Option<Type<'db>>,
        upper: Option<Type<'db>>,
    ) -> ConstraintSet<'db, 'c> {
        match (lower, upper) {
            (None, None) => ConstraintSet::always(build.builder),
            (Some(lower), None) => ConstraintSet::constrain_typevar_lower_bound(
                db,
                env,
                build,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                typevar,
                lower,
            ),
            (None, Some(upper)) => ConstraintSet::constrain_typevar_upper_bound(
                db,
                env,
                build,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                typevar,
                upper,
            ),
            (Some(lower), Some(upper)) => ConstraintSet::constrain_typevar(
                db,
                env,
                build,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                typevar,
                lower,
                upper,
            ),
        }
    }

    fn known_instance(db: &TestDb, class: KnownClass) -> Type<'_> {
        class.to_instance(db, &db.program_environment())
    }

    fn bounded_path_bounds<'db>(
        db: &'db TestDb,
        construction: &ConstructionBudget,
        set: ConstraintSet<'db, '_>,
        inferable: TypeVarSet<'db>,
        max_paths: usize,
        max_visits: usize,
    ) -> Result<CandidateSolutions<'db>, ProjectionError> {
        CandidateSolutions::compute(
            db,
            &db.program_environment(),
            set,
            inferable,
            SolutionLimits::bounded(
                SolutionBudget {
                    paths: max_paths,
                    visits: max_visits,
                    ..SolutionBudget::default()
                },
                construction,
            ),
            UnspecializedTypeVarPolicy::Ignore,
            CandidateSearch::All(ReportDiagnostics::Yes),
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

    #[test]
    fn shared_promotion_respects_dependent_bounds() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let vars = ["A", "B", "C"].map(|name| create_typevar(db, name));
        let context = GenericContext::from_typevar_instances(db, &env, vars);
        let [a, b, c] = vars.map(Type::TypeVar);
        let literal = Type::string_literal(db, "x");
        let object = Type::object();
        let tuple = |types| Type::heterogeneous_tuple(db, &env, types);

        // The left arm permits widening C with B, and the right permits widening C with A.
        // Widening C alone would violate its upper bound on both arms.
        let left = tuple([literal, object, b]);
        let right = tuple([object, literal, a]);
        let target = UnionType::from_two_elements(db, &env, left, right);
        let bounds = tuple([a, b, c]).assignable_solutions_for_argument_context(
            db,
            &env,
            target,
            context.inferable_typevars(db),
        );
        let original = context.repeat_specialization(db, literal);
        let widest = context.repeat_specialization(db, known_instance(db, KnownClass::Str));
        assert_eq!(
            bounds.promote_for_argument_context(db, &env, original, widest),
            original
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
        let u = u.with_typevar(db, declaration);
        let actual_bound = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(u)]);
        let mut storage = ConstraintSetStorage::default();
        let data = AtomicConstraint::ConcreteUpper(ConcreteUpperBound::new(
            ConstraintProvenance::INFERRED,
            t,
            actual_bound,
        ));
        let support = storage.intern_constraint_typevars(db, &env, &data.into());
        let mentioned = support
            .iter()
            .map(|typevar| storage.typevar_data(typevar))
            .collect::<Vec<_>>();

        assert_eq!(mentioned, vec![t, u]);
        assert!(support.is_complete());

        let builder = ConstraintSetBuilder::new();
        let constraint = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            builder.unlimited(),
            ConstraintProvenance::INFERRED,
            TypeVarSet::All,
            t,
            actual_bound,
        );
        let constraint = constraint.diagram().unwrap();
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
            let u = u.with_typevar(db, declaration);
            let mut storage = ConstraintSetStorage::default();
            let data = AtomicConstraint::TypeVarRange(TypeVarRangeBound::new(
                db,
                ConstraintProvenance::INFERRED,
                t,
                u,
            ));
            let support = storage.intern_constraint_typevars(db, &env, &data.into());
            let mentioned = support
                .iter()
                .map(|typevar| storage.typevar_data(typevar))
                .collect::<Vec<_>>();

            assert_eq!(mentioned, vec![t, u]);
            assert!(support.is_complete());
        }
    }

    #[test]
    fn validity_closes_over_typevars_in_declared_upper_bounds() {
        // Both evidence paths violate `T ≤ U ≤ int`. `U` occurs only in `T`'s declared upper bound,
        // so rejecting them requires validity traversal to add `U` to its worklist.
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(db, &env);
        let u = create_typevar(db, "U")
            .map_bound_or_constraints(db, |_| Some(TypeVarBoundOrConstraints::UpperBound(int)));
        let t = create_typevar(db, "T").map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(u)))
        });
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let set = create_constraint(db, build, t, KnownClass::Str).or(db, build, || {
            create_constraint(db, build, t, KnownClass::Bytes)
        });

        assert_eq!(
            CandidateSolutions::compute(
                db,
                &env,
                set,
                TypeVarSet::All,
                SolutionLimits::bounded(SolutionBudget::default(), build.budget),
                UnspecializedTypeVarPolicy::Ignore,
                CandidateSearch::All(ReportDiagnostics::Yes),
            ),
            Ok(CandidateSolutions::Unsatisfiable)
        );
    }

    #[test]
    fn upper_bound_collapses_never() {
        let db = setup_db();
        let db = &db;
        let int = known_instance(db, KnownClass::Int);

        let mut upper = UpperBound::from_clause(int);
        upper.add_clause(ConstraintProvenance::INFERRED, Type::Never);
        assert_eq!(
            upper
                .iter_evidence(EvidenceSource::Inferred)
                .collect::<Vec<_>>(),
            vec![Type::Never]
        );

        upper.add_clause(ConstraintProvenance::INFERRED, int);
        assert_eq!(
            upper
                .iter_evidence(EvidenceSource::Inferred)
                .collect::<Vec<_>>(),
            vec![Type::Never]
        );
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
                upper.add_clause(ConstraintProvenance::INFERRED, clause);
            }

            assert_eq!(upper.iter_evidence(EvidenceSource::Inferred).count(), 2);
            assert_eq!(upper.as_single_bound(db, &env), Some(expected));
        }
    }

    #[test]
    fn upper_bound_distinguishes_missing_bound_from_explicit_object() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();

        let missing = UpperBound::unconstrained();
        assert!(!missing.has_evidence_without_validity(EvidenceSource::All));
        assert_eq!(missing.as_single_bound(db, &env), Some(Type::object()));

        let explicit = UpperBound::from_clause(Type::object());
        assert!(explicit.has_evidence_without_validity(EvidenceSource::All));
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
                upper.add_clause(ConstraintProvenance::INFERRED, clause);
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
        upper.add_clause(ConstraintProvenance::INFERRED, int);
        assert_eq!(upper.as_single_bound(db, &env), None);
    }

    #[test]
    fn trivial_disjointness_does_not_claim_bounded_typevar_class_is_disjoint() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let bool = known_instance(db, KnownClass::Bool);
        let u = create_typevar(db, "U")
            .map_bound_or_constraints(db, |_| Some(TypeVarBoundOrConstraints::UpperBound(bool)));
        let type_of_u = SubclassOfType::from(db, &env, u);
        let bool_class = KnownClass::Bool.to_class_literal(db, &env);

        for (left, right) in [(type_of_u, bool_class), (bool_class, type_of_u)] {
            let trivial =
                left.when_trivially_disjoint_from(db, &env, right, build, TypeVarSet::None);
            let full = left.when_disjoint_from(db, &env, right, build, TypeVarSet::None);

            assert!(trivial.is_trivially_never_satisfied());
            assert!(!full.is_always_satisfied(db, &env, TypeVarSet::All));
        }
    }

    #[test]
    fn trivial_disjointness_implies_full_disjointness() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
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
                    left.when_trivially_disjoint_from(db, &env, right, build, TypeVarSet::None);
                if trivial.is_trivially_always_satisfied() {
                    positive_results += 1;
                    assert!(
                        left.when_disjoint_from(db, &env, right, build, TypeVarSet::None)
                            .is_always_satisfied(db, &env, TypeVarSet::All),
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
    fn satisfiability_respects_visit_budget() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let mut set = ConstraintSet::always(&builder);
        for index in 0..8 {
            let t = BoundTypeVarInstance::synthetic(
                db,
                &env,
                Name::new(format!("T{index}")),
                TypeVarVariance::Invariant,
            );
            set = set.and(db, build, || {
                create_constraint(db, build, t, KnownClass::Int).or(db, build, || {
                    create_constraint(db, build, t, KnownClass::Str)
                })
            });
        }

        // Leave room for preprocessing, but require the satisfiability walk to respect the limit.
        assert_eq!(
            bounded_path_bounds(db, build.budget, set, TypeVarSet::All, usize::MAX, 16),
            Err(ProjectionError::TraversalBudgetExceeded)
        );
        assert!(matches!(
            bounded_path_bounds(
                db,
                build.budget,
                set,
                TypeVarSet::All,
                usize::MAX,
                usize::MAX
            ),
            Ok(CandidateSolutions::Constrained(_))
        ));
    }

    #[test]
    fn bounded_path_fast_paths_respect_limits() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
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
                bounded_path_bounds(db, build.budget, set, inferable, 0, 0),
                Err(ProjectionError::TraversalBudgetExceeded)
            );
            assert_eq!(
                bounded_path_bounds(db, build.budget, set, inferable, 0, 1),
                Ok(expected)
            );
        }

        let set = create_constraint(db, build, t, KnownClass::Int);
        let expected = CandidateSolutions::compute(
            db,
            &env,
            set,
            inferable,
            SolutionLimits::unbounded(build.budget),
            UnspecializedTypeVarPolicy::Ignore,
            CandidateSearch::All(ReportDiagnostics::Yes),
        )
        .unwrap();
        assert_eq!(
            bounded_path_bounds(db, build.budget, set, inferable, 0, 2),
            Err(ProjectionError::PathBudgetExceeded)
        );
        assert_eq!(
            bounded_path_bounds(db, build.budget, set, inferable, 1, 1),
            Err(ProjectionError::TraversalBudgetExceeded)
        );
        assert_eq!(
            bounded_path_bounds(db, build.budget, set, inferable, 1, 2),
            Ok(expected)
        );
    }

    #[test]
    fn default_solve_leaves_unbounded_typevar_unsolved_without_bounds() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let inferable = TypeVarSet::from_typevars(db, [t]);
        let builder = ConstraintSetBuilder::new();
        let path_bound = CandidateTypeVarSolution {
            bound_typevar: t,
            inferable,
            evidence_lower: None,
            inferred_lower: None,
            declared_lower: None,
            validity_lower: Type::Never,
            upper: UpperBound::unconstrained(),
            has_declared_evidence: false,
            has_only_non_concrete_evidence: None,
            selected_declared_constraint: None,
        };

        assert_eq!(
            CandidateSolutions::default_solve(db, &env, &builder, &path_bound),
            PathBoundSolution::Unsolved
        );
        assert_eq!(PathBoundSolution::Unsolved.as_type(), None);
        assert_eq!(
            CandidateSolutions::Constrained(Box::new([CandidateSolution {
                typevars: Box::new([path_bound]),
                validity: SolutionValidity::Valid,
            }]))
            .solve(db, &env, &builder),
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
        let mut bounds = CandidateTypeVarSolver::default();
        bounds.add_lower(
            ConstraintProvenance::INFERRED,
            known_instance(db, KnownClass::Int),
        );
        bounds.add_upper(
            ConstraintProvenance::INFERRED,
            known_instance(db, KnownClass::Str),
        );
        let mut storage = builder.storage.borrow_mut();
        let inferable = TypeVarSet::from_typevars(db, [t]);
        let invalid = bounds
            .finish(
                db,
                &env,
                &mut storage,
                t,
                inferable,
                &ConstructionBudget::Unlimited,
            )
            .unwrap();
        assert_eq!(invalid, None);
        drop(storage);

        assert_eq!(PathBoundSolution::Unsatisfiable.as_type(), None);
        assert_eq!(
            CandidateSolutions::default_solve(
                db,
                &env,
                &builder,
                &CandidateTypeVarSolution::from_equivalence(t, Type::Never, inferable),
            ),
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
        let inferable = TypeVarSet::from_typevars(db, [t, u]);
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);
        let binding = |bound_typevar, solution| TypeVarSolution {
            bound_typevar,
            solution,
        };

        for lower in [None, Some(Type::any())] {
            let mut bounds = CandidateTypeVarSolver::default();
            if let Some(lower) = lower {
                bounds.add_lower(ConstraintProvenance::INFERRED, lower);
            }
            bounds.add_upper(ConstraintProvenance::INFERRED, left);
            bounds.add_upper(ConstraintProvenance::INFERRED, right);
            let mut storage = builder.storage.borrow_mut();
            let exhausted = bounds
                .finish(db, &env, &mut storage, t, inferable, build.budget)
                .unwrap()
                .expect("expected a valid solution");
            drop(storage);
            let expected = PathBoundSolution::BudgetExceeded { fallback: lower };
            assert_eq!(
                CandidateSolutions::preliminary_solve(db, &env, &builder, &exhausted),
                lower.map_or(expected, PathBoundSolution::Solved)
            );
            assert_eq!(
                CandidateSolutions::default_solve(db, &env, &builder, &exhausted),
                expected
            );
            assert_eq!(expected.as_type(), lower);

            for reverse in [false, true] {
                let mut paths = vec![
                    CandidateSolution {
                        typevars: vec![
                            exhausted.clone(),
                            CandidateTypeVarSolution::from_equivalence(u, str, inferable),
                        ]
                        .into_boxed_slice(),
                        validity: SolutionValidity::Valid,
                    },
                    CandidateSolution {
                        typevars: vec![CandidateTypeVarSolution::from_equivalence(
                            t, int, inferable,
                        )]
                        .into_boxed_slice(),
                        validity: SolutionValidity::Valid,
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
                        .solve(db, &env, &builder),
                    Solutions::Constrained(SolutionPaths::BudgetExceeded(expected_paths))
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
        let mut bounds = CandidateTypeVarSolver::default();
        for upper in gradual_upper {
            bounds.add_upper(ConstraintProvenance::INFERRED, upper);
        }
        let mut storage = builder.storage.borrow_mut();
        let exhausted = bounds
            .finish(
                db,
                &env,
                &mut storage,
                constrained,
                TypeVarSet::from_typevars(db, [constrained]),
                build.budget,
            )
            .unwrap()
            .expect("expected a valid solution");
        drop(storage);
        assert_eq!(exhausted.has_only_non_concrete_evidence, Some(true));
        assert_eq!(
            CandidateSolutions::preliminary_solve(db, &env, &builder, &exhausted),
            PathBoundSolution::BudgetExceeded { fallback: None }
        );
        assert_eq!(
            CandidateSolutions::default_solve(db, &env, &builder, &exhausted),
            PathBoundSolution::BudgetExceeded { fallback: None }
        );
        Ok(())
    }

    #[test]
    fn gradual_equivalence_preserves_alternative_bounds() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let int = known_instance(db, KnownClass::Int);
        let str = known_instance(db, KnownClass::Str);
        let list = |ty| KnownClass::List.to_specialized_instance(db, &env, &[ty]);

        for dynamic in [Type::any(), Type::unknown()] {
            for (gradual, bound, witness) in
                [(dynamic, int, str), (list(dynamic), list(int), list(str))]
            {
                for (lower, upper) in [(Some(bound), None), (None, Some(bound))] {
                    let builder = ConstraintSetBuilder::new();
                    let build = builder.unlimited();
                    let equivalent = ConstraintSet::constrain_typevar_equivalence_bound(
                        db,
                        &env,
                        build,
                        ConstraintProvenance::INFERRED,
                        TypeVarSet::All,
                        t,
                        gradual,
                    );
                    let range = create_constraint_set_with_bounds(db, &env, build, t, lower, upper);
                    let witness = ConstraintSet::constrain_typevar_equivalence_bound(
                        db,
                        &env,
                        build,
                        ConstraintProvenance::INFERRED,
                        TypeVarSet::All,
                        t,
                        witness,
                    );

                    // `T = Any` permits `T = str`, even though `int ≤ T` (or `T ≤ int`)
                    // does not. The same holds inside an invariant container.
                    let alternatives = equivalent
                        .or(db, build, || range)
                        .and(db, build, || witness);
                    assert!(!alternatives.is_never_satisfied(db, &env, TypeVarSet::All));

                    // Requiring both bounds still rules out that specialization.
                    let conjunction = equivalent
                        .and(db, build, || range)
                        .and(db, build, || witness);
                    assert!(conjunction.is_never_satisfied(db, &env, TypeVarSet::All));
                }
            }
        }
    }

    #[test]
    fn trivial_satisfaction_only_recognizes_terminals() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let t_str = create_constraint(db, build, t, KnownClass::Str);
        let impossible = t_int.and(db, build, || t_str);

        assert!(ConstraintSet::always(&builder).is_trivially_always_satisfied());
        assert!(!ConstraintSet::always(&builder).is_trivially_never_satisfied());
        assert!(ConstraintSet::never(&builder).is_trivially_never_satisfied());
        assert!(!ConstraintSet::never(&builder).is_trivially_always_satisfied());
        assert!(!t_int.is_trivially_always_satisfied());
        assert!(!t_int.is_trivially_never_satisfied());
        assert!(impossible.is_never_satisfied(db, &env, TypeVarSet::All));
        assert!(!impossible.is_trivially_never_satisfied());

        let t_bool_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            build,
            ConstraintProvenance::INFERRED,
            TypeVarSet::All,
            t,
            KnownClass::Bool.to_instance(db, &env),
        );
        let t_int_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            build,
            ConstraintProvenance::INFERRED,
            TypeVarSet::All,
            t,
            KnownClass::Int.to_instance(db, &env),
        );
        let tautology = t_bool_upper.negate(db, build).or(db, build, || t_int_upper);

        assert!(tautology.is_always_satisfied(db, &env, TypeVarSet::All));
        assert!(!tautology.is_trivially_always_satisfied());
    }

    #[test]
    fn a_tautology_rules_out_an_all_negative_counterexample() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let [t, u, v] = ["T", "U", "V"].map(|name| create_typevar(db, name));

        // T ≤ U or T ≤ V can be false, but T ≤ U or T ≤ T always holds.
        for (right, always) in [(v, false), (t, true)] {
            let builder = ConstraintSetBuilder::new();
            let bound = |other| {
                let range = TypeVarRangeBound::new(db, ConstraintProvenance::INFERRED, t, other);
                let (node, order) = Constraint::Atomic(AtomicConstraint::TypeVarRange(range))
                    .new_node(db, &env, &mut builder.storage.borrow_mut());
                ConstraintSet::from_node(&builder, node, order)
            };
            let set = bound(u).or(db, builder.unlimited(), || bound(right));
            assert_eq!(set.is_always_satisfied_in_isolation(db, &env), always);
        }
    }

    #[test]
    fn combinators_only_short_circuit_on_terminal_saturation() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let t_str = create_constraint(db, build, t, KnownClass::Str);
        let impossible = t_int.and(db, build, || t_str);
        let t_bool_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            build,
            ConstraintProvenance::INFERRED,
            TypeVarSet::All,
            t,
            KnownClass::Bool.to_instance(db, &env),
        );
        let t_int_upper = ConstraintSet::constrain_typevar_upper_bound(
            db,
            &env,
            build,
            ConstraintProvenance::INFERRED,
            TypeVarSet::All,
            t,
            KnownClass::Int.to_instance(db, &env),
        );
        let tautology = t_bool_upper.negate(db, build).or(db, build, || t_int_upper);

        let forced = Cell::new(0);
        ConstraintSet::never(&builder).and(db, build, || {
            forced.set(forced.get() + 1);
            t_int
        });
        ConstraintSet::always(&builder).or(db, build, || {
            forced.set(forced.get() + 1);
            t_int
        });
        assert_eq!(forced.get(), 0);

        impossible.and(db, build, || {
            forced.set(forced.get() + 1);
            t_int
        });
        tautology.or(db, build, || {
            forced.set(forced.get() + 1);
            t_int
        });
        assert_eq!(forced.get(), 2);

        let visited = Cell::new(0);
        [impossible, t_int].into_iter().when_all(db, build, |set| {
            visited.set(visited.get() + 1);
            set
        });
        assert_eq!(visited.get(), 2);

        visited.set(0);
        [tautology, t_int].into_iter().when_any(db, build, |set| {
            visited.set(visited.get() + 1);
            set
        });
        assert_eq!(visited.get(), 2);

        visited.set(0);
        [ConstraintSet::never(&builder), t_int]
            .into_iter()
            .when_all(db, build, |set| {
                visited.set(visited.get() + 1);
                set
            });
        assert_eq!(visited.get(), 1);

        visited.set(0);
        [ConstraintSet::always(&builder), t_int]
            .into_iter()
            .when_any(db, build, |set| {
                visited.set(visited.get() + 1);
                set
            });
        assert_eq!(visited.get(), 1);
    }

    #[test]
    fn never_satisfied_results_depend_on_inferable() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let t_str = create_constraint(db, build, t, KnownClass::Str);
        let impossible = t_int.and(db, build, || t_str);

        assert!(!t_int.is_never_satisfied(db, &env, TypeVarSet::All));
        assert!(!t_int.is_never_satisfied(db, &env, TypeVarSet::All));
        assert!(impossible.is_never_satisfied(db, &env, TypeVarSet::All));
        assert!(impossible.is_never_satisfied(db, &env, TypeVarSet::All));
        assert!(ConstraintSet::never(&builder).is_never_satisfied(db, &env, TypeVarSet::All));
        assert!(!ConstraintSet::always(&builder).is_never_satisfied(db, &env, TypeVarSet::All));

        let owned = create_compacted_owned_set(db);
        owned.query(|_builder, set| {
            assert!(!set.is_never_satisfied(db, &env, TypeVarSet::All));
            assert!(!set.is_never_satisfied(db, &env, TypeVarSet::All));
        });

        // list[U] ≤ T ≤ list[int] is satisfiable only when U is also inferable.
        let u = create_typevar(db, "U");
        let infer_t = TypeVarSet::from_typevars(db, [t]);
        let infer_tu = TypeVarSet::from_typevars(db, [t, u]);
        let int = known_instance(db, KnownClass::Int);
        let list = |ty| KnownClass::List.to_specialized_instance(db, &env, &[ty]);
        let lower = list(Type::TypeVar(u));
        let upper = list(int);
        let owned = ConstraintSetBuilder::new().into_owned(|builder| {
            ConstraintSet::constrain_typevar(
                db,
                &env,
                builder,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                t,
                lower,
                upper,
            )
        });

        // The answer must not depend on which inferable set was queried first.
        owned.query(|_builder, set| {
            assert!(set.is_never_satisfied(db, &env, infer_t));
            assert!(!set.is_never_satisfied(db, &env, infer_tu));
        });
        owned.query(|_builder, set| {
            assert!(!set.is_never_satisfied(db, &env, infer_tu));
            assert!(set.is_never_satisfied(db, &env, infer_t));
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
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);

        let first = t_int.and(db, build, || u_str);
        let second = u_str.and(db, build, || t_int);

        let first_diagram = first.diagram().unwrap();
        let second_diagram = second.diagram().unwrap();
        assert_eq!(first_diagram.node, second_diagram.node);
        assert_ne!(first_diagram.source_order, second_diagram.source_order);
        assert!(!first.is_never_satisfied(db, &env, TypeVarSet::All));
        assert!(!second.is_never_satisfied(db, &env, TypeVarSet::All));
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
        ) -> impl Iterator<Item = Result<AtomicConstraint<'db>, UnsatisfiableBound>> {
            let PermutedConstraint(typevar, provenance, lower, upper) = self;
            iter::chain(
                lower.into_iter().flat_map(move |lower| {
                    AtomicConstraint::new_lower_bound(
                        db,
                        provenance,
                        typevar,
                        lower,
                        TypeVarSet::All,
                    )
                }),
                upper.into_iter().flat_map(move |upper| {
                    AtomicConstraint::new_upper_bound(
                        db,
                        env,
                        provenance,
                        typevar,
                        upper,
                        TypeVarSet::All,
                    )
                }),
            )
        }

        fn node(
            self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            storage: &mut ConstraintSetStorage<'db>,
            budget: &ConstructionBudget,
        ) -> NodeId {
            let constraints = self.constraints(db, env);
            let (node, _) = Constraint::new_nodes(db, env, storage, constraints, budget).unwrap();
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
        build_bdd: impl Fn(
            &mut ConstraintSetStorage<'db>,
            &ConstructionBudget,
        ) -> Result<NodeId, ProjectionError>,
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
                    storage.intern_constraint(db, &env, constraint.into());
                }
            }

            let node = build_bdd(&mut storage, &ConstructionBudget::Unlimited).unwrap();
            let source_order = atoms
                .iter()
                .flat_map(|atom| atom.constraints(db, &env).filter_map(Result::ok))
                .fold(None, |source_order, constraint| {
                    let constraint = storage.intern_atomic_constraint(db, &env, constraint);
                    let constraint_source_order =
                        storage.atomic_constraint_source_order(constraint);
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
                set.is_never_satisfied(db, &env, TypeVarSet::All),
                set.is_always_satisfied(db, &env, TypeVarSet::All),
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
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(str), None),
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(int), None),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t],
            &atoms,
            |storage, budget| {
                let [str_t, int_t] = atoms.map(|atom| atom.node(db, &env, storage, budget));
                str_t
                    .or(storage, int_t, budget)?
                    .and(storage, str_t, budget)
            },
            ["never=false always=false merged=[T=str] paths=[T=str]"],
        );

        check_solutions_for_constraint_orderings(
            db,
            &[t],
            &atoms,
            |storage, budget| {
                let [str_t, int_t] = atoms.map(|atom| atom.node(db, &env, storage, budget));
                str_t.or(storage, int_t, budget)
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
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(str), None),
            PermutedConstraint(u, ConstraintProvenance::INFERRED, Some(bytes), None),
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(int), None),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u],
            &atoms,
            |storage, budget| {
                let [str_t, bytes_u, int_t] =
                    atoms.map(|atom| atom.node(db, &env, storage, budget));
                let compound = str_t.and(storage, bytes_u, budget)?;
                compound
                    .or(storage, int_t, budget)?
                    .and(storage, compound, budget)
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
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(str), None),
            PermutedConstraint(u, ConstraintProvenance::INFERRED, Some(bytes), None),
            PermutedConstraint(x, ConstraintProvenance::INFERRED, Some(int), None),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u, x],
            &atoms,
            |storage, budget| {
                let [str_t, bytes_u, int_x] =
                    atoms.map(|atom| atom.node(db, &env, storage, budget));
                let early = int_x
                    .and(storage, str_t, budget)?
                    .and(storage, bytes_u, budget)?;
                let late = bytes_u.and(storage, str_t, budget)?;
                early.or(storage, late, budget)
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
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(str), None),
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(int), None),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t],
            &atoms,
            |storage, budget| {
                let [str_t, int_t] = atoms.map(|atom| atom.node(db, &env, storage, budget));
                let true_path = int_t.and(storage, str_t, budget)?;
                let false_path = int_t.negate(storage, budget)?.and(storage, str_t, budget)?;
                true_path.or(storage, false_path, budget)
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
            PermutedConstraint(t, ConstraintProvenance::INFERRED, None, Some(list_u)),
            PermutedConstraint(u, ConstraintProvenance::INFERRED, None, Some(int)),
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(list_int), None),
            PermutedConstraint(v, ConstraintProvenance::INFERRED, Some(bytes), None),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u, v],
            &atoms,
            |storage, budget| {
                let [t_list_u, u_int, list_int_t, bytes_v] =
                    atoms.map(|atom| atom.node(db, &env, storage, budget));
                t_list_u
                    .and(storage, u_int, budget)?
                    .and(storage, list_int_t, budget)?
                    .or(storage, bytes_v, budget)
            },
            // The unrelated `V = bytes` alternative must not pick up bindings for `T` or `U`.
            [
                "never=false always=false merged=[T=list[int], U=int, V=bytes] paths=[U=int, T=list[int]; V=bytes]",
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
            PermutedConstraint(t, ConstraintProvenance::INFERRED, None, Some(int)),
            PermutedConstraint(t, ConstraintProvenance::INFERRED, None, Some(str)),
            PermutedConstraint(u, ConstraintProvenance::INFERRED, Some(bytes), None),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u],
            &atoms,
            |storage, budget| {
                let [t_int, t_str, bytes_u] =
                    atoms.map(|atom| atom.node(db, &env, storage, budget));
                t_int
                    .or(storage, t_str, budget)?
                    .negate(storage, budget)?
                    .or(storage, bytes_u, budget)
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
            PermutedConstraint(t, ConstraintProvenance::INFERRED, None, Some(int)),
            PermutedConstraint(t, ConstraintProvenance::INFERRED, None, Some(str)),
            PermutedConstraint(t, ConstraintProvenance::INFERRED, Some(int), None),
            PermutedConstraint(u, ConstraintProvenance::INFERRED, None, Some(int)),
        ];

        check_solutions_for_constraint_orderings(
            db,
            &[t, u],
            &atoms,
            |storage, budget| {
                let [t_int, t_str, int_t, u_int] =
                    atoms.map(|atom| atom.node(db, &env, storage, budget));
                t_int
                    .or(storage, t_str, budget)?
                    .and(storage, int_t, budget)?
                    .and(storage, u_int, budget)
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
        let actual = set
            .diagram()
            .unwrap()
            .node
            .display_graph(db, &env, &storage, &"")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_display_graph_output() {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let constraints = ConstraintSetBuilder::new();
        let build = constraints.unlimited();
        let t_str = create_constraint(db, build, t, KnownClass::Str);
        let t_bool = create_constraint(db, build, t, KnownClass::Bool);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let u_bool = create_constraint(db, build, u, KnownClass::Bool);
        // Construct this in a different order than above to make the source_orders more
        // interesting.
        let set =
            (u_str.or(db, build, || u_bool)).and(db, build, || t_str.or(db, build, || t_bool));
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
        let t_int = create_constraint(&db, builder.unlimited(), t, KnownClass::Int);
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
        let build = builder.unlimited();

        // Neither lhs nor rhs have uncertain branches (checked above). The operand with the
        // "lower" BDD variable (in this case, the lhs) is parked into a new uncertain branch in
        // the union result.
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let union = t_int.or(db, build, || u_str);
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
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let t_bool = create_constraint(db, build, t, KnownClass::Bool);
        let u_int = create_constraint(db, build, u, KnownClass::Int);

        // lhs and rhs both have uncertain branches (checked above). These uncertain branches are
        // carried through to the intersection result.
        let lhs = t_int.or(db, build, || u_str);
        let rhs = t_bool.or(db, build, || u_int);
        let intersection = lhs.and(db, build, || rhs);
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
        let build = builder.unlimited();
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let v = create_typevar(db, "V");
        let last = create_constraint(db, build, t, KnownClass::Int);
        let middle = create_constraint(db, build, u, KnownClass::Str);
        let first = create_constraint(db, build, v, KnownClass::Bytes);

        // The uncertain branch already accepts every assignment of the stronger guarded path,
        // whether that path requires or excludes the first constraint.
        for guard in [first, first.negate(db, build)] {
            let stronger = guard.and(db, build, || middle).and(db, build, || last);
            let absorbed = stronger.or(db, build, || middle);
            assert_eq!(
                absorbed.diagram().unwrap().node,
                middle.diagram().unwrap().node
            );
        }
    }

    #[test]
    fn disjunction_of_independent_conjunctions_stays_compact() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
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
                    create_constraint(db, build, typevar, KnownClass::Int)
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
            .map(|(x, y)| x.and(db, build, || y))
            .collect();
        while groups.len() > 1 {
            groups = groups
                .chunks(2)
                .map(|pair| {
                    let left = pair[0];
                    pair.get(1)
                        .map_or(left, |right| left.or(db, build, || *right))
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
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let union = t_int.or(db, build, || u_str);
        let negated = union.negate(db, build);
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
        let build = builder.unlimited();

        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let tdd = t_int.or(db, build, || u_str);
        let negated = tdd.negate(db, build);

        // T ∧ ¬T == false
        assert!(
            tdd.and(db, build, || negated)
                .is_never_satisfied(db, &env, TypeVarSet::All)
        );

        // T ∨ ¬T == true
        assert!(
            tdd.or(db, build, || negated)
                .is_always_satisfied(db, &env, TypeVarSet::All)
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
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let tdd = t_int.or(db, build, || u_str);
        let negated = tdd.negate(db, build);
        let double_negated = negated.negate(db, build);
        assert!(
            tdd.is_equivalent_to(db, &env, double_negated, TypeVarSet::All, build.budget)
                .unwrap()
        );
        assert!(
            !tdd.is_equivalent_to(db, &env, t_int, TypeVarSet::All, build.budget)
                .unwrap()
        );
        assert!(
            !t_int
                .is_equivalent_to(db, &env, tdd, TypeVarSet::All, build.budget)
                .unwrap()
        );

        let equivalent = tdd.iff(db, build, double_negated);
        assert!(equivalent.is_always_satisfied(db, &env, TypeVarSet::All));
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
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let tdd = t_int.or(db, build, || u_str);

        // iff(T, T) == true
        assert!(
            tdd.iff(db, build, tdd)
                .is_always_satisfied(db, &env, TypeVarSet::All)
        );

        // iff(T, ¬T) == false
        let negated = tdd.negate(db, build);
        assert!(
            tdd.iff(db, build, negated)
                .is_never_satisfied(db, &env, TypeVarSet::All)
        );
    }

    #[test]
    fn constraint_set_source_order_combination_is_idempotent() {
        let db = setup_db();
        let db = &db;
        let t = create_typevar(db, "T");
        let u = create_typevar(db, "U");
        let builder = ConstraintSetBuilder::new();
        let build = builder.unlimited();
        let t_int = create_constraint(db, build, t, KnownClass::Int);
        let u_str = create_constraint(db, build, u, KnownClass::Str);
        let combined = t_int.and(db, build, || u_str);

        let t_bool = create_constraint(db, build, t, KnownClass::Bool);
        let u_int = create_constraint(db, build, u, KnownClass::Int);
        let alternatives = t_int
            .and(db, build, || u_int)
            .or(db, build, || u_str.and(db, build, || t_bool));

        for original in [t_int, combined, alternatives] {
            let storage = builder.storage.borrow();
            let original_source_order_count = storage.source_orders.len();
            drop(storage);
            let intersection = original.and(db, build, || original);
            let union = original.or(db, build, || original);

            let original = original.diagram().unwrap();
            for combined in [intersection, union] {
                let combined = combined.diagram().unwrap();
                assert_eq!(combined.node, original.node);
                assert_eq!(combined.source_order, original.source_order);
            }
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
        let build = builder.unlimited();
        let mut left = create_constraint(db, build, t, KnownClass::Int);
        let mut right = create_constraint(db, build, t, KnownClass::Str);
        let expected = {
            let storage = builder.storage.borrow();
            [left.diagram().unwrap().node, right.diagram().unwrap().node].map(|node| {
                storage
                    .interior_node_data(node)
                    .constraint
                    .expect_atomic(&storage)
            })
        };
        let original = left.or(db, build, || right);

        // The TDD stops growing, but each sidecar shares both of its predecessors. Walking the
        // sidecar as a tree would take exponentially many steps.
        for _ in 0..63 {
            let next = left.or(db, build, || right);
            left = right;
            right = next;
        }
        assert_eq!(
            right.diagram().unwrap().node,
            original.diagram().unwrap().node
        );
        assert_eq!(
            builder
                .storage
                .borrow()
                .calculate_source_orders(right.diagram().unwrap().source_order)
                .into_iter()
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn deeply_nested_source_order_preserves_first_occurrences() {
        let mut storage = ConstraintSetStorage::default();
        let first = AtomicConstraintId::new(0);
        let second = AtomicConstraintId::new(1);
        let first_order = storage.atomic_constraint_source_order(first);
        let second_order = storage.atomic_constraint_source_order(second);
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
            let original = ConstraintSetBuilder::new().into_owned(|build| {
                create_constraint_set_with_bounds(db, &env, build, t, lower, upper)
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

        let original = ConstraintSetBuilder::new().into_owned(|build| {
            let _unused_t_int = create_constraint(db, build, t, KnownClass::Int);
            let _unused_str = create_constraint(db, build, unused, KnownClass::Str);
            ConstraintSet::constrain_typevar_upper_bound(
                db,
                &env,
                build,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                t,
                Type::TypeVar(u),
            )
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
            ConstraintSet::constrain_typevar_upper_bound(
                db,
                &env,
                builder,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                t,
                Type::TypeVar(u),
            )
        });
        let destination = ConstraintSetBuilder::new()
            .into_owned(|build| create_constraint(db, build, u, KnownClass::Int));

        destination.query(|builder, _| {
            let build = builder.unlimited();
            let original_u_id = builder.storage.borrow_mut().typevar_id(db, u);
            let loaded = build.load(db, &env, &source);
            let direct = ConstraintSet::constrain_typevar_upper_bound(
                db,
                &env,
                build,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                t,
                Type::TypeVar(u),
            );
            assert!(
                loaded
                    .iff(db, build, direct)
                    .is_always_satisfied(db, &env, TypeVarSet::All)
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

        ConstraintSetBuilder::new().into_owned(|build| {
            let _unused_t_int = create_constraint(db, build, t, KnownClass::Int);
            let _unused_u_str = create_constraint(db, build, u, KnownClass::Str);
            create_constraint(db, build, v, KnownClass::Bool)
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

        let owned = ConstraintSetBuilder::new().into_owned(|build| {
            let t_int = create_constraint(db, build, t, KnownClass::Int);
            let u_str = create_constraint(db, build, u, KnownClass::Str);
            t_int.and(db, build, || u_str).reduce_inferable(
                db,
                &env,
                build,
                TypeVarSet::from_typevars(db, [t]),
                TypeVarSet::All,
            )
        });

        assert_eq!(
            owned.query(|_, constraints| constraints.solutions(db, &env, TypeVarSet::All)),
            Ok(Solutions::Constrained(SolutionPaths::Complete(vec![
                solution([TypeVarSolution {
                    bound_typevar: u,
                    solution: known_instance(db, KnownClass::Str),
                },])
            ]))),
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

        let owned = ConstraintSetBuilder::new().into_owned(|build| {
            let u_t = ConstraintSet::constrain_typevar_equivalence_bound(
                db,
                &env,
                build,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                u,
                Type::TypeVar(t),
            );
            let t_str = create_constraint(db, build, t, KnownClass::Str);
            let u_int = create_constraint(db, build, u, KnownClass::Int);

            // Eliminating T leaves a derived U = str alternative alongside the direct U = int.
            let projected = u_t
                .and(db, build, || t_str)
                .or(db, build, || u_int)
                .reduce_inferable(
                    db,
                    &env,
                    build,
                    TypeVarSet::from_typevars(db, [t]),
                    TypeVarSet::All,
                );
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
            let build = builder.unlimited();
            // Keep typevar orientation fixed while changing only the TDD variable order.
            for typevar in [fresh_r, fresh_a, a, r] {
                builder.storage.borrow_mut().intern_typevar(db, typevar);
            }
            let atom = |index: usize| {
                let (typevar, lower, upper) = atoms[index];
                create_constraint_set_with_bounds(db, &env, build, typevar, lower, upper)
            };
            for &index in allocation_order {
                let _ = atom(index);
            }
            let [int_r, a_int, str_r, a_str, r_bound, a_bound] = [0, 1, 2, 3, 4, 5].map(atom);

            // Eliminating FreshA and FreshR leaves both bounds of the int and str alternatives
            // on A and R. All original constraints disappear, but their source order survives.
            let projected = int_r
                .and(db, build, || a_int)
                .or(db, build, || str_r.and(db, build, || a_str))
                .and(db, build, || r_bound)
                .and(db, build, || a_bound)
                .reduce_inferable(
                    db,
                    &env,
                    build,
                    TypeVarSet::from_typevars(db, [fresh_a, fresh_r]),
                    TypeVarSet::All,
                );
            let storage = builder.storage.borrow();
            storage
                .calculate_source_orders(projected.diagram().unwrap().source_order)
                .into_iter()
                .map(|constraint| storage.atomic_constraint_data(constraint))
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
            let owned = ConstraintSetBuilder::new().into_owned(|build| {
                let earlier = ConstraintSet::constrain_typevar_lower_bound(
                    db,
                    &env,
                    build,
                    ConstraintProvenance::INFERRED,
                    TypeVarSet::All,
                    t,
                    int,
                );
                let later = create_constraint_set_with_bounds(db, &env, build, t, lower, upper);
                let absorbed = earlier.or(db, build, || later).and(db, build, || later);
                expected = Some(
                    absorbed
                        .or(db, build, || earlier)
                        .solutions(db, &env, inferable),
                );
                absorbed
            });
            assert_matches!(&expected, Some(Ok(Solutions::Constrained(_))));

            let builder = ConstraintSetBuilder::new();
            let build = builder.unlimited();
            let reloaded = build.load(db, &env, &owned);
            let earlier = ConstraintSet::constrain_typevar_lower_bound(
                db,
                &env,
                build,
                ConstraintProvenance::INFERRED,
                TypeVarSet::All,
                t,
                int,
            );
            assert_eq!(
                Some(
                    reloaded
                        .or(db, build, || earlier)
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
            let build = builder.unlimited();
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
            let existing_constraint = storage
                .interior_node_data(set.diagram().unwrap().node)
                .constraint
                .expect_atomic(&storage);
            assert_eq!(
                Some(storage.atomic_constraint_source_order(existing_constraint)),
                set.diagram().unwrap().source_order
            );
            drop(storage);

            let w = create_typevar(db, "W");
            let w_str = create_constraint(db, build, w, KnownClass::Str);
            let mut storage = builder.storage.borrow_mut();
            let diagram = w_str.diagram().unwrap();
            let new_constraint = diagram
                .node
                .root_constraint(&storage)
                .expect("new constraint should be nonterminal");

            assert!(diagram.node.index() >= node_split);
            assert!(new_constraint.index() >= constraint_split);
            assert!(storage.typevar_id(db, w).index() >= typevar_split);
            drop(storage);
            assert!(
                diagram
                    .source_order
                    .is_some_and(|source_order| source_order.index() >= source_order_split)
            );

            let combined = set.and(db, build, || w_str);
            assert!(!combined.is_never_satisfied(db, &env, TypeVarSet::All));

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
        let loaded = builder.unlimited().load(db, &env, &owned);
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
        let owned = ConstraintSetBuilder::new().into_owned(|build| {
            let _unused = create_constraint(db, build, t, KnownClass::Int);
            ConstraintSet::always(build.builder)
        });

        assert!(owned.inner.is_none());

        owned.query(|builder, set| {
            assert!(set.is_always_satisfied(db, &env, TypeVarSet::All));
            let storage = builder.storage.borrow();
            assert!(storage.compacted.is_none());
            assert!(storage.nodes.is_empty());
            assert!(storage.constraints.is_empty());
            assert!(storage.typevars.is_empty());
        });

        let builder = ConstraintSetBuilder::new();
        let loaded = builder.unlimited().load(db, &env, &owned);
        assert!(loaded.is_always_satisfied(db, &env, TypeVarSet::All));
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
        let owned = builder.into_owned(|build| {
            let t_int = create_constraint(db, build, t, KnownClass::Int);
            let u_str = create_constraint(db, build, u, KnownClass::Str);
            let result = t_int.or(db, build, || u_str);
            check_display_graph(
                db,
                build.builder,
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
        let loaded = builder.unlimited().load(db, &env, &owned);
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
