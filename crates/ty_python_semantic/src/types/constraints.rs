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
//! complex constraint sets using union, intersection, and negation operations. We use a [binary
//! decision diagram][bdd] (BDD) to represent a constraint set.
//!
//! Note that all lower and upper bounds in a constraint must be fully static. We take the bottom
//! and top materializations of the types to remove any gradual forms if needed.
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
//! [bdd]: https://en.wikipedia.org/wiki/Binary_decision_diagram

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
use crate::types::generics::{GenericContext, InferableTypeVars, Specialization};
use crate::types::visitor::{
    TypeCollector, TypeVisitor, any_over_type, walk_type_with_recursion_guard,
};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, IntersectionType, Type, TypeVarBoundOrConstraints,
    UnionType, walk_bound_type_var_type,
};
use crate::{Db, FxIndexMap, FxIndexSet, FxOrderSet};

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

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct OwnedConstraintSet<'db> {
    /// The BDD representing this constraint set
    node: NodeId,
    constraints: IndexVec<ConstraintId, Constraint<'db>>,
    nodes: IndexVec<NodeId, InteriorNodeData>,
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
    builder: &'c ConstraintSetBuilder<'db>,
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

    /// Returns whether this constraint set contains any cycles between typevars. If it does, then
    /// we cannot create a specialization from this constraint set.
    ///
    /// We have restrictions in place that ensure that there are no cycles in the _lower and upper
    /// bounds_ of each constraint, but it's still possible for a constraint to _mention_ another
    /// typevar without _constraining_ it. For instance, `(T ≤ int) ∧ (U ≤ list[T])` is a valid
    /// constraint set, which we can create a specialization from (`T = int, U = list[int]`). But
    /// `(T ≤ list[U]) ∧ (U ≤ list[T])` does not violate our lower/upper bounds restrictions, since
    /// neither bound _is_ a typevar. And it's not something we can create a specialization from,
    /// since we would endlessly substitute until we stack overflow.
    pub(crate) fn is_cyclic(self, db: &'db dyn Db) -> bool {
        #[derive(Default)]
        struct CollectReachability<'db> {
            reachable_typevars: RefCell<FxHashSet<BoundTypeVarIdentity<'db>>>,
            recursion_guard: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for CollectReachability<'db> {
            fn should_visit_lazy_type_attributes(&self) -> bool {
                true
            }

            fn visit_bound_type_var_type(
                &self,
                db: &'db dyn Db,
                bound_typevar: BoundTypeVarInstance<'db>,
            ) {
                self.reachable_typevars
                    .borrow_mut()
                    .insert(bound_typevar.identity(db));
                walk_bound_type_var_type(db, bound_typevar, self);
            }

            fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
                // Override the default `walk_generic_alias` to skip walking the generic
                // context. The generic context contains the typevar *definitions* for the
                // specialization (the mapping keys), but those typevars are bound — they
                // are not free occurrences in the type. Walking them here would cause false
                // cycles: e.g. the constraint `list[int] ≤ _T@list` would appear cyclic
                // because `_T@list` is found in the generic context of `list[int]`, even
                // though `_T` is bound to `int` in that specialization.
                for ty in alias.specialization(db).types(db) {
                    self.visit_type(db, *ty);
                }
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        fn visit_dfs<'db>(
            reachable_typevars: &mut FxHashMap<
                BoundTypeVarIdentity<'db>,
                FxHashSet<BoundTypeVarIdentity<'db>>,
            >,
            discovered: &mut FxHashSet<BoundTypeVarIdentity<'db>>,
            bound_typevar: BoundTypeVarIdentity<'db>,
        ) -> bool {
            discovered.insert(bound_typevar);
            let outgoing = reachable_typevars
                .remove(&bound_typevar)
                .expect("should not visit typevar twice in DFS");
            for outgoing in outgoing {
                if discovered.contains(&outgoing) {
                    return true;
                }
                if reachable_typevars.contains_key(&outgoing) {
                    if visit_dfs(reachable_typevars, discovered, outgoing) {
                        return true;
                    }
                }
            }
            discovered.remove(&bound_typevar);
            false
        }

        // First find all of the typevars that each constraint directly mentions.
        let mut reachable_typevars: FxHashMap<
            BoundTypeVarIdentity<'db>,
            FxHashSet<BoundTypeVarIdentity<'db>>,
        > = FxHashMap::default();
        self.node
            .for_each_constraint(self.builder, &mut |constraint, _| {
                let visitor = CollectReachability::default();
                let constraint = self.builder.constraint_data(constraint);
                visitor.visit_type(db, constraint.lower);
                visitor.visit_type(db, constraint.upper);
                reachable_typevars
                    .entry(constraint.typevar.identity(db))
                    .or_default()
                    .extend(visitor.reachable_typevars.into_inner());
            });

        // Then perform a depth-first search to see if there are any cycles.
        let mut discovered: FxHashSet<BoundTypeVarIdentity<'db>> = FxHashSet::default();
        while let Some(bound_typevar) = reachable_typevars.keys().copied().next() {
            if !discovered.contains(&bound_typevar) {
                let cycle_found =
                    visit_dfs(&mut reachable_typevars, &mut discovered, bound_typevar);
                if cycle_found {
                    return true;
                }
            }
        }

        false
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
        inferable: InferableTypeVars<'_, 'db>,
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

    pub(crate) fn solutions(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
    ) -> Solutions<'db, 'c> {
        self.verify_builder(builder);

        // If the constraint set is cyclic, we'll hit an infinite expansion when trying to add type
        // mappings for it.
        if self.is_cyclic(db) {
            return Solutions::Unsatisfiable;
        }

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

#[derive(Default)]
pub(crate) struct ConstraintSetBuilder<'db> {
    storage: RefCell<ConstraintSetStorage<'db>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, get_size2::GetSize)]
struct ConstraintSetStorage<'db> {
    constraints: IndexVec<ConstraintId, Constraint<'db>>,
    nodes: IndexVec<NodeId, InteriorNodeData>,

    constraint_cache: FxHashMap<Constraint<'db>, ConstraintId>,
    node_cache: FxHashMap<InteriorNodeData, NodeId>,

    negate_cache: FxHashMap<NodeId, NodeId>,
    or_cache: FxHashMap<(NodeId, NodeId, usize), NodeId>,
    and_cache: FxHashMap<(NodeId, NodeId, usize), NodeId>,
    iff_cache: FxHashMap<(NodeId, NodeId, usize), NodeId>,
    exists_one_cache: FxHashMap<(NodeId, BoundTypeVarIdentity<'db>), NodeId>,
    retain_one_cache: FxHashMap<(NodeId, BoundTypeVarIdentity<'db>), NodeId>,
    restrict_one_cache: FxHashMap<(NodeId, ConstraintAssignment), (NodeId, bool)>,
    solutions_cache: FxHashMap<NodeId, Vec<Solution<'db>>>,
    simplify_cache: FxHashMap<NodeId, NodeId>,

    single_sequent_cache: FxHashMap<ConstraintId, SequentMap>,
    pair_sequent_cache: FxHashMap<(ConstraintId, ConstraintId), SequentMap>,
}

impl<'db> ConstraintSetBuilder<'db> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn into_owned(
        self,
        f: impl for<'c> FnOnce(&'c Self) -> ConstraintSet<'db, 'c>,
    ) -> OwnedConstraintSet<'db> {
        let constraint = f(&self);
        let node = constraint.node;
        let storage = self.storage.into_inner();
        OwnedConstraintSet {
            node,
            constraints: storage.constraints,
            nodes: storage.nodes,
        }
    }

    pub(crate) fn load<'c>(&'c self, other: &OwnedConstraintSet<'db>) -> ConstraintSet<'db, 'c> {
        let mut constraints = IndexVec::with_capacity(other.constraints.len());
        for constraint in other.constraints.iter().copied() {
            constraints.push(self.intern_constraint(constraint));
        }

        let mut nodes = IndexVec::with_capacity(other.nodes.len());
        let remap = |old: NodeId, nodes: &IndexVec<NodeId, NodeId>| {
            if old.is_terminal() { old } else { nodes[old] }
        };
        for node in other.nodes.iter().copied() {
            let remapped = InteriorNodeData {
                constraint: constraints[node.constraint],
                if_true: remap(node.if_true, &nodes),
                if_false: remap(node.if_false, &nodes),
                source_order: node.source_order,
                max_source_order: node.max_source_order,
            };
            nodes.push(self.intern_interior_node(remapped));
        }

        ConstraintSet::from_node(self, remap(other.node, &nodes))
    }

    fn intern_constraint(&self, data: Constraint<'db>) -> ConstraintId {
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
    fn can_be_bound_for(self, db: &'db dyn Db, typevar: Self) -> bool {
        self.identity(db) > typevar.identity(db)
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

/// The index of an individual constraint (i.e. a BDD variable) within a [`ConstraintSetStorage`].
#[newtype_index]
#[derive(Ord, PartialOrd, salsa::Update, get_size2::GetSize)]
pub struct ConstraintId;

/// An individual constraint in a constraint set. This restricts a single typevar to be within a
/// lower and upper bound.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct Constraint<'db> {
    pub(crate) typevar: BoundTypeVarInstance<'db>,
    pub(crate) lower: Type<'db>,
    pub(crate) upper: Type<'db>,
}

impl<'db> Constraint<'db> {
    #[expect(clippy::new_ret_no_self)]
    fn new(
        builder: &ConstraintSetBuilder<'db>,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> ConstraintId {
        builder.intern_constraint(Constraint {
            typevar,
            lower,
            upper,
        })
    }

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
                    Constraint::new(builder, typevar, Type::Never, Type::object()),
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

        // If `lower ≰ upper`, then the constraint cannot be satisfied, since there is no type that
        // is both greater than `lower`, and less than `upper`.
        if !lower.is_constraint_set_assignable_to(db, upper) {
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
                let (bound, typevar) = if lower.can_be_bound_for(db, typevar) {
                    (lower, typevar)
                } else {
                    (typevar, lower)
                };
                Node::new_constraint(
                    builder,
                    Constraint::new(builder, typevar, Type::TypeVar(bound), Type::TypeVar(bound)),
                    1,
                )
            }

            // L ≤ T ≤ U == ([L] ≤ T) && (T ≤ [U])
            (Type::TypeVar(lower), Type::TypeVar(upper))
                if typevar.can_be_bound_for(db, lower) && typevar.can_be_bound_for(db, upper) =>
            {
                let lower = Node::new_constraint(
                    builder,
                    Constraint::new(builder, lower, Type::Never, Type::TypeVar(typevar)),
                    1,
                );
                let upper = Node::new_constraint(
                    builder,
                    Constraint::new(builder, upper, Type::TypeVar(typevar), Type::object()),
                    1,
                );
                lower.and(builder, upper)
            }

            // L ≤ T ≤ U == ([L] ≤ T) && ([T] ≤ U)
            (Type::TypeVar(lower), _) if typevar.can_be_bound_for(db, lower) => {
                let lower = Node::new_constraint(
                    builder,
                    Constraint::new(builder, lower, Type::Never, Type::TypeVar(typevar)),
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
            (_, Type::TypeVar(upper)) if typevar.can_be_bound_for(db, upper) => {
                let lower = if lower.is_never() {
                    ALWAYS_TRUE
                } else {
                    Constraint::new_node(db, builder, typevar, lower, Type::object())
                };
                let upper = Node::new_constraint(
                    builder,
                    Constraint::new(builder, upper, Type::TypeVar(typevar), Type::object()),
                    1,
                );
                lower.and(builder, upper)
            }

            _ => Node::new_constraint(builder, Constraint::new(builder, typevar, lower, upper), 1),
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

    /// Defines the ordering of the variables in a constraint set BDD.
    ///
    /// If we only care about _correctness_, we can choose any ordering that we want, as long as
    /// it's consistent. However, different orderings can have very different _performance_
    /// characteristics. Many BDD libraries attempt to reorder variables on the fly while building
    /// and working with BDDs. We don't do that, but we have tried to make some simple choices that
    /// have clear wins.
    ///
    /// In particular, we use the IDs that salsa assigns to each constraint as it is created. This
    /// tends to ensure that constraints that are close to each other in the source are also close
    /// to each other in the BDD structure.
    ///
    /// As an optimization, we also _reverse_ this ordering, so that constraints that appear
    /// earlier in the source appear "lower" (closer to the terminal nodes) in the BDD. Since we
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
        std::cmp::Reverse(self)
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

        // If `lower ≰ upper`, then the intersection is empty, since there is no type that is both
        // greater than `lower`, and less than `upper`.
        if !lower.is_constraint_set_assignable_to(db, upper) {
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
        self.display_inner(db, builder, false)
    }

    fn display_negated<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
    ) -> impl Display {
        self.display_inner(db, builder, true)
    }

    fn display_inner<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        negated: bool,
    ) -> impl Display {
        struct DisplayConstrainedTypeVar<'db> {
            constraint: Constraint<'db>,
            negated: bool,
            db: &'db dyn Db,
        }

        impl Display for DisplayConstrainedTypeVar<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let lower = self.constraint.lower;
                let upper = self.constraint.upper;
                let typevar = self.constraint.typevar;
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
                        return write!(
                            f,
                            "({} {} {})",
                            smaller,
                            if self.negated { "≠" } else { "=" },
                            larger,
                        );
                    }

                    return write!(
                        f,
                        "({} {} {})",
                        typevar.identity(self.db).display(self.db),
                        if self.negated { "≠" } else { "=" },
                        lower.display(self.db)
                    );
                }

                if lower.is_never() && upper.is_object() {
                    return write!(
                        f,
                        "({} {} *)",
                        typevar.identity(self.db).display(self.db),
                        if self.negated { "≠" } else { "=" }
                    );
                }

                if self.negated {
                    f.write_str("¬")?;
                }
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

        DisplayConstrainedTypeVar {
            constraint: builder.constraint_data(self),
            negated,
            db,
        }
    }
}

/// The index of a BDD node within a [`ConstraintSetStorage`].
///
/// The "variables" of a constraint set BDD are individual constraints, represented by an interned
/// [`Constraint`].
///
/// Terminal nodes (`false` and `true`) have hard-coded index values. Interior nodes are stored in
/// a [`ConstraintSetStorage`], and are represented by the index into the storage array. By
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

impl Node {
    /// Creates a new BDD node, ensuring that it is quasi-reduced.
    #[expect(clippy::new_ret_no_self)]
    fn new(
        builder: &ConstraintSetBuilder<'_>,
        constraint: ConstraintId,
        if_true: NodeId,
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
            if_false
                .root_constraint(builder)
                .is_none_or(|root_constraint| {
                    root_constraint.ordering() > constraint.ordering()
                })
        );
        if if_true == ALWAYS_FALSE && if_false == ALWAYS_FALSE {
            return ALWAYS_FALSE;
        }
        let max_source_order = source_order
            .max(if_true.max_source_order(builder))
            .max(if_false.max_source_order(builder));
        builder.intern_interior_node(InteriorNodeData {
            constraint,
            if_true,
            if_false,
            source_order,
            max_source_order,
        })
    }

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
            if_false: ALWAYS_FALSE,
            source_order,
            max_source_order: source_order,
        })
    }

    /// Creates a new BDD node for a positive or negative individual constraint. (For a positive
    /// constraint, this returns the same BDD node as [`new_constraint`][Self::new_constraint]. For
    /// a negative constraint, it returns the negation of that BDD node.)
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
                    if_false: ALWAYS_FALSE,
                    source_order,
                    max_source_order: source_order,
                })
            }
            ConstraintAssignment::Negative(constraint) => {
                builder.intern_interior_node(InteriorNodeData {
                    constraint,
                    if_true: ALWAYS_FALSE,
                    if_false: ALWAYS_TRUE,
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
                Node::new(
                    builder,
                    interior.constraint,
                    interior.if_true.with_adjusted_source_order(builder, delta),
                    interior.if_false.with_adjusted_source_order(builder, delta),
                    interior.source_order + delta,
                )
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
                    |path, _| interior.if_true.for_each_path_inner(db, builder, f, path),
                );
                path.walk_edge(
                    db,
                    builder,
                    interior.constraint.when_false(),
                    interior.source_order,
                    |path, _| interior.if_false.for_each_path_inner(db, builder, f, path),
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
                let interior = builder.interior_node_data(self);
                let true_always_satisfied = path
                    .walk_edge(
                        db,
                        builder,
                        interior.constraint.when_true(),
                        interior.source_order,
                        |path, _| {
                            interior
                                .if_true
                                .is_always_satisfied_inner(db, builder, path)
                        },
                    )
                    .unwrap_or(true);
                if !true_always_satisfied {
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
                            .is_always_satisfied_inner(db, builder, path)
                    },
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

    fn solutions<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
    ) -> Solutions<'db, 'c> {
        match self.node() {
            Node::AlwaysTrue => Solutions::Unconstrained,
            Node::AlwaysFalse => Solutions::Unsatisfiable,
            Node::Interior(interior) => interior.solutions(db, builder),
        }
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
                Node::new(
                    builder,
                    other_interior.constraint,
                    ALWAYS_TRUE,
                    ALWAYS_TRUE,
                    other_interior.source_order + other_offset,
                )
            }
            (Node::Interior(_), Node::AlwaysTrue) => {
                let self_interior = builder.interior_node_data(self);
                Node::new(
                    builder,
                    self_interior.constraint,
                    ALWAYS_TRUE,
                    ALWAYS_TRUE,
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
                Node::new(
                    builder,
                    other_interior.constraint,
                    ALWAYS_FALSE,
                    ALWAYS_FALSE,
                    other_interior.source_order + other_offset,
                )
            }
            (Node::Interior(_), Node::AlwaysFalse) => {
                let self_interior = builder.interior_node_data(self);
                Node::new(
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
        match (self.node(), other.node()) {
            (Node::AlwaysFalse, Node::AlwaysFalse) | (Node::AlwaysTrue, Node::AlwaysTrue) => {
                ALWAYS_TRUE
            }
            (Node::AlwaysTrue, Node::AlwaysFalse) | (Node::AlwaysFalse, Node::AlwaysTrue) => {
                ALWAYS_FALSE
            }
            (Node::AlwaysTrue | Node::AlwaysFalse, Node::Interior(_)) => {
                let interior = builder.interior_node_data(other);
                Node::new(
                    builder,
                    interior.constraint,
                    self.iff_inner(builder, interior.if_true, other_offset),
                    self.iff_inner(builder, interior.if_false, other_offset),
                    interior.source_order + other_offset,
                )
            }
            (Node::Interior(_), Node::AlwaysTrue | Node::AlwaysFalse) => {
                let interior = builder.interior_node_data(self);
                Node::new(
                    builder,
                    interior.constraint,
                    interior.if_true.iff_inner(builder, other, other_offset),
                    interior.if_false.iff_inner(builder, other, other_offset),
                    interior.source_order,
                )
            }
            (Node::Interior(a), Node::Interior(b)) => a.iff(builder, b, other_offset),
        }
    }

    /// Returns the `if-then-else` of three BDDs: when `self` evaluates to `true`, it returns what
    /// `then_node` evaluates to; otherwise it returns what `else_node` evaluates to.
    fn ite(self, builder: &ConstraintSetBuilder<'_>, then_node: Self, else_node: Self) -> Self {
        self.and(builder, then_node)
            .or(builder, self.negate(builder).and(builder, else_node))
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
        inferable: InferableTypeVars<'_, 'db>,
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

    /// Returns a new BDD that is the _existential abstraction_ of `self` for a set of typevars.
    /// All typevars _other_ than the one given will be removed and abstracted away.
    fn retain_one<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarIdentity<'db>,
    ) -> Self {
        match self.node() {
            Node::AlwaysTrue => ALWAYS_TRUE,
            Node::AlwaysFalse => ALWAYS_FALSE,
            Node::Interior(interior) => interior.retain_one(db, builder, bound_typevar),
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

    /// Invokes a callback for each of the representative types of a particular typevar for this
    /// constraint set.
    ///
    /// We first abstract the BDD so that it only mentions constraints on the requested typevar. We
    /// then invoke your callback for each distinct path from the BDD root to the `AlwaysTrue`
    /// terminal. Each of those paths can be viewed as the conjunction of the individual
    /// constraints of each internal node that we traverse as we walk that path. We provide the
    /// lower/upper bound of this conjunction to your callback, allowing you to choose any suitable
    /// type in the range.
    ///
    /// If the abstracted BDD does not mention the typevar at all (i.e., it leaves the typevar
    /// completely unconstrained), we will invoke your callback once with `None`.
    fn find_representative_types<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarIdentity<'db>,
        mut f: impl FnMut(Option<&[RepresentativeBounds<'db>]>),
    ) {
        self.retain_one(db, builder, bound_typevar)
            .find_representative_types_inner(db, builder, &mut Vec::default(), &mut f);
    }

    fn find_representative_types_inner<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        current_bounds: &mut Vec<RepresentativeBounds<'db>>,
        f: &mut dyn FnMut(Option<&[RepresentativeBounds<'db>]>),
    ) {
        match self.node() {
            Node::AlwaysTrue => {
                // If we reach the `true` terminal, the path we've been following represents one
                // representative type.
                if current_bounds.is_empty() {
                    f(None);
                    return;
                }

                // If `lower ≰ upper`, then this path somehow represents in invalid specialization.
                // That should have been removed from the BDD domain as part of the simplification
                // process. (Here we are just checking assignability, so we don't need to construct
                // the lower and upper bounds in a consistent order.)
                debug_assert!({
                    let greatest_lower_bound = UnionType::from_elements(
                        db,
                        current_bounds.iter().map(|bounds| bounds.lower),
                    );
                    let least_upper_bound = IntersectionType::from_elements(
                        db,
                        current_bounds.iter().map(|bounds| bounds.upper),
                    );
                    greatest_lower_bound.is_constraint_set_assignable_to(db, least_upper_bound)
                });

                // We've been tracking the lower and upper bound that the types for this path must
                // satisfy. Pass those bounds along and let the caller choose a representative type
                // from within that range.
                f(Some(current_bounds));
            }

            Node::AlwaysFalse => {
                // If we reach the `false` terminal, the path we've been following represents an
                // invalid specialization, so we skip it.
            }

            Node::Interior(_) => {
                let interior = builder.interior_node_data(self);
                let reset_point = current_bounds.len();

                // For an interior node, there are two outgoing paths: one for the `if_true`
                // branch, and one for the `if_false` branch.
                //
                // For the `if_true` branch, this node's constraint places additional restrictions
                // on the types that satisfy the current path through the BDD. So we intersect the
                // current glb/lub with the constraint's bounds to get the new glb/lub for the
                // recursive call.
                current_bounds.push(RepresentativeBounds::from_interior_node(builder, interior));
                interior
                    .if_true
                    .find_representative_types_inner(db, builder, current_bounds, f);
                current_bounds.truncate(reset_point);

                // For the `if_false` branch, then the types that satisfy the current path through
                // the BDD do _not_ satisfy the node's constraint. Because we used `retain_one` to
                // abstract the BDD to a single typevar, we don't need to worry about how that
                // negative constraint affects the lower/upper bound that we're tracking. The
                // abstraction process will have compared the negative constraint with all of the
                // other constraints in the BDD, and added new interior nodes to handle the
                // combination of those constraints. So we can recurse down the `if_false` branch
                // without updating the lower/upper bounds, relying on the other constraints along
                // the path to incorporate that negative "hole" in the set of valid types for this
                // path.
                interior
                    .if_false
                    .find_representative_types_inner(db, builder, current_bounds, f);
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
                    write!(f, "\n{prefix}┡━₁ ",)?;
                    format_node(
                        db,
                        builder,
                        interior.if_true,
                        &format_args!("{prefix}│   ",),
                        seen,
                        f,
                    )?;
                    write!(f, "\n{prefix}└─₀ ",)?;
                    format_node(
                        db,
                        builder,
                        interior.if_false,
                        &format_args!("{prefix}    ",),
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

#[derive(Clone, Copy, Debug)]
struct RepresentativeBounds<'db> {
    lower: Type<'db>,
    upper: Type<'db>,
    source_order: usize,
}

impl<'db> RepresentativeBounds<'db> {
    fn from_interior_node(builder: &ConstraintSetBuilder<'db>, interior: InteriorNodeData) -> Self {
        let constraint = builder.constraint_data(interior.constraint);
        Self {
            lower: constraint.lower,
            upper: constraint.upper,
            source_order: interior.source_order,
        }
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

        let interior = builder.interior_node_data(self.node());
        let result = Node::new(
            builder,
            interior.constraint,
            interior.if_true.negate(builder),
            interior.if_false.negate(builder),
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
            Ordering::Equal => Node::new(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .or_inner(builder, other_interior.if_true, other_offset),
                self_interior
                    .if_false
                    .or_inner(builder, other_interior.if_false, other_offset),
                self_interior.source_order,
            ),
            Ordering::Less => Node::new(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .or_inner(builder, other.node(), other_offset),
                self_interior
                    .if_false
                    .or_inner(builder, other.node(), other_offset),
                self_interior.source_order,
            ),
            Ordering::Greater => Node::new(
                builder,
                other_interior.constraint,
                self.node()
                    .or_inner(builder, other_interior.if_true, other_offset),
                self.node()
                    .or_inner(builder, other_interior.if_false, other_offset),
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
            Ordering::Equal => Node::new(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .and_inner(builder, other_interior.if_true, other_offset),
                self_interior
                    .if_false
                    .and_inner(builder, other_interior.if_false, other_offset),
                self_interior.source_order,
            ),
            Ordering::Less => Node::new(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .and_inner(builder, other.node(), other_offset),
                self_interior
                    .if_false
                    .and_inner(builder, other.node(), other_offset),
                self_interior.source_order,
            ),
            Ordering::Greater => Node::new(
                builder,
                other_interior.constraint,
                self.node()
                    .and_inner(builder, other_interior.if_true, other_offset),
                self.node()
                    .and_inner(builder, other_interior.if_false, other_offset),
                other_interior.source_order + other_offset,
            ),
        };

        let mut storage = builder.storage.borrow_mut();
        storage.and_cache.insert(key, result);
        result
    }

    fn iff(self, builder: &ConstraintSetBuilder<'_>, other: Self, other_offset: usize) -> NodeId {
        let key = (self.node(), other.node(), other_offset);
        let storage = builder.storage.borrow();
        if let Some(result) = storage.iff_cache.get(&key) {
            return *result;
        }
        drop(storage);

        let self_interior = builder.interior_node_data(self.node());
        let self_ordering = self_interior.constraint.ordering();
        let other_interior = builder.interior_node_data(other.node());
        let other_ordering = other_interior.constraint.ordering();
        let result = match self_ordering.cmp(&other_ordering) {
            Ordering::Equal => Node::new(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .iff_inner(builder, other_interior.if_true, other_offset),
                self_interior
                    .if_false
                    .iff_inner(builder, other_interior.if_false, other_offset),
                self_interior.source_order,
            ),
            Ordering::Less => Node::new(
                builder,
                self_interior.constraint,
                self_interior
                    .if_true
                    .iff_inner(builder, other.node(), other_offset),
                self_interior
                    .if_false
                    .iff_inner(builder, other.node(), other_offset),
                self_interior.source_order,
            ),
            Ordering::Greater => Node::new(
                builder,
                other_interior.constraint,
                self.node()
                    .iff_inner(builder, other_interior.if_true, other_offset),
                self.node()
                    .iff_inner(builder, other_interior.if_false, other_offset),
                other_interior.source_order + other_offset,
            ),
        };

        let mut storage = builder.storage.borrow_mut();
        storage.iff_cache.insert(key, result);
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
            // that mentions `bound_typevar`.
            // TODO: This will currently remove constraints that mention a typevar, but the sequent
            // map is not yet propagating all derived facts about those constraints. For instance,
            // removing `T` from `T ≤ int ∧ U ≤ Sequence[T]` should produce `U ≤ Sequence[int]`.
            // But that requires `T ≤ int ∧ U ≤ Sequence[T] → U ≤ Sequence[int]` to exist in the
            // sequent map. It doesn't, and so we currently produce `U ≤ Unknown` in this case.
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

    fn retain_one<'db>(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        bound_typevar: BoundTypeVarIdentity<'db>,
    ) -> NodeId {
        let key = (self.node(), bound_typevar);
        let storage = builder.storage.borrow();
        if let Some(result) = storage.retain_one_cache.get(&key) {
            return *result;
        }
        drop(storage);

        let mut path = self.path_assignments(builder);
        let result = self.abstract_one_inner(
            db,
            builder,
            // Remove any node that constrains some other typevar than `bound_typevar`, and any
            // node that constrains `bound_typevar` with a lower/upper bound of some other typevar.
            // (For the latter, if there are any derived facts that we can infer from the typevar
            // bound, those will be automatically added to the result.)
            &mut |constraint| {
                let constraint = builder.constraint_data(constraint);
                if constraint.typevar.identity(db) != bound_typevar {
                    return true;
                }
                if constraint.lower.has_typevar(db) || constraint.upper.has_typevar(db) {
                    return true;
                }
                false
            },
            &mut path,
        );

        let mut storage = builder.storage.borrow_mut();
        storage.retain_one_cache.insert(key, result);
        result
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
            // with the OR of its if_false/if_true edges. That is, the result is true if there's
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
            if_true.or(builder, if_false)
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
            // NB: We cannot use `Node::new` here, because the recursive calls might introduce new
            // derived constraints into the result, and those constraints might appear before this
            // one in the BDD ordering.
            Node::new_constraint(
                builder,
                self_interior.constraint,
                self_interior.source_order,
            )
            .ite(builder, if_true, if_false)
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
            // variable by replacing this node with its if_false/if_true edge, accordingly.
            if assignment == self_interior.constraint.when_true() {
                (self_interior.if_true, true)
            } else if assignment == self_interior.constraint.when_false() {
                (self_interior.if_false, true)
            } else {
                let (if_true, found_in_true) =
                    self_interior.if_true.restrict_one(db, builder, assignment);
                let (if_false, found_in_false) =
                    self_interior.if_false.restrict_one(db, builder, assignment);
                (
                    Node::new(
                        builder,
                        self_interior.constraint,
                        if_true,
                        if_false,
                        self_interior.source_order,
                    ),
                    found_in_true || found_in_false,
                )
            }
        };

        let mut storage = builder.storage.borrow_mut();
        storage.restrict_one_cache.insert(key, result);
        result
    }

    fn solutions<'db, 'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
    ) -> Solutions<'db, 'c> {
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

        fn solutions_inner<'db, 'c>(
            db: &'db dyn Db,
            builder: &'c ConstraintSetBuilder<'db>,
            interior: NodeId,
        ) -> Ref<'c, Vec<Solution<'db>>> {
            let key = interior;
            let storage = builder.storage.borrow();
            if let Ok(solutions) =
                Ref::filter_map(storage, |storage| storage.solutions_cache.get(&key))
            {
                return solutions;
            }

            // Sort the constraints in each path by their `source_order`s, to ensure that we construct
            // any unions or intersections in our type mappings in a stable order. Constraints might
            // come out of `PathAssignment`s with identical `source_order`s, but if they do, those
            // "tied" constraints will still be ordered in a stable way. So we need a stable sort to
            // retain that stable per-tie ordering.
            let mut sorted_paths = Vec::new();
            interior.for_each_path(db, builder, |path| {
                let mut path: Vec<_> = path.positive_constraints().collect();
                path.sort_by_key(|(_, source_order)| *source_order);
                sorted_paths.push(path);
            });
            sorted_paths.sort_by(|path1, path2| {
                let source_orders1 = path1.iter().map(|(_, source_order)| *source_order);
                let source_orders2 = path2.iter().map(|(_, source_order)| *source_order);
                source_orders1.cmp(source_orders2)
            });

            let mut solutions = Vec::with_capacity(sorted_paths.len());
            let mut mappings: FxHashMap<BoundTypeVarInstance<'db>, Bounds<'db>> =
                FxHashMap::default();
            'paths: for path in sorted_paths {
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

                let mut solution = Vec::with_capacity(mappings.len());
                for (bound_typevar, bounds) in mappings.drain() {
                    match bound_typevar.typevar(db).require_bound_or_constraints(db) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            let bound = bound.top_materialization(db);
                            let lower = UnionType::from_elements(db, bounds.lower);
                            if !lower.is_assignable_to(db, bound) {
                                // This path does not satisfy the typevar's upper bound, and is
                                // therefore not a valid specialization.
                                continue 'paths;
                            }

                            // Prefer the lower bound (often the concrete actual type seen) over the
                            // upper bound (which may include TypeVar bounds/constraints). The upper bound
                            // should only be used as a fallback when no concrete type was inferred.
                            if !lower.is_never() {
                                solution.push(TypeVarSolution {
                                    bound_typevar,
                                    solution: lower,
                                });
                                continue;
                            }

                            let upper = IntersectionType::from_elements(
                                db,
                                std::iter::chain(bounds.upper, [bound]),
                            );
                            if upper != bound {
                                solution.push(TypeVarSolution {
                                    bound_typevar,
                                    solution: upper,
                                });
                            }
                        }

                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            // Filter out the typevar constraints that aren't satisfied by this path.
                            let lower = UnionType::from_elements(db, bounds.lower);
                            let upper = IntersectionType::from_elements(db, bounds.upper);
                            let compatible_constraints =
                                constraints.elements(db).iter().filter(|constraint| {
                                    let constraint_lower = constraint.bottom_materialization(db);
                                    let constraint_upper = constraint.top_materialization(db);
                                    lower.is_assignable_to(db, constraint_lower)
                                        && constraint_upper.is_assignable_to(db, upper)
                                });

                            // If only one constraint remains, that's our specialization for this path.
                            match compatible_constraints.at_most_one() {
                                Ok(None) => {
                                    // This path does not satisfy any of the constraints, and is
                                    // therefore not a valid specialization.
                                    continue 'paths;
                                }

                                Ok(Some(compatible_constraint)) => {
                                    solution.push(TypeVarSolution {
                                        bound_typevar,
                                        solution: *compatible_constraint,
                                    });
                                }

                                Err(_) => {
                                    // This path satisfies multiple constraints. For now, don't
                                    // prefer any of them, and fall back on the default
                                    // specialization for this typevar.
                                }
                            }
                        }
                    }
                }

                solutions.push(solution);
            }

            let mut storage = builder.storage.borrow_mut();
            storage.solutions_cache.insert(key, solutions);
            drop(storage);

            let storage = builder.storage.borrow();
            Ref::map(storage, |storage| &storage.solutions_cache[&key])
        }

        let solutions = solutions_inner(db, builder, self.node());
        if solutions.is_empty() {
            return Solutions::Unsatisfiable;
        }
        Solutions::Constrained(solutions)
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
                    if left_typevar.can_be_bound_for(db, right_typevar) {
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
                    Constraint::new(builder, constrained_typevar, new_lower, new_upper);
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
                        builder.intern_constraint(intersection_constraint_data);

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

#[derive(Debug)]
pub(crate) enum Solutions<'db, 'c> {
    Unsatisfiable,
    Unconstrained,
    Constrained(Ref<'c, Vec<Solution<'db>>>),
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
}

impl ConstraintAssignment {
    fn constraint(self) -> ConstraintId {
        match self {
            ConstraintAssignment::Positive(constraint) => constraint,
            ConstraintAssignment::Negative(constraint) => constraint,
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
        }
    }

    fn display<'db>(self, db: &'db dyn Db, builder: &ConstraintSetBuilder<'db>) -> impl Display {
        struct DisplayConstraintAssignment<'db, 'c> {
            constraint: ConstraintAssignment,
            db: &'db dyn Db,
            builder: &'c ConstraintSetBuilder<'db>,
        }

        impl Display for DisplayConstraintAssignment<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.constraint {
                    ConstraintAssignment::Positive(constraint) => {
                        constraint.display(self.db, self.builder).fmt(f)
                    }
                    ConstraintAssignment::Negative(constraint) => {
                        constraint.display_negated(self.db, self.builder).fmt(f)
                    }
                }
            }
        }

        DisplayConstraintAssignment {
            constraint: self,
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
    pair_implications: FxHashMap<(ConstraintId, ConstraintId), FxOrderSet<ConstraintId>>,
    /// Sequents of the form `C → D`
    single_implications: FxHashMap<ConstraintId, FxOrderSet<ConstraintId>>,
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
        // If either antecedent implies the consequent on its own, this new sequent is redundant.
        if ante1.implies(db, builder, post) || ante2.implies(db, builder, post) {
            return;
        }
        if self
            .pair_implications
            .entry(Self::pair_key(ante1, ante2))
            .or_default()
            .insert(post)
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

        // If the lower or upper bound of this constraint is a typevar, we can propagate the
        // constraint:
        //
        //   1. `(S ≤ T ≤ U) → (S ≤ U)`
        //   2. `(S ≤ T ≤ τ) → (S ≤ τ)`
        //   3. `(τ ≤ T ≤ U) → (τ ≤ U)`
        //
        // Technically, (1) also allows `(S = T) → (S = S)`, but the rhs of that is vacuously true,
        // so we don't add a sequent for that case.

        let post_constraint = match (lower, upper) {
            // Case 1
            (Type::TypeVar(lower_typevar), Type::TypeVar(upper_typevar)) => {
                if !lower_typevar.is_same_typevar_as(db, upper_typevar) {
                    Constraint::new(builder, lower_typevar, Type::Never, upper)
                } else {
                    return;
                }
            }

            // Case 2
            (Type::TypeVar(lower_typevar), _) => {
                Constraint::new(builder, lower_typevar, Type::Never, upper)
            }

            // Case 3
            (_, Type::TypeVar(upper_typevar)) => {
                Constraint::new(builder, upper_typevar, lower, Type::object())
            }

            _ => return,
        };

        self.add_single_implication(db, builder, constraint, post_constraint);
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
            if left_typevar.can_be_bound_for(db, right_typevar) {
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

        let post_constraint = Constraint::new(builder, constrained_typevar, new_lower, new_upper);
        self.add_pair_implication(
            db,
            builder,
            left_constraint,
            right_constraint,
            post_constraint,
        );
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
                let new_constraint = |bound_typevar: BoundTypeVarInstance<'db>,
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
                    Constraint::new(builder, bound_typevar, right_lower, right_upper)
                };
                let post_constraint = match (left_lower, left_upper) {
                    (Type::TypeVar(bound_typevar), Type::TypeVar(other_bound_typevar))
                        if bound_typevar.is_same_typevar_as(db, other_bound_typevar) =>
                    {
                        new_constraint(bound_typevar, right_lower, right_upper)
                    }
                    (Type::TypeVar(bound_typevar), _) => {
                        new_constraint(bound_typevar, Type::Never, right_upper)
                    }
                    (_, Type::TypeVar(bound_typevar)) => {
                        new_constraint(bound_typevar, right_lower, Type::object())
                    }
                    _ => return,
                };
                self.add_pair_implication(
                    db,
                    builder,
                    left_constraint,
                    right_constraint,
                    post_constraint,
                );
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
                    builder.intern_constraint(intersection_constraint_data);
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
                            post.display(self.db, self.builder),
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
        let found_conflict = self.add_assignment(db, builder, assignment, source_order);
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
        result
    }

    pub(crate) fn positive_constraints(&self) -> impl Iterator<Item = (ConstraintId, usize)> + '_ {
        self.assignments
            .iter()
            .filter_map(|(assignment, source_order)| match assignment {
                ConstraintAssignment::Positive(constraint) => Some((*constraint, *source_order)),
                ConstraintAssignment::Negative(_) => None,
            })
    }

    fn assignment_holds(&self, assignment: ConstraintAssignment) -> bool {
        self.assignments.contains_key(&assignment)
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
    ) -> Result<(), PathAssignmentConflict> {
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
            Entry::Occupied(_) => return Ok(()),
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
            for post in posts {
                if self.assignment_holds(ante1.when_true())
                    && self.assignment_holds(ante2.when_true())
                {
                    new_constraints.push(*post);
                }
            }
        }

        for (ante, posts) in &self.map.single_implications {
            for post in posts {
                if self.assignment_holds(ante.when_true()) {
                    new_constraints.push(*post);
                }
            }
        }

        for new_constraint in new_constraints {
            self.add_assignment(db, builder, new_constraint.when_true(), source_order)?;
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
            .map(|constraint| match constraint {
                ConstraintAssignment::Positive(constraint) => {
                    constraint.display(db, builder).to_string()
                }
                ConstraintAssignment::Negative(constraint) => {
                    constraint.display_negated(db, builder).to_string()
                }
            })
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

impl<'db> GenericContext<'db> {
    pub(crate) fn specialize_constrained<'c>(
        self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<Specialization<'db>, ()> {
        tracing::trace!(
            target: "ty_python_semantic::types::constraints::specialize_constrained",
            generic_context = %self.display_full(db),
            constraints = %constraints.node.display(db, builder),
            "create specialization for constraint set",
        );

        // If the constraint set is cyclic, don't even try to construct a specialization.
        if constraints.is_cyclic(db) {
            tracing::error!(
                target: "ty_python_semantic::types::constraints::specialize_constrained",
                constraints = %constraints.node.display(db, builder),
                "constraint set is cyclic",
            );
            // TODO: Better error
            return Err(());
        }

        // First we intersect with the valid specializations of all of the typevars. We need all of
        // valid specializations to hold simultaneously, so we do this once before abstracting over
        // each typevar.
        let abstracted = self
            .variables(db)
            .fold(ALWAYS_TRUE, |constraints, bound_typevar| {
                constraints
                    .and_with_offset(builder, bound_typevar.valid_specializations(db, builder))
            })
            .and_with_offset(builder, constraints.node);
        tracing::trace!(
            target: "ty_python_semantic::types::constraints::specialize_constrained",
            valid = %abstracted.display(db, builder),
            "limited to valid specializations",
        );

        // Then we find all of the "representative types" for each typevar in the constraint set.
        let mut error_occurred = false;
        let mut representatives = Vec::new();
        let types = self.variables(db).map(|bound_typevar| {
            // Each representative type represents one of the ways that the typevar can satisfy the
            // constraint, expressed as a lower/upper bound on the types that the typevar can
            // specialize to.
            //
            // If there are multiple paths in the BDD, they technically represent independent
            // possible specializations. If there's a type that satisfies all of them, we will
            // return that as the specialization. If not, then the constraint set is ambiguous.
            // (This happens most often with constrained typevars.) We could in the future turn
            // _each_ of the paths into separate specializations, but it's not clear what we would
            // do with that, so instead we just report the ambiguity as a specialization failure.
            let mut unconstrained = false;
            let identity = bound_typevar.identity(db);
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::specialize_constrained",
                bound_typevar = %identity.display(db),
                abstracted = %abstracted.retain_one(db, builder, identity).display(db, builder),
                "find specialization for typevar",
            );
            representatives.clear();
            abstracted.find_representative_types(db, builder, identity, |representative| {
                match representative {
                    Some(representative) => {
                        representatives.extend_from_slice(representative);
                    }
                    None => {
                        unconstrained = true;
                    }
                }
            });

            // The BDD is satisfiable, but the typevar is unconstrained, then we use `None` to tell
            // specialize_recursive to fall back on the typevar's default.
            if unconstrained {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::specialize_constrained",
                    bound_typevar = %identity.display(db),
                    "typevar is unconstrained",
                );
                return None;
            }

            // If there are no satisfiable paths in the BDD, then there is no valid specialization
            // for this constraint set.
            if representatives.is_empty() {
                // TODO: Construct a useful error here
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::specialize_constrained",
                    bound_typevar = %identity.display(db),
                    "typevar cannot be satisfied",
                );
                error_occurred = true;
                return None;
            }

            // Before constructing the final lower and upper bound, sort the constraints by
            // their source order. This should give us a consistently ordered specialization,
            // regardless of the variable ordering of the original BDD.
            representatives.sort_unstable_by_key(|bounds| bounds.source_order);
            let greatest_lower_bound =
                UnionType::from_elements(db, representatives.iter().map(|bounds| bounds.lower));
            let least_upper_bound = IntersectionType::from_elements(
                db,
                representatives.iter().map(|bounds| bounds.upper),
            );

            // If `lower ≰ upper`, then there is no type that satisfies all of the paths in the
            // BDD. That's an ambiguous specialization, as described above.
            if !greatest_lower_bound.is_constraint_set_assignable_to(db, least_upper_bound) {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::specialize_constrained",
                    bound_typevar = %identity.display(db),
                    greatest_lower_bound = %greatest_lower_bound.display(db),
                    least_upper_bound = %least_upper_bound.display(db),
                    "typevar bounds are incompatible",
                );
                error_occurred = true;
                return None;
            }

            // Of all of the types that satisfy all of the paths in the BDD, we choose the
            // "largest" one (i.e., "closest to `object`") as the specialization.
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::specialize_constrained",
                bound_typevar = %identity.display(db),
                specialization = %least_upper_bound.display(db),
                "found specialization for typevar",
            );
            Some(least_upper_bound)
        });

        let specialization = self.specialize_recursive(db, types);
        if error_occurred {
            return Err(());
        }
        Ok(specialization)
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

    #[test]
    fn test_display_graph_output() {
        let expected = indoc! {r#"
            <0> (U = bool) 2/4
            ┡━₁ <1> (U = str) 1/4
            │   ┡━₁ <2> (T = bool) 4/4
            │   │   ┡━₁ <3> (T = str) 3/3
            │   │   │   ┡━₁ always
            │   │   │   └─₀ always
            │   │   └─₀ <4> (T = str) 3/3
            │   │       ┡━₁ always
            │   │       └─₀ never
            │   └─₀ <2> SHARED
            └─₀ <5> (U = str) 1/4
                ┡━₁ <2> SHARED
                └─₀ never
        "#}
        .trim_end();

        let db = setup_db();
        let t =
            BoundTypeVarInstance::synthetic(&db, Name::new_static("T"), TypeVarVariance::Invariant);
        let u =
            BoundTypeVarInstance::synthetic(&db, Name::new_static("U"), TypeVarVariance::Invariant);
        let bool_type = KnownClass::Bool.to_instance(&db);
        let str_type = KnownClass::Str.to_instance(&db);
        let constraints = ConstraintSetBuilder::new();
        let t_str = ConstraintSet::constrain_typevar(&db, &constraints, t, str_type, str_type);
        let t_bool = ConstraintSet::constrain_typevar(&db, &constraints, t, bool_type, bool_type);
        let u_str = ConstraintSet::constrain_typevar(&db, &constraints, u, str_type, str_type);
        let u_bool = ConstraintSet::constrain_typevar(&db, &constraints, u, bool_type, bool_type);
        // Construct this in a different order than above to make the source_orders more
        // interesting.
        let set = (u_str.or(&db, &constraints, || u_bool))
            .and(&db, &constraints, || t_str.or(&db, &constraints, || t_bool));
        let actual = set.node.display_graph(&db, &constraints, &"").to_string();
        assert_eq!(actual, expected);
    }
}
