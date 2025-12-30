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

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt::Display;
use std::ops::Range;

use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use salsa::plumbing::AsId;

use crate::types::generics::{GenericContext, InferableTypeVars, Specialization};
use crate::types::visitor::{
    TypeCollector, TypeVisitor, any_over_type, walk_type_with_recursion_guard,
};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, IntersectionType, Type, TypeVarBoundOrConstraints,
    UnionType, walk_bound_type_var_type,
};
use crate::{Db, FxOrderMap, FxOrderSet};

/// An extension trait for building constraint sets from [`Option`] values.
pub(crate) trait OptionConstraintsExtension<T> {
    /// Returns a constraint set that is always satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_none_or<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db>;

    /// Returns a constraint set that is never satisfiable if the option is `None`; otherwise
    /// applies a function to determine under what constraints the value inside of it holds.
    fn when_some_and<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db>;
}

impl<T> OptionConstraintsExtension<T> for Option<T> {
    fn when_none_or<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::always(),
        }
    }

    fn when_some_and<'db>(self, f: impl FnOnce(T) -> ConstraintSet<'db>) -> ConstraintSet<'db> {
        match self {
            Some(value) => f(value),
            None => ConstraintSet::never(),
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
    fn when_any<'db>(
        self,
        db: &'db dyn Db,
        f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db>;

    /// Returns the constraints under which every element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_never_satisfied`][ConstraintSet::is_never_satisfied], then the overall result
    /// must be as well, and we stop consuming elements from the iterator.
    fn when_all<'db>(
        self,
        db: &'db dyn Db,
        f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db>;
}

impl<I, T> IteratorConstraintsExtension<T> for I
where
    I: Iterator<Item = T>,
{
    fn when_any<'db>(
        self,
        db: &'db dyn Db,
        mut f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::never();
        for child in self {
            if result.union(db, f(child)).is_always_satisfied(db) {
                return result;
            }
        }
        result
    }

    fn when_all<'db>(
        self,
        db: &'db dyn Db,
        mut f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::always();
        for child in self {
            if result.intersect(db, f(child)).is_never_satisfied(db) {
                return result;
            }
        }
        result
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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct ConstraintSet<'db> {
    /// The BDD representing this constraint set
    node: Node<'db>,
}

impl<'db> ConstraintSet<'db> {
    fn never() -> Self {
        Self {
            node: Node::AlwaysFalse,
        }
    }

    fn always() -> Self {
        Self {
            node: Node::AlwaysTrue,
        }
    }

    /// Returns a constraint set that constraints a typevar to a particular range of types.
    pub(crate) fn constrain_typevar(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        lower: Type<'db>,
        upper: Type<'db>,
    ) -> Self {
        Self {
            node: ConstrainedTypeVar::new_node(db, typevar, lower, upper),
        }
    }

    /// Returns whether this constraint set never holds
    pub(crate) fn is_never_satisfied(self, db: &'db dyn Db) -> bool {
        self.node.is_never_satisfied(db)
    }

    /// Returns whether this constraint set always holds
    pub(crate) fn is_always_satisfied(self, db: &'db dyn Db) -> bool {
        self.node.is_always_satisfied(db)
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
        self.node.for_each_constraint(db, &mut |constraint, _| {
            let visitor = CollectReachability::default();
            visitor.visit_type(db, constraint.lower(db));
            visitor.visit_type(db, constraint.upper(db));
            reachable_typevars
                .entry(constraint.typevar(db).identity(db))
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
        lhs: Type<'db>,
        rhs: Type<'db>,
    ) -> Self {
        Self {
            node: self.node.implies_subtype_of(db, lhs, rhs),
        }
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
        self,
        db: &'db dyn Db,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> bool {
        self.node.satisfied_by_all_typevars(db, inferable)
    }

    pub(crate) fn limit_to_valid_specializations(self, db: &'db dyn Db) -> Self {
        let mut result = self.node;
        let mut seen = FxHashSet::default();
        self.node.for_each_constraint(db, &mut |constraint, _| {
            let bound_typevar = constraint.typevar(db);
            if seen.insert(bound_typevar) {
                result = result.and_with_offset(db, bound_typevar.valid_specializations(db));
            }
        });
        Self { node: result }
    }

    /// Updates this constraint set to hold the union of itself and another constraint set.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn union(&mut self, db: &'db dyn Db, other: Self) -> Self {
        self.node = self.node.or_with_offset(db, other.node);
        *self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn intersect(&mut self, db: &'db dyn Db, other: Self) -> Self {
        self.node = self.node.and_with_offset(db, other.node);
        *self
    }

    /// Returns the negation of this constraint set.
    pub(crate) fn negate(self, db: &'db dyn Db) -> Self {
        Self {
            node: self.node.negate(db),
        }
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn and(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_never_satisfied(db) {
            self.intersect(db, other());
        }
        self
    }

    /// Returns the union of this constraint set and another. The other constraint set is provided
    /// as a thunk, to implement short-circuiting: the thunk is not forced if the constraint set is
    /// already saturated.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn or(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_always_satisfied(db) {
            self.union(db, other());
        }
        self
    }

    /// Returns a constraint set encoding that this constraint set implies another.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn implies(self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        self.negate(db).or(db, other)
    }

    /// Returns a constraint set encoding that this constraint set is equivalent to another.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    pub(crate) fn iff(self, db: &'db dyn Db, other: Self) -> Self {
        ConstraintSet {
            node: self.node.iff_with_offset(db, other.node),
        }
    }

    /// Reduces the set of inferable typevars for this constraint set. You provide an iterator of
    /// the typevars that were inferable when this constraint set was created, and which should be
    /// abstracted away. Those typevars will be removed from the constraint set, and the constraint
    /// set will return true whenever there was _any_ specialization of those typevars that
    /// returned true before.
    pub(crate) fn reduce_inferable(
        self,
        db: &'db dyn Db,
        to_remove: impl IntoIterator<Item = BoundTypeVarIdentity<'db>>,
    ) -> Self {
        let node = self.node.exists(db, to_remove);
        Self { node }
    }

    pub(crate) fn for_each_path(self, db: &'db dyn Db, f: impl FnMut(&PathAssignments<'db>)) {
        self.node.for_each_path(db, f);
    }

    pub(crate) fn range(
        db: &'db dyn Db,
        lower: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        Self::constrain_typevar(db, typevar, lower, upper)
    }

    #[expect(dead_code)] // Keep this around for debugging purposes
    pub(crate) fn display(self, db: &'db dyn Db) -> impl Display {
        self.node.simplify_for_display(db).display(db)
    }

    #[expect(dead_code)] // Keep this around for debugging purposes
    pub(crate) fn display_graph(self, db: &'db dyn Db, prefix: &dyn Display) -> impl Display {
        self.node.display_graph(db, prefix)
    }
}

impl From<bool> for ConstraintSet<'_> {
    fn from(b: bool) -> Self {
        if b { Self::always() } else { Self::never() }
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
    Simplified(ConstrainedTypeVar<'db>),
    CannotSimplify,
    Disjoint,
}

impl IntersectionResult<'_> {
    fn is_disjoint(self) -> bool {
        matches!(self, IntersectionResult::Disjoint)
    }
}

/// An individual constraint in a constraint set. This restricts a single typevar to be within a
/// lower and upper bound.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct ConstrainedTypeVar<'db> {
    pub(crate) typevar: BoundTypeVarInstance<'db>,
    pub(crate) lower: Type<'db>,
    pub(crate) upper: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ConstrainedTypeVar<'_> {}

#[salsa::tracked]
impl<'db> ConstrainedTypeVar<'db> {
    /// Returns a new range constraint.
    ///
    /// Panics if `lower` and `upper` are not both fully static.
    fn new_node(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        mut lower: Type<'db>,
        mut upper: Type<'db>,
    ) -> Node<'db> {
        // It's not useful for an upper bound to be an intersection type, or for a lower bound to
        // be a union type. Because the following equivalences hold, we can break these bounds
        // apart and create an equivalent BDD with more nodes but simpler constraints. (Fewer,
        // simpler constraints mean that our sequent maps won't grow pathologically large.)
        //
        //   T ≤ (α & β)   ⇔ (T ≤ α) ∧ (T ≤ β)
        //   T ≤ (¬α & ¬β) ⇔ (T ≤ ¬α) ∧ (T ≤ ¬β)
        //   (α | β) ≤ T   ⇔ (α ≤ T) ∧ (β ≤ T)
        if let Type::Union(lower_union) = lower {
            let mut result = Node::AlwaysTrue;
            for lower_element in lower_union.elements(db) {
                result = result.and_with_offset(
                    db,
                    ConstrainedTypeVar::new_node(db, typevar, *lower_element, upper),
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
            let mut result = Node::AlwaysTrue;
            for upper_element in upper_intersection.iter_positive(db) {
                result = result.and_with_offset(
                    db,
                    ConstrainedTypeVar::new_node(db, typevar, lower, upper_element),
                );
            }
            for upper_element in upper_intersection.iter_negative(db) {
                result = result.and_with_offset(
                    db,
                    ConstrainedTypeVar::new_node(db, typevar, lower, upper_element.negate(db)),
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
                    db,
                    ConstrainedTypeVar::new(db, typevar, Type::Never, Type::object()),
                    1,
                )
                .negate(db);
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
            return Node::AlwaysFalse;
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
                    db,
                    ConstrainedTypeVar::new(
                        db,
                        typevar,
                        Type::TypeVar(bound),
                        Type::TypeVar(bound),
                    ),
                    1,
                )
            }

            // L ≤ T ≤ U == ([L] ≤ T) && (T ≤ [U])
            (Type::TypeVar(lower), Type::TypeVar(upper))
                if typevar.can_be_bound_for(db, lower) && typevar.can_be_bound_for(db, upper) =>
            {
                let lower = Node::new_constraint(
                    db,
                    ConstrainedTypeVar::new(db, lower, Type::Never, Type::TypeVar(typevar)),
                    1,
                );
                let upper = Node::new_constraint(
                    db,
                    ConstrainedTypeVar::new(db, upper, Type::TypeVar(typevar), Type::object()),
                    1,
                );
                lower.and(db, upper)
            }

            // L ≤ T ≤ U == ([L] ≤ T) && ([T] ≤ U)
            (Type::TypeVar(lower), _) if typevar.can_be_bound_for(db, lower) => {
                let lower = Node::new_constraint(
                    db,
                    ConstrainedTypeVar::new(db, lower, Type::Never, Type::TypeVar(typevar)),
                    1,
                );
                let upper = if upper.is_object() {
                    Node::AlwaysTrue
                } else {
                    Self::new_node(db, typevar, Type::Never, upper)
                };
                lower.and(db, upper)
            }

            // L ≤ T ≤ U == (L ≤ [T]) && (T ≤ [U])
            (_, Type::TypeVar(upper)) if typevar.can_be_bound_for(db, upper) => {
                let lower = if lower.is_never() {
                    Node::AlwaysTrue
                } else {
                    Self::new_node(db, typevar, lower, Type::object())
                };
                let upper = Node::new_constraint(
                    db,
                    ConstrainedTypeVar::new(db, upper, Type::TypeVar(typevar), Type::object()),
                    1,
                );
                lower.and(db, upper)
            }

            _ => Node::new_constraint(db, ConstrainedTypeVar::new(db, typevar, lower, upper), 1),
        }
    }

    fn when_true(self) -> ConstraintAssignment<'db> {
        ConstraintAssignment::Positive(self)
    }

    fn when_false(self) -> ConstraintAssignment<'db> {
        ConstraintAssignment::Negative(self)
    }

    fn normalized(self, db: &'db dyn Db) -> Self {
        Self::new(
            db,
            self.typevar(db),
            self.lower(db).normalized(db),
            self.upper(db).normalized(db),
        )
    }

    /// Defines the ordering of the variables in a constraint set BDD.
    ///
    /// If we only care about _correctness_, we can choose any ordering that we want, as long as
    /// it's consistent. However, different orderings can have very different _performance_
    /// characteristics. Many BDD libraries attempt to reorder variables on the fly while building
    /// and working with BDDs. We don't do that, but we have tried to make some simple choices that
    /// have clear wins.
    ///
    /// In particular, we compare the _typevars_ of each constraint first, so that all constraints
    /// for a single typevar are guaranteed to be adjacent in the BDD structure. There are several
    /// simplifications that we perform that operate on constraints with the same typevar, and this
    /// ensures that we can find all candidate simplifications more easily.
    fn ordering(self, db: &'db dyn Db) -> impl Ord {
        (
            self.typevar(db).binding_context(db),
            self.typevar(db).identity(db),
            self.as_id(),
        )
    }

    /// Returns whether this constraint implies another — i.e., whether every type that
    /// satisfies this constraint also satisfies `other`.
    ///
    /// This is used to simplify how we display constraint sets, by removing redundant constraints
    /// from a clause.
    fn implies(self, db: &'db dyn Db, other: Self) -> bool {
        if !self.typevar(db).is_same_typevar_as(db, other.typevar(db)) {
            return false;
        }
        other
            .lower(db)
            .is_constraint_set_assignable_to(db, self.lower(db))
            && self
                .upper(db)
                .is_constraint_set_assignable_to(db, other.upper(db))
    }

    /// Returns the intersection of two range constraints, or `None` if the intersection is empty.
    fn intersect(self, db: &'db dyn Db, other: Self) -> IntersectionResult<'db> {
        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α ≤ (t₁ ∩ t₂))
        let lower = UnionType::from_elements(db, [self.lower(db), other.lower(db)]);
        let upper = IntersectionType::from_elements(db, [self.upper(db), other.upper(db)]);

        // If `lower ≰ upper`, then the intersection is empty, since there is no type that is both
        // greater than `lower`, and less than `upper`.
        if !lower.is_constraint_set_assignable_to(db, upper) {
            return IntersectionResult::Disjoint;
        }

        if lower.is_union() || upper.is_nontrivial_intersection(db) {
            return IntersectionResult::CannotSimplify;
        }

        IntersectionResult::Simplified(Self::new(db, self.typevar(db), lower, upper))
    }

    pub(crate) fn display(self, db: &'db dyn Db) -> impl Display {
        self.display_inner(db, false)
    }

    fn display_negated(self, db: &'db dyn Db) -> impl Display {
        self.display_inner(db, true)
    }

    fn display_inner(self, db: &'db dyn Db, negated: bool) -> impl Display {
        struct DisplayConstrainedTypeVar<'db> {
            constraint: ConstrainedTypeVar<'db>,
            negated: bool,
            db: &'db dyn Db,
        }

        impl Display for DisplayConstrainedTypeVar<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let lower = self.constraint.lower(self.db);
                let upper = self.constraint.upper(self.db);
                let typevar = self.constraint.typevar(self.db);
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
            constraint: self,
            negated,
            db,
        }
    }
}

/// A BDD node.
///
/// The "variables" of a constraint set BDD are individual constraints, represented by an interned
/// [`ConstrainedTypeVar`].
///
/// Terminal nodes (`false` and `true`) have their own dedicated enum variants. The
/// [`Interior`][InteriorNode] variant represents interior nodes.
///
/// BDD nodes are _quasi-reduced_, which means that there are no duplicate nodes (which we handle
/// via Salsa interning). Unlike the typical BDD representation, which is (fully) reduced, we do
/// allow redundant nodes, with `if_true` and `if_false` edges that point at the same node. That
/// means that our BDDs "remember" all of the individual constraints that they were created with.
///
/// BDD nodes are also _ordered_, meaning that every path from the root of a BDD to a terminal node
/// visits variables in the same order. [`ConstrainedTypeVar::ordering`] defines the variable
/// ordering that we use for constraint set BDDs.
///
/// In addition to this BDD variable ordering, we also track a `source_order` for each individual
/// constraint. This records the order in which constraints are added to the constraint set, which
/// typically tracks when they appear in the underlying Python source code. This provides an
/// ordering that is stable across multiple runs, for consistent test and diagnostic output. (We
/// cannot use this ordering as our BDD variable ordering, since we calculate it from already
/// constructed BDDs, and we need the BDD variable ordering to be fixed and available before
/// construction starts.)
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
enum Node<'db> {
    AlwaysFalse,
    AlwaysTrue,
    Interior(InteriorNode<'db>),
}

impl<'db> Node<'db> {
    /// Creates a new BDD node, ensuring that it is quasi-reduced.
    fn new(
        db: &'db dyn Db,
        constraint: ConstrainedTypeVar<'db>,
        if_true: Node<'db>,
        if_false: Node<'db>,
        source_order: usize,
    ) -> Self {
        debug_assert!((if_true.root_constraint(db)).is_none_or(|root_constraint| {
            root_constraint.ordering(db) > constraint.ordering(db)
        }));
        debug_assert!(
            (if_false.root_constraint(db)).is_none_or(|root_constraint| {
                root_constraint.ordering(db) > constraint.ordering(db)
            })
        );
        if if_true == Node::AlwaysFalse && if_false == Node::AlwaysFalse {
            return Node::AlwaysFalse;
        }
        let max_source_order = source_order
            .max(if_true.max_source_order(db))
            .max(if_false.max_source_order(db));
        Self::Interior(InteriorNode::new(
            db,
            constraint,
            if_true,
            if_false,
            source_order,
            max_source_order,
        ))
    }

    /// Creates a new BDD node for an individual constraint. (The BDD will evaluate to `true` when
    /// the constraint holds, and to `false` when it does not.)
    fn new_constraint(
        db: &'db dyn Db,
        constraint: ConstrainedTypeVar<'db>,
        source_order: usize,
    ) -> Self {
        Self::Interior(InteriorNode::new(
            db,
            constraint,
            Node::AlwaysTrue,
            Node::AlwaysFalse,
            source_order,
            source_order,
        ))
    }

    /// Creates a new BDD node for a positive or negative individual constraint. (For a positive
    /// constraint, this returns the same BDD node as [`new_constraint`][Self::new_constraint]. For
    /// a negative constraint, it returns the negation of that BDD node.)
    fn new_satisfied_constraint(
        db: &'db dyn Db,
        constraint: ConstraintAssignment<'db>,
        source_order: usize,
    ) -> Self {
        match constraint {
            ConstraintAssignment::Positive(constraint) => Self::Interior(InteriorNode::new(
                db,
                constraint,
                Node::AlwaysTrue,
                Node::AlwaysFalse,
                source_order,
                source_order,
            )),
            ConstraintAssignment::Negative(constraint) => Self::Interior(InteriorNode::new(
                db,
                constraint,
                Node::AlwaysFalse,
                Node::AlwaysTrue,
                source_order,
                source_order,
            )),
        }
    }

    /// Returns the BDD variable of the root node of this BDD, or `None` if this BDD is a terminal
    /// node.
    fn root_constraint(self, db: &'db dyn Db) -> Option<ConstrainedTypeVar<'db>> {
        match self {
            Node::Interior(interior) => Some(interior.constraint(db)),
            _ => None,
        }
    }

    fn max_source_order(self, db: &'db dyn Db) -> usize {
        match self {
            Node::Interior(interior) => interior.max_source_order(db),
            Node::AlwaysTrue | Node::AlwaysFalse => 0,
        }
    }

    /// Returns a copy of this BDD node with all `source_order`s adjusted by the given amount.
    fn with_adjusted_source_order(self, db: &'db dyn Db, delta: usize) -> Self {
        if delta == 0 {
            return self;
        }
        match self {
            Node::AlwaysTrue => Node::AlwaysTrue,
            Node::AlwaysFalse => Node::AlwaysFalse,
            Node::Interior(interior) => Node::new(
                db,
                interior.constraint(db),
                interior.if_true(db).with_adjusted_source_order(db, delta),
                interior.if_false(db).with_adjusted_source_order(db, delta),
                interior.source_order(db) + delta,
            ),
        }
    }

    fn for_each_path(self, db: &'db dyn Db, mut f: impl FnMut(&PathAssignments<'db>)) {
        match self {
            Node::AlwaysTrue => {}
            Node::AlwaysFalse => {}
            Node::Interior(interior) => {
                let map = interior.sequent_map(db);
                let mut path = PathAssignments::default();
                self.for_each_path_inner(db, &mut f, map, &mut path);
            }
        }
    }

    fn for_each_path_inner(
        self,
        db: &'db dyn Db,
        f: &mut dyn FnMut(&PathAssignments<'db>),
        map: &SequentMap<'db>,
        path: &mut PathAssignments<'db>,
    ) {
        match self {
            Node::AlwaysTrue => f(path),
            Node::AlwaysFalse => {}
            Node::Interior(interior) => {
                let constraint = interior.constraint(db);
                let source_order = interior.source_order(db);
                path.walk_edge(db, map, constraint.when_true(), source_order, |path, _| {
                    interior.if_true(db).for_each_path_inner(db, f, map, path);
                });
                path.walk_edge(db, map, constraint.when_false(), source_order, |path, _| {
                    interior.if_false(db).for_each_path_inner(db, f, map, path);
                });
            }
        }
    }

    /// Returns whether this BDD represent the constant function `true`.
    fn is_always_satisfied(self, db: &'db dyn Db) -> bool {
        match self {
            Node::AlwaysTrue => true,
            Node::AlwaysFalse => false,
            Node::Interior(interior) => {
                let map = interior.sequent_map(db);
                let mut path = PathAssignments::default();
                self.is_always_satisfied_inner(db, map, &mut path)
            }
        }
    }

    fn is_always_satisfied_inner(
        self,
        db: &'db dyn Db,
        map: &SequentMap<'db>,
        path: &mut PathAssignments<'db>,
    ) -> bool {
        match self {
            Node::AlwaysTrue => true,
            Node::AlwaysFalse => false,
            Node::Interior(interior) => {
                // walk_edge will return None if this node's constraint (or anything we can derive
                // from it) causes the if_true edge to become impossible. We want to ignore
                // impossible paths, and so we treat them as passing the "always satisfied" check.
                let constraint = interior.constraint(db);
                let source_order = interior.source_order(db);
                let true_always_satisfied = path
                    .walk_edge(db, map, constraint.when_true(), source_order, |path, _| {
                        interior
                            .if_true(db)
                            .is_always_satisfied_inner(db, map, path)
                    })
                    .unwrap_or(true);
                if !true_always_satisfied {
                    return false;
                }

                // Ditto for the if_false branch
                path.walk_edge(db, map, constraint.when_false(), source_order, |path, _| {
                    interior
                        .if_false(db)
                        .is_always_satisfied_inner(db, map, path)
                })
                .unwrap_or(true)
            }
        }
    }

    /// Returns whether this BDD represent the constant function `false`.
    fn is_never_satisfied(self, db: &'db dyn Db) -> bool {
        match self {
            Node::AlwaysTrue => false,
            Node::AlwaysFalse => true,
            Node::Interior(interior) => {
                let map = interior.sequent_map(db);
                let mut path = PathAssignments::default();
                self.is_never_satisfied_inner(db, map, &mut path)
            }
        }
    }

    fn is_never_satisfied_inner(
        self,
        db: &'db dyn Db,
        map: &SequentMap<'db>,
        path: &mut PathAssignments<'db>,
    ) -> bool {
        match self {
            Node::AlwaysTrue => false,
            Node::AlwaysFalse => true,
            Node::Interior(interior) => {
                // walk_edge will return None if this node's constraint (or anything we can derive
                // from it) causes the if_true edge to become impossible. We want to ignore
                // impossible paths, and so we treat them as passing the "never satisfied" check.
                let constraint = interior.constraint(db);
                let source_order = interior.source_order(db);
                let true_never_satisfied = path
                    .walk_edge(db, map, constraint.when_true(), source_order, |path, _| {
                        interior.if_true(db).is_never_satisfied_inner(db, map, path)
                    })
                    .unwrap_or(true);
                if !true_never_satisfied {
                    return false;
                }

                // Ditto for the if_false branch
                path.walk_edge(db, map, constraint.when_false(), source_order, |path, _| {
                    interior
                        .if_false(db)
                        .is_never_satisfied_inner(db, map, path)
                })
                .unwrap_or(true)
            }
        }
    }

    /// Returns the negation of this BDD.
    fn negate(self, db: &'db dyn Db) -> Self {
        match self {
            Node::AlwaysTrue => Node::AlwaysFalse,
            Node::AlwaysFalse => Node::AlwaysTrue,
            Node::Interior(interior) => interior.negate(db),
        }
    }

    /// Returns the `or` or union of two BDDs.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    fn or_with_offset(self, db: &'db dyn Db, other: Self) -> Self {
        // To ensure that `self` appears before `other` in `source_order`, we add the maximum
        // `source_order` of the lhs to all of the `source_order`s in the rhs.
        //
        // TODO: If we store `other_offset` as a new field on InteriorNode, we might be able to
        // avoid all of the extra work in the calls to with_adjusted_source_order, and apply the
        // adjustment lazily when walking a BDD tree. (ditto below in the other _with_offset
        // methods)
        let other_offset = self.max_source_order(db);
        self.or_inner(db, other, other_offset)
    }

    fn or(self, db: &'db dyn Db, other: Self) -> Self {
        self.or_inner(db, other, 0)
    }

    fn or_inner(self, db: &'db dyn Db, other: Self, other_offset: usize) -> Self {
        match (self, other) {
            (Node::AlwaysTrue, Node::AlwaysTrue) => Node::AlwaysTrue,
            (Node::AlwaysTrue, Node::Interior(other_interior)) => Node::new(
                db,
                other_interior.constraint(db),
                Node::AlwaysTrue,
                Node::AlwaysTrue,
                other_interior.source_order(db) + other_offset,
            ),
            (Node::Interior(self_interior), Node::AlwaysTrue) => Node::new(
                db,
                self_interior.constraint(db),
                Node::AlwaysTrue,
                Node::AlwaysTrue,
                self_interior.source_order(db),
            ),
            (Node::AlwaysFalse, _) => other.with_adjusted_source_order(db, other_offset),
            (_, Node::AlwaysFalse) => self,
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.or(db, other_interior, other_offset)
            }
        }
    }

    /// Returns the `and` or intersection of two BDDs.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    fn and_with_offset(self, db: &'db dyn Db, other: Self) -> Self {
        // To ensure that `self` appears before `other` in `source_order`, we add the maximum
        // `source_order` of the lhs to all of the `source_order`s in the rhs.
        let other_offset = self.max_source_order(db);
        self.and_inner(db, other, other_offset)
    }

    fn and(self, db: &'db dyn Db, other: Self) -> Self {
        self.and_inner(db, other, 0)
    }

    fn and_inner(self, db: &'db dyn Db, other: Self, other_offset: usize) -> Self {
        match (self, other) {
            (Node::AlwaysFalse, Node::AlwaysFalse) => Node::AlwaysFalse,
            (Node::AlwaysFalse, Node::Interior(other_interior)) => Node::new(
                db,
                other_interior.constraint(db),
                Node::AlwaysFalse,
                Node::AlwaysFalse,
                other_interior.source_order(db) + other_offset,
            ),
            (Node::Interior(self_interior), Node::AlwaysFalse) => Node::new(
                db,
                self_interior.constraint(db),
                Node::AlwaysFalse,
                Node::AlwaysFalse,
                self_interior.source_order(db),
            ),
            (Node::AlwaysTrue, _) => other.with_adjusted_source_order(db, other_offset),
            (_, Node::AlwaysTrue) => self,
            (Node::Interior(self_interior), Node::Interior(other_interior)) => {
                self_interior.and(db, other_interior, other_offset)
            }
        }
    }

    fn implies(self, db: &'db dyn Db, other: Self) -> Self {
        // p → q == ¬p ∨ q
        self.negate(db).or(db, other)
    }

    /// Returns a new BDD that evaluates to `true` when both input BDDs evaluate to the same
    /// result.
    ///
    /// In the result, `self` will appear before `other` according to the `source_order` of the BDD
    /// nodes.
    fn iff_with_offset(self, db: &'db dyn Db, other: Self) -> Self {
        // To ensure that `self` appears before `other` in `source_order`, we add the maximum
        // `source_order` of the lhs to all of the `source_order`s in the rhs.
        let other_offset = self.max_source_order(db);
        self.iff_inner(db, other, other_offset)
    }

    fn iff(self, db: &'db dyn Db, other: Self) -> Self {
        self.iff_inner(db, other, 0)
    }

    fn iff_inner(self, db: &'db dyn Db, other: Self, other_offset: usize) -> Self {
        match (self, other) {
            (Node::AlwaysFalse, Node::AlwaysFalse) | (Node::AlwaysTrue, Node::AlwaysTrue) => {
                Node::AlwaysTrue
            }
            (Node::AlwaysTrue, Node::AlwaysFalse) | (Node::AlwaysFalse, Node::AlwaysTrue) => {
                Node::AlwaysFalse
            }
            (Node::AlwaysTrue | Node::AlwaysFalse, Node::Interior(interior)) => Node::new(
                db,
                interior.constraint(db),
                self.iff_inner(db, interior.if_true(db), other_offset),
                self.iff_inner(db, interior.if_false(db), other_offset),
                interior.source_order(db) + other_offset,
            ),
            (Node::Interior(interior), Node::AlwaysTrue | Node::AlwaysFalse) => Node::new(
                db,
                interior.constraint(db),
                interior.if_true(db).iff_inner(db, other, other_offset),
                interior.if_false(db).iff_inner(db, other, other_offset),
                interior.source_order(db),
            ),
            (Node::Interior(a), Node::Interior(b)) => a.iff(db, b, other_offset),
        }
    }

    /// Returns the `if-then-else` of three BDDs: when `self` evaluates to `true`, it returns what
    /// `then_node` evaluates to; otherwise it returns what `else_node` evaluates to.
    fn ite(self, db: &'db dyn Db, then_node: Self, else_node: Self) -> Self {
        self.and(db, then_node)
            .or(db, self.negate(db).and(db, else_node))
    }

    fn implies_subtype_of(self, db: &'db dyn Db, lhs: Type<'db>, rhs: Type<'db>) -> Self {
        // When checking subtyping involving a typevar, we can turn the subtyping check into a
        // constraint (i.e, "is `T` a subtype of `int` becomes the constraint `T ≤ int`), and then
        // check when the BDD implies that constraint.
        //
        // Note that we are NOT guaranteed that `lhs` and `rhs` will always be fully static, since
        // these types are coming in from arbitrary subtyping checks that the caller might want to
        // perform. So we have to take the appropriate materialization when translating the check
        // into a constraint.
        let constraint = match (lhs, rhs) {
            (Type::TypeVar(bound_typevar), _) => ConstrainedTypeVar::new_node(
                db,
                bound_typevar,
                Type::Never,
                rhs.bottom_materialization(db),
            ),
            (_, Type::TypeVar(bound_typevar)) => ConstrainedTypeVar::new_node(
                db,
                bound_typevar,
                lhs.top_materialization(db),
                Type::object(),
            ),
            _ => panic!("at least one type should be a typevar"),
        };

        self.implies(db, constraint)
    }

    fn satisfied_by_all_typevars(
        self,
        db: &'db dyn Db,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> bool {
        match self {
            Node::AlwaysTrue => return true,
            Node::AlwaysFalse => return false,
            Node::Interior(_) => {}
        }

        let mut typevars = FxHashSet::default();
        self.for_each_constraint(db, &mut |constraint, _| {
            typevars.insert(constraint.typevar(db));
        });

        // Returns if some specialization satisfies this constraint set.
        let some_specialization_satisfies = move |specializations: Node<'db>| {
            let when_satisfied = specializations.implies(db, self).and(db, specializations);
            !when_satisfied.is_never_satisfied(db)
        };

        // Returns if all specializations satisfy this constraint set.
        let all_specializations_satisfy = move |specializations: Node<'db>| {
            let when_satisfied = specializations.implies(db, self).and(db, specializations);
            when_satisfied
                .iff(db, specializations)
                .is_always_satisfied(db)
        };

        for typevar in typevars {
            if typevar.is_inferable(db, inferable) {
                // If the typevar is in inferable position, we need to verify that some valid
                // specialization satisfies the constraint set.
                let valid_specializations = typevar.valid_specializations(db);
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
                    typevar.required_specializations(db);
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
    fn exists(
        self,
        db: &'db dyn Db,
        bound_typevars: impl IntoIterator<Item = BoundTypeVarIdentity<'db>>,
    ) -> Self {
        bound_typevars
            .into_iter()
            .fold(self, |abstracted, bound_typevar| {
                abstracted.exists_one(db, bound_typevar)
            })
    }

    fn exists_one(self, db: &'db dyn Db, bound_typevar: BoundTypeVarIdentity<'db>) -> Self {
        match self {
            Node::AlwaysTrue => Node::AlwaysTrue,
            Node::AlwaysFalse => Node::AlwaysFalse,
            Node::Interior(interior) => interior.exists_one(db, bound_typevar),
        }
    }

    /// Returns a new BDD that is the _existential abstraction_ of `self` for a set of typevars.
    /// All typevars _other_ than the one given will be removed and abstracted away.
    fn retain_one(self, db: &'db dyn Db, bound_typevar: BoundTypeVarIdentity<'db>) -> Self {
        match self {
            Node::AlwaysTrue => Node::AlwaysTrue,
            Node::AlwaysFalse => Node::AlwaysFalse,
            Node::Interior(interior) => interior.retain_one(db, bound_typevar),
        }
    }

    fn abstract_one_inner(
        self,
        db: &'db dyn Db,
        should_remove: &mut dyn FnMut(ConstrainedTypeVar<'db>) -> bool,
        map: &SequentMap<'db>,
        path: &mut PathAssignments<'db>,
    ) -> Self {
        match self {
            Node::AlwaysTrue => Node::AlwaysTrue,
            Node::AlwaysFalse => Node::AlwaysFalse,
            Node::Interior(interior) => interior.abstract_one_inner(db, should_remove, map, path),
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
    fn find_representative_types(
        self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarIdentity<'db>,
        mut f: impl FnMut(Option<&[RepresentativeBounds<'db>]>),
    ) {
        self.retain_one(db, bound_typevar)
            .find_representative_types_inner(db, &mut Vec::default(), &mut f);
    }

    fn find_representative_types_inner(
        self,
        db: &'db dyn Db,
        current_bounds: &mut Vec<RepresentativeBounds<'db>>,
        f: &mut dyn FnMut(Option<&[RepresentativeBounds<'db>]>),
    ) {
        match self {
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

            Node::Interior(interior) => {
                let reset_point = current_bounds.len();

                // For an interior node, there are two outgoing paths: one for the `if_true`
                // branch, and one for the `if_false` branch.
                //
                // For the `if_true` branch, this node's constraint places additional restrictions
                // on the types that satisfy the current path through the BDD. So we intersect the
                // current glb/lub with the constraint's bounds to get the new glb/lub for the
                // recursive call.
                current_bounds.push(RepresentativeBounds::from_interior_node(db, interior));
                interior
                    .if_true(db)
                    .find_representative_types_inner(db, current_bounds, f);
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
                    .if_false(db)
                    .find_representative_types_inner(db, current_bounds, f);
            }
        }
    }

    /// Returns a new BDD that returns the same results as `self`, but with some inputs fixed to
    /// particular values. (Those variables will not be checked when evaluating the result, and
    /// will not be present in the result.)
    ///
    /// Also returns whether _all_ of the restricted variables appeared in the BDD.
    fn restrict(
        self,
        db: &'db dyn Db,
        assignment: impl IntoIterator<Item = ConstraintAssignment<'db>>,
    ) -> (Self, bool) {
        assignment
            .into_iter()
            .fold((self, true), |(restricted, found), assignment| {
                let (restricted, found_this) = restricted.restrict_one(db, assignment);
                (restricted, found && found_this)
            })
    }

    /// Returns a new BDD that returns the same results as `self`, but with one input fixed to a
    /// particular value. (That variable will be not be checked when evaluating the result, and
    /// will not be present in the result.)
    ///
    /// Also returns whether the restricted variable appeared in the BDD.
    fn restrict_one(self, db: &'db dyn Db, assignment: ConstraintAssignment<'db>) -> (Self, bool) {
        match self {
            Node::AlwaysTrue => (Node::AlwaysTrue, false),
            Node::AlwaysFalse => (Node::AlwaysFalse, false),
            Node::Interior(interior) => interior.restrict_one(db, assignment),
        }
    }

    /// Returns a new BDD with any occurrence of `left ∧ right` replaced with `replacement`.
    fn substitute_intersection(
        self,
        db: &'db dyn Db,
        left: ConstraintAssignment<'db>,
        left_source_order: usize,
        right: ConstraintAssignment<'db>,
        right_source_order: usize,
        replacement: Node<'db>,
    ) -> Self {
        // We perform a Shannon expansion to find out what the input BDD evaluates to when:
        //   - left and right are both true
        //   - left is false
        //   - left is true and right is false
        // This covers the entire truth table of `left ∧ right`.
        let (when_left_and_right, both_found) = self.restrict(db, [left, right]);
        if !both_found {
            // If left and right are not both present in the input BDD, we should not even attempt
            // the substitution, since the Shannon expansion might introduce the missing variables!
            // That confuses us below when we try to detect whether the substitution is consistent
            // with the input.
            return self;
        }
        let (when_not_left, _) = self.restrict(db, [left.negated()]);
        let (when_left_but_not_right, _) = self.restrict(db, [left, right.negated()]);

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
        let left_node = Node::new_satisfied_constraint(db, left, left_source_order);
        let right_node = Node::new_satisfied_constraint(db, right, right_source_order);
        let right_result = right_node.ite(db, Node::AlwaysFalse, when_left_but_not_right);
        let left_result = left_node.ite(db, right_result, when_not_left);
        let result = replacement.ite(db, when_left_and_right, left_result);

        // Lastly, verify that the result is consistent with the input. (It must produce the same
        // results when `left ∧ right`.) If it doesn't, the substitution isn't valid, and we should
        // return the original BDD unmodified.
        let validity = replacement.iff(db, left_node.and(db, right_node));
        let constrained_original = self.and(db, validity);
        let constrained_replacement = result.and(db, validity);
        if constrained_original == constrained_replacement {
            result
        } else {
            self
        }
    }

    /// Returns a new BDD with any occurrence of `left ∨ right` replaced with `replacement`.
    fn substitute_union(
        self,
        db: &'db dyn Db,
        left: ConstraintAssignment<'db>,
        left_source_order: usize,
        right: ConstraintAssignment<'db>,
        right_source_order: usize,
        replacement: Node<'db>,
    ) -> Self {
        // We perform a Shannon expansion to find out what the input BDD evaluates to when:
        //   - left and right are both true
        //   - left is true and right is false
        //   - left is false and right is true
        //   - left and right are both false
        // This covers the entire truth table of `left ∨ right`.
        let (when_l1_r1, both_found) = self.restrict(db, [left, right]);
        if !both_found {
            // If left and right are not both present in the input BDD, we should not even attempt
            // the substitution, since the Shannon expansion might introduce the missing variables!
            // That confuses us below when we try to detect whether the substitution is consistent
            // with the input.
            return self;
        }
        let (when_l0_r0, _) = self.restrict(db, [left.negated(), right.negated()]);
        let (when_l1_r0, _) = self.restrict(db, [left, right.negated()]);
        let (when_l0_r1, _) = self.restrict(db, [left.negated(), right]);

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
            db,
            when_l1_r0.or(db, when_l0_r1.or(db, when_l1_r1)),
            when_l0_r0,
        );

        // Lastly, verify that the result is consistent with the input. (It must produce the same
        // results when `left ∨ right`.) If it doesn't, the substitution isn't valid, and we should
        // return the original BDD unmodified.
        let left_node = Node::new_satisfied_constraint(db, left, left_source_order);
        let right_node = Node::new_satisfied_constraint(db, right, right_source_order);
        let validity = replacement.iff(db, left_node.or(db, right_node));
        let constrained_original = self.and(db, validity);
        let constrained_replacement = result.and(db, validity);
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
        db: &'db dyn Db,
        f: &mut dyn FnMut(ConstrainedTypeVar<'db>, usize),
    ) {
        let Node::Interior(interior) = self else {
            return;
        };
        f(interior.constraint(db), interior.source_order(db));
        interior.if_true(db).for_each_constraint(db, f);
        interior.if_false(db).for_each_constraint(db, f);
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
    fn simplify_for_display(self, db: &'db dyn Db) -> Self {
        match self {
            Node::AlwaysTrue | Node::AlwaysFalse => self,
            Node::Interior(interior) => interior.simplify(db),
        }
    }

    /// Returns clauses describing all of the variable assignments that cause this BDD to evaluate
    /// to `true`. (This translates the boolean function that this BDD represents into DNF form.)
    fn satisfied_clauses(self, db: &'db dyn Db) -> SatisfiedClauses<'db> {
        struct Searcher<'db> {
            clauses: SatisfiedClauses<'db>,
            current_clause: SatisfiedClause<'db>,
        }

        impl<'db> Searcher<'db> {
            fn visit_node(&mut self, db: &'db dyn Db, node: Node<'db>) {
                match node {
                    Node::AlwaysFalse => {}
                    Node::AlwaysTrue => self.clauses.push(self.current_clause.clone()),
                    Node::Interior(interior) => {
                        let interior_constraint = interior.constraint(db).normalized(db);
                        self.current_clause.push(interior_constraint.when_true());
                        self.visit_node(db, interior.if_true(db));
                        self.current_clause.pop();
                        self.current_clause.push(interior_constraint.when_false());
                        self.visit_node(db, interior.if_false(db));
                        self.current_clause.pop();
                    }
                }
            }
        }

        let mut searcher = Searcher {
            clauses: SatisfiedClauses::default(),
            current_clause: SatisfiedClause::default(),
        };
        searcher.visit_node(db, self);
        searcher.clauses
    }

    fn display(self, db: &'db dyn Db) -> impl Display {
        // To render a BDD in DNF form, you perform a depth-first search of the BDD tree, looking
        // for any path that leads to the AlwaysTrue terminal. Each such path represents one of the
        // intersection clauses in the DNF form. The path traverses zero or more interior nodes,
        // and takes either the true or false edge from each one. That gives you the positive or
        // negative individual constraints in the path's clause.
        struct DisplayNode<'db> {
            node: Node<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplayNode<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.node {
                    Node::AlwaysTrue => f.write_str("always"),
                    Node::AlwaysFalse => f.write_str("never"),
                    Node::Interior(_) => {
                        let mut clauses = self.node.satisfied_clauses(self.db);
                        clauses.simplify(self.db);
                        clauses.display(self.db).fmt(f)
                    }
                }
            }
        }

        DisplayNode { node: self, db }
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
    fn display_graph(self, db: &'db dyn Db, prefix: &dyn Display) -> impl Display {
        struct DisplayNode<'a, 'db> {
            db: &'db dyn Db,
            node: Node<'db>,
            prefix: &'a dyn Display,
        }

        impl<'a, 'db> DisplayNode<'a, 'db> {
            fn new(db: &'db dyn Db, node: Node<'db>, prefix: &'a dyn Display) -> Self {
                Self { db, node, prefix }
            }
        }

        impl Display for DisplayNode<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.node {
                    Node::AlwaysTrue => write!(f, "always"),
                    Node::AlwaysFalse => write!(f, "never"),
                    Node::Interior(interior) => {
                        write!(
                            f,
                            "{} {}/{}",
                            interior.constraint(self.db).display(self.db),
                            interior.source_order(self.db),
                            interior.max_source_order(self.db),
                        )?;
                        // Calling display_graph recursively here causes rustc to claim that the
                        // expect(unused) up above is unfulfilled!
                        write!(
                            f,
                            "\n{}┡━₁ {}",
                            self.prefix,
                            DisplayNode::new(
                                self.db,
                                interior.if_true(self.db),
                                &format_args!("{}│   ", self.prefix)
                            ),
                        )?;
                        write!(
                            f,
                            "\n{}└─₀ {}",
                            self.prefix,
                            DisplayNode::new(
                                self.db,
                                interior.if_false(self.db),
                                &format_args!("{}    ", self.prefix)
                            ),
                        )?;
                        Ok(())
                    }
                }
            }
        }

        DisplayNode::new(db, self, prefix)
    }
}

#[derive(Clone, Copy, Debug)]
struct RepresentativeBounds<'db> {
    lower: Type<'db>,
    upper: Type<'db>,
    source_order: usize,
}

impl<'db> RepresentativeBounds<'db> {
    fn from_interior_node(db: &'db dyn Db, interior: InteriorNode<'db>) -> Self {
        let constraint = interior.constraint(db);
        let lower = constraint.lower(db);
        let upper = constraint.upper(db);
        let source_order = interior.source_order(db);
        Self {
            lower,
            upper,
            source_order,
        }
    }
}

/// An interior node of a BDD
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct InteriorNode<'db> {
    constraint: ConstrainedTypeVar<'db>,
    if_true: Node<'db>,
    if_false: Node<'db>,

    /// Represents the order in which this node's constraint was added to the containing constraint
    /// set, relative to all of the other constraints in the set. This starts off at 1 for a simple
    /// single-constraint set (e.g. created with [`Node::new_constraint`] or
    /// [`Node::new_satisfied_constraint`]). It will get incremented, if needed, as that simple BDD
    /// is combined into larger BDDs.
    source_order: usize,

    /// The maximum `source_order` across this node and all of its descendants.
    max_source_order: usize,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for InteriorNode<'_> {}

#[salsa::tracked]
impl<'db> InteriorNode<'db> {
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn negate(self, db: &'db dyn Db) -> Node<'db> {
        Node::new(
            db,
            self.constraint(db),
            self.if_true(db).negate(db),
            self.if_false(db).negate(db),
            self.source_order(db),
        )
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn or(self, db: &'db dyn Db, other: Self, other_offset: usize) -> Node<'db> {
        let self_constraint = self.constraint(db);
        let other_constraint = other.constraint(db);
        match (self_constraint.ordering(db)).cmp(&other_constraint.ordering(db)) {
            Ordering::Equal => Node::new(
                db,
                self_constraint,
                self.if_true(db)
                    .or_inner(db, other.if_true(db), other_offset),
                self.if_false(db)
                    .or_inner(db, other.if_false(db), other_offset),
                self.source_order(db),
            ),
            Ordering::Less => Node::new(
                db,
                self_constraint,
                self.if_true(db)
                    .or_inner(db, Node::Interior(other), other_offset),
                self.if_false(db)
                    .or_inner(db, Node::Interior(other), other_offset),
                self.source_order(db),
            ),
            Ordering::Greater => Node::new(
                db,
                other_constraint,
                Node::Interior(self).or_inner(db, other.if_true(db), other_offset),
                Node::Interior(self).or_inner(db, other.if_false(db), other_offset),
                other.source_order(db) + other_offset,
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn and(self, db: &'db dyn Db, other: Self, other_offset: usize) -> Node<'db> {
        let self_constraint = self.constraint(db);
        let other_constraint = other.constraint(db);
        match (self_constraint.ordering(db)).cmp(&other_constraint.ordering(db)) {
            Ordering::Equal => Node::new(
                db,
                self_constraint,
                self.if_true(db)
                    .and_inner(db, other.if_true(db), other_offset),
                self.if_false(db)
                    .and_inner(db, other.if_false(db), other_offset),
                self.source_order(db),
            ),
            Ordering::Less => Node::new(
                db,
                self_constraint,
                self.if_true(db)
                    .and_inner(db, Node::Interior(other), other_offset),
                self.if_false(db)
                    .and_inner(db, Node::Interior(other), other_offset),
                self.source_order(db),
            ),
            Ordering::Greater => Node::new(
                db,
                other_constraint,
                Node::Interior(self).and_inner(db, other.if_true(db), other_offset),
                Node::Interior(self).and_inner(db, other.if_false(db), other_offset),
                other.source_order(db) + other_offset,
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn iff(self, db: &'db dyn Db, other: Self, other_offset: usize) -> Node<'db> {
        let self_constraint = self.constraint(db);
        let other_constraint = other.constraint(db);
        match (self_constraint.ordering(db)).cmp(&other_constraint.ordering(db)) {
            Ordering::Equal => Node::new(
                db,
                self_constraint,
                self.if_true(db)
                    .iff_inner(db, other.if_true(db), other_offset),
                self.if_false(db)
                    .iff_inner(db, other.if_false(db), other_offset),
                self.source_order(db),
            ),
            Ordering::Less => Node::new(
                db,
                self_constraint,
                self.if_true(db)
                    .iff_inner(db, Node::Interior(other), other_offset),
                self.if_false(db)
                    .iff_inner(db, Node::Interior(other), other_offset),
                self.source_order(db),
            ),
            Ordering::Greater => Node::new(
                db,
                other_constraint,
                Node::Interior(self).iff_inner(db, other.if_true(db), other_offset),
                Node::Interior(self).iff_inner(db, other.if_false(db), other_offset),
                other.source_order(db) + other_offset,
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn exists_one(self, db: &'db dyn Db, bound_typevar: BoundTypeVarIdentity<'db>) -> Node<'db> {
        let map = self.sequent_map(db);
        let mut path = PathAssignments::default();
        let mentions_typevar = |ty: Type<'db>| match ty {
            Type::TypeVar(haystack) => haystack.identity(db) == bound_typevar,
            _ => false,
        };
        self.abstract_one_inner(
            db,
            // Remove any node that constrains `bound_typevar`, or that has a lower/upper bound
            // that mentions `bound_typevar`.
            // TODO: This will currently remove constraints that mention a typevar, but the sequent
            // map is not yet propagating all derived facts about those constraints. For instance,
            // removing `T` from `T ≤ int ∧ U ≤ Sequence[T]` should produce `U ≤ Sequence[int]`.
            // But that requires `T ≤ int ∧ U ≤ Sequence[T] → U ≤ Sequence[int]` to exist in the
            // sequent map. It doesn't, and so we currently produce `U ≤ Unknown` in this case.
            &mut |constraint| {
                if constraint.typevar(db).identity(db) == bound_typevar {
                    return true;
                }
                if any_over_type(db, constraint.lower(db), &mentions_typevar, false) {
                    return true;
                }
                if any_over_type(db, constraint.upper(db), &mentions_typevar, false) {
                    return true;
                }
                false
            },
            map,
            &mut path,
        )
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn retain_one(self, db: &'db dyn Db, bound_typevar: BoundTypeVarIdentity<'db>) -> Node<'db> {
        let map = self.sequent_map(db);
        let mut path = PathAssignments::default();
        self.abstract_one_inner(
            db,
            // Remove any node that constrains some other typevar than `bound_typevar`, and any
            // node that constrains `bound_typevar` with a lower/upper bound of some other typevar.
            // (For the latter, if there are any derived facts that we can infer from the typevar
            // bound, those will be automatically added to the result.)
            &mut |constraint| {
                if constraint.typevar(db).identity(db) != bound_typevar {
                    return true;
                }
                if constraint.lower(db).has_typevar(db) || constraint.upper(db).has_typevar(db) {
                    return true;
                }
                false
            },
            map,
            &mut path,
        )
    }

    fn abstract_one_inner(
        self,
        db: &'db dyn Db,
        should_remove: &mut dyn FnMut(ConstrainedTypeVar<'db>) -> bool,
        map: &SequentMap<'db>,
        path: &mut PathAssignments<'db>,
    ) -> Node<'db> {
        let self_constraint = self.constraint(db);
        let self_source_order = self.source_order(db);
        if should_remove(self_constraint) {
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
            let self_source_order = self.source_order(db);
            let if_true = path
                .walk_edge(
                    db,
                    map,
                    self_constraint.when_true(),
                    self_source_order,
                    |path, new_range| {
                        let branch =
                            self.if_true(db)
                                .abstract_one_inner(db, should_remove, map, path);
                        path.assignments[new_range]
                            .iter()
                            .filter(|(assignment, _)| {
                                // Don't add back any derived facts if they are ones that we would have
                                // removed!
                                !should_remove(assignment.constraint())
                            })
                            .fold(branch, |branch, (assignment, source_order)| {
                                branch.and(
                                    db,
                                    Node::new_satisfied_constraint(db, *assignment, *source_order),
                                )
                            })
                    },
                )
                .unwrap_or(Node::AlwaysFalse);
            let if_false = path
                .walk_edge(
                    db,
                    map,
                    self_constraint.when_false(),
                    self_source_order,
                    |path, new_range| {
                        let branch =
                            self.if_false(db)
                                .abstract_one_inner(db, should_remove, map, path);
                        path.assignments[new_range]
                            .iter()
                            .filter(|(assignment, _)| {
                                // Don't add back any derived facts if they are ones that we would have
                                // removed!
                                !should_remove(assignment.constraint())
                            })
                            .fold(branch, |branch, (assignment, source_order)| {
                                branch.and(
                                    db,
                                    Node::new_satisfied_constraint(db, *assignment, *source_order),
                                )
                            })
                    },
                )
                .unwrap_or(Node::AlwaysFalse);
            if_true.or(db, if_false)
        } else {
            // Otherwise, we abstract the if_false/if_true edges recursively.
            let if_true = path
                .walk_edge(
                    db,
                    map,
                    self_constraint.when_true(),
                    self_source_order,
                    |path, _| {
                        self.if_true(db)
                            .abstract_one_inner(db, should_remove, map, path)
                    },
                )
                .unwrap_or(Node::AlwaysFalse);
            let if_false = path
                .walk_edge(
                    db,
                    map,
                    self_constraint.when_false(),
                    self_source_order,
                    |path, _| {
                        self.if_false(db)
                            .abstract_one_inner(db, should_remove, map, path)
                    },
                )
                .unwrap_or(Node::AlwaysFalse);
            // NB: We cannot use `Node::new` here, because the recursive calls might introduce new
            // derived constraints into the result, and those constraints might appear before this
            // one in the BDD ordering.
            Node::new_constraint(db, self_constraint, self.source_order(db))
                .ite(db, if_true, if_false)
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn restrict_one(
        self,
        db: &'db dyn Db,
        assignment: ConstraintAssignment<'db>,
    ) -> (Node<'db>, bool) {
        // If this node's variable is larger than the assignment's variable, then we have reached a
        // point in the BDD where the assignment can no longer affect the result,
        // and we can return early.
        let self_constraint = self.constraint(db);
        if assignment.constraint().ordering(db) < self_constraint.ordering(db) {
            return (Node::Interior(self), false);
        }

        // Otherwise, check if this node's variable is in the assignment. If so, substitute the
        // variable by replacing this node with its if_false/if_true edge, accordingly.
        if assignment == self_constraint.when_true() {
            (self.if_true(db), true)
        } else if assignment == self_constraint.when_false() {
            (self.if_false(db), true)
        } else {
            let (if_true, found_in_true) = self.if_true(db).restrict_one(db, assignment);
            let (if_false, found_in_false) = self.if_false(db).restrict_one(db, assignment);
            (
                Node::new(
                    db,
                    self_constraint,
                    if_true,
                    if_false,
                    self.source_order(db),
                ),
                found_in_true || found_in_false,
            )
        }
    }

    /// Returns a sequent map for this BDD, which records the relationships between the constraints
    /// that appear in the BDD.
    #[salsa::tracked(
        returns(ref),
        cycle_initial=sequent_map_cycle_initial,
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn sequent_map(self, db: &'db dyn Db) -> SequentMap<'db> {
        tracing::trace!(
            target: "ty_python_semantic::types::constraints::SequentMap",
            constraints = %Node::Interior(self).display(db),
            "create sequent map",
        );

        // Sort the constraints in this BDD by their `source_order`s before adding them to the
        // sequent map. This ensures that constraints appear in the sequent map in a stable order.
        // The constraints mentioned in a BDD should all have distinct `source_order`s, so an
        // unstable sort is fine.
        let mut constraints = Vec::new();
        Node::Interior(self).for_each_constraint(db, &mut |constraint, source_order| {
            constraints.push((constraint, source_order));
        });
        constraints.sort_unstable_by_key(|(_, source_order)| *source_order);

        let mut map = SequentMap::default();
        for (constraint, _) in constraints {
            map.add(db, constraint);
        }
        map
    }

    /// Returns a simplified version of a BDD.
    ///
    /// This is calculated by looking at the relationships that exist between the constraints that
    /// are mentioned in the BDD. For instance, if one constraint implies another (`x → y`), then
    /// `x ∧ ¬y` is not a valid input, and we can rewrite any occurrences of `x ∨ y` into `y`.
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn simplify(self, db: &'db dyn Db) -> Node<'db> {
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
        Node::Interior(self).for_each_constraint(db, &mut |constraint, source_order| {
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
        let mut simplified = Node::Interior(self);
        let mut next_source_order = self.max_source_order(db) + 1;
        while let Some((left_constraint, right_constraint)) = to_visit.pop() {
            let left_source_order = source_orders[&left_constraint];
            let right_source_order = source_orders[&right_constraint];

            // If the constraints refer to different typevars, the only simplifications we can make
            // are of the form `S ≤ T ∧ T ≤ int → S ≤ int`.
            let left_typevar = left_constraint.typevar(db);
            let right_typevar = right_constraint.typevar(db);
            if !left_typevar.is_same_typevar_as(db, right_typevar) {
                // We've structured our constraints so that a typevar's upper/lower bound can only
                // be another typevar if the bound is "later" in our arbitrary ordering. That means
                // we only have to check this pair of constraints in one direction — though we do
                // have to figure out which of the two typevars is constrained, and which one is
                // the upper/lower bound.
                let (bound_typevar, bound_constraint, constrained_typevar, constrained_constraint) =
                    if left_typevar.can_be_bound_for(db, right_typevar) {
                        (
                            left_typevar,
                            left_constraint,
                            right_typevar,
                            right_constraint,
                        )
                    } else {
                        (
                            right_typevar,
                            right_constraint,
                            left_typevar,
                            left_constraint,
                        )
                    };

                // We then look for cases where the "constrained" typevar's upper and/or lower
                // bound matches the "bound" typevar. If so, we're going to add an implication to
                // the constraint set that replaces the upper/lower bound that matched with the
                // bound constraint's corresponding bound.
                let (new_lower, new_upper) = match (
                    constrained_constraint.lower(db),
                    constrained_constraint.upper(db),
                ) {
                    // (B ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ BU)
                    (Type::TypeVar(constrained_lower), Type::TypeVar(constrained_upper))
                        if constrained_lower.is_same_typevar_as(db, bound_typevar)
                            && constrained_upper.is_same_typevar_as(db, bound_typevar) =>
                    {
                        (bound_constraint.lower(db), bound_constraint.upper(db))
                    }

                    // (CL ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (CL ≤ C ≤ BU)
                    (constrained_lower, Type::TypeVar(constrained_upper))
                        if constrained_upper.is_same_typevar_as(db, bound_typevar) =>
                    {
                        (constrained_lower, bound_constraint.upper(db))
                    }

                    // (B ≤ C ≤ CU) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ CU)
                    (Type::TypeVar(constrained_lower), constrained_upper)
                        if constrained_lower.is_same_typevar_as(db, bound_typevar) =>
                    {
                        (bound_constraint.lower(db), constrained_upper)
                    }

                    _ => continue,
                };

                let new_constraint =
                    ConstrainedTypeVar::new(db, constrained_typevar, new_lower, new_upper);
                if seen_constraints.contains(&new_constraint) {
                    continue;
                }
                let new_node = Node::new_constraint(db, new_constraint, next_source_order);
                next_source_order += 1;
                let positive_left_node = Node::new_satisfied_constraint(
                    db,
                    left_constraint.when_true(),
                    left_source_order,
                );
                let positive_right_node = Node::new_satisfied_constraint(
                    db,
                    right_constraint.when_true(),
                    right_source_order,
                );
                let lhs = positive_left_node.and(db, positive_right_node);
                let intersection = new_node.ite(db, lhs, Node::AlwaysFalse);
                simplified = simplified.and(db, intersection);
                continue;
            }

            // From here on out we know that both constraints constrain the same typevar. The
            // clause above will propagate all that we know about the current typevar relative to
            // other typevars, producing constraints on this typevar that have concrete lower/upper
            // bounds. That means we can skip the simplifications below if any bound is another
            // typevar.
            if left_constraint.lower(db).is_type_var()
                || left_constraint.upper(db).is_type_var()
                || right_constraint.lower(db).is_type_var()
                || right_constraint.upper(db).is_type_var()
            {
                continue;
            }

            // Containment: The range of one constraint might completely contain the range of the
            // other. If so, there are several potential simplifications.
            let larger_smaller = if left_constraint.implies(db, right_constraint) {
                Some((
                    right_constraint,
                    right_source_order,
                    left_constraint,
                    left_source_order,
                ))
            } else if right_constraint.implies(db, left_constraint) {
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
                    db,
                    larger_constraint.when_true(),
                    larger_source_order,
                );
                let negative_larger_node = Node::new_satisfied_constraint(
                    db,
                    larger_constraint.when_false(),
                    larger_source_order,
                );

                // larger ∨ smaller = larger
                simplified = simplified.substitute_union(
                    db,
                    larger_constraint.when_true(),
                    larger_source_order,
                    smaller_constraint.when_true(),
                    smaller_source_order,
                    positive_larger_node,
                );

                // ¬larger ∧ ¬smaller = ¬larger
                simplified = simplified.substitute_intersection(
                    db,
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
                    larger_constraint.when_false(),
                    larger_source_order,
                    smaller_constraint.when_true(),
                    smaller_source_order,
                    Node::AlwaysFalse,
                );

                // larger ∨ ¬smaller = true
                // (larger fills in everything that's missing in ¬smaller)
                simplified = simplified.substitute_union(
                    db,
                    larger_constraint.when_true(),
                    larger_source_order,
                    smaller_constraint.when_false(),
                    smaller_source_order,
                    Node::AlwaysTrue,
                );
            }

            // There are some simplifications we can make when the intersection of the two
            // constraints is empty, and others that we can make when the intersection is
            // non-empty.
            match left_constraint.intersect(db, right_constraint) {
                IntersectionResult::Simplified(intersection_constraint) => {
                    let intersection_constraint = intersection_constraint.normalized(db);

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
                        db,
                        intersection_constraint.when_true(),
                        next_source_order,
                    );
                    let negative_intersection_node = Node::new_satisfied_constraint(
                        db,
                        intersection_constraint.when_false(),
                        next_source_order,
                    );
                    next_source_order += 1;

                    let positive_left_node = Node::new_satisfied_constraint(
                        db,
                        left_constraint.when_true(),
                        left_source_order,
                    );
                    let negative_left_node = Node::new_satisfied_constraint(
                        db,
                        left_constraint.when_false(),
                        left_source_order,
                    );

                    let positive_right_node = Node::new_satisfied_constraint(
                        db,
                        right_constraint.when_true(),
                        right_source_order,
                    );
                    let negative_right_node = Node::new_satisfied_constraint(
                        db,
                        right_constraint.when_false(),
                        right_source_order,
                    );

                    // left ∧ right = intersection
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        positive_intersection_node,
                    );

                    // ¬left ∨ ¬right = ¬intersection
                    simplified = simplified.substitute_union(
                        db,
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
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        positive_left_node.and(db, negative_intersection_node),
                    );

                    // ¬left ∧ right = ¬intersection ∧ right
                    // (save as above but reversed)
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        positive_right_node.and(db, negative_intersection_node),
                    );

                    // left ∨ ¬right = intersection ∨ ¬right
                    // (clip the positive constraint to the smallest range that actually adds
                    // something to the negative constraint)
                    simplified = simplified.substitute_union(
                        db,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        negative_right_node.or(db, positive_intersection_node),
                    );

                    // ¬left ∨ right = ¬left ∨ intersection
                    // (save as above but reversed)
                    simplified = simplified.substitute_union(
                        db,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        negative_left_node.or(db, positive_intersection_node),
                    );
                }

                // If the intersection doesn't simplify to a single clause, we shouldn't update the
                // BDD.
                IntersectionResult::CannotSimplify => {}

                IntersectionResult::Disjoint => {
                    // All of the below hold because we just proved that the intersection of left
                    // and right is empty.

                    let positive_left_node = Node::new_satisfied_constraint(
                        db,
                        left_constraint.when_true(),
                        left_source_order,
                    );
                    let positive_right_node = Node::new_satisfied_constraint(
                        db,
                        right_constraint.when_true(),
                        right_source_order,
                    );

                    // left ∧ right = false
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_true(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        Node::AlwaysFalse,
                    );

                    // ¬left ∨ ¬right = true
                    simplified = simplified.substitute_union(
                        db,
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_false(),
                        right_source_order,
                        Node::AlwaysTrue,
                    );

                    // left ∧ ¬right = left
                    // (there is nothing in the hole of ¬right that overlaps with left)
                    simplified = simplified.substitute_intersection(
                        db,
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
                        left_constraint.when_false(),
                        left_source_order,
                        right_constraint.when_true(),
                        right_source_order,
                        positive_right_node,
                    );
                }
            }
        }

        simplified
    }
}

fn sequent_map_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: InteriorNode<'db>,
) -> SequentMap<'db> {
    SequentMap::default()
}

/// An assignment of one BDD variable to either `true` or `false`. (When evaluating a BDD, we
/// must provide an assignment for each variable present in the BDD.)
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Update)]
pub(crate) enum ConstraintAssignment<'db> {
    Positive(ConstrainedTypeVar<'db>),
    Negative(ConstrainedTypeVar<'db>),
}

impl<'db> ConstraintAssignment<'db> {
    fn constraint(self) -> ConstrainedTypeVar<'db> {
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
    fn implies(self, db: &'db dyn Db, other: Self) -> bool {
        match (self, other) {
            // For two positive constraints, one range has to fully contain the other; the smaller
            // constraint implies the larger.
            //
            //     ....|----other-----|....
            //     ......|---self---|......
            (
                ConstraintAssignment::Positive(self_constraint),
                ConstraintAssignment::Positive(other_constraint),
            ) => self_constraint.implies(db, other_constraint),

            // For two negative constraints, one range has to fully contain the other; the ranges
            // represent "holes", though, so the constraint with the larger range implies the one
            // with the smaller.
            //
            //     |-----|...other...|-----|
            //     |---|.....self......|---|
            (
                ConstraintAssignment::Negative(self_constraint),
                ConstraintAssignment::Negative(other_constraint),
            ) => other_constraint.implies(db, self_constraint),

            // For a positive and negative constraint, the ranges have to be disjoint, and the
            // positive range implies the negative range.
            //
            //     |---------------|...self...|---|
            //     ..|---other---|................|
            (
                ConstraintAssignment::Positive(self_constraint),
                ConstraintAssignment::Negative(other_constraint),
            ) => self_constraint
                .intersect(db, other_constraint)
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

    fn display(self, db: &'db dyn Db) -> impl Display {
        struct DisplayConstraintAssignment<'db> {
            constraint: ConstraintAssignment<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplayConstraintAssignment<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.constraint {
                    ConstraintAssignment::Positive(constraint) => {
                        constraint.display(self.db).fmt(f)
                    }
                    ConstraintAssignment::Negative(constraint) => {
                        constraint.display_negated(self.db).fmt(f)
                    }
                }
            }
        }

        DisplayConstraintAssignment {
            constraint: self,
            db,
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
#[derive(Debug, Default, Eq, PartialEq, get_size2::GetSize, salsa::Update)]
struct SequentMap<'db> {
    /// Sequents of the form `¬C₁ → false`
    single_tautologies: FxHashSet<ConstrainedTypeVar<'db>>,
    /// Sequents of the form `C₁ ∧ C₂ → false`
    pair_impossibilities: FxHashSet<(ConstrainedTypeVar<'db>, ConstrainedTypeVar<'db>)>,
    /// Sequents of the form `C₁ ∧ C₂ → D`
    pair_implications: FxHashMap<
        (ConstrainedTypeVar<'db>, ConstrainedTypeVar<'db>),
        FxOrderSet<ConstrainedTypeVar<'db>>,
    >,
    /// Sequents of the form `C → D`
    single_implications: FxHashMap<ConstrainedTypeVar<'db>, FxOrderSet<ConstrainedTypeVar<'db>>>,
    /// Constraints that we have already processed
    processed: FxHashSet<ConstrainedTypeVar<'db>>,
    /// Constraints that enqueued to be processed
    enqueued: Vec<ConstrainedTypeVar<'db>>,
}

impl<'db> SequentMap<'db> {
    fn add(&mut self, db: &'db dyn Db, constraint: ConstrainedTypeVar<'db>) {
        self.enqueue_constraint(constraint);

        while let Some(constraint) = self.enqueued.pop() {
            // If we've already processed this constraint, we can skip it.
            if !self.processed.insert(constraint) {
                continue;
            }

            // First see if we can create any sequents from the constraint on its own.
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                constraint = %constraint.display(db),
                "add sequents for constraint",
            );
            self.add_sequents_for_single(db, constraint);

            // Then check this constraint against all of the other ones we've seen so far, seeing
            // if they're related to each other.
            let processed = std::mem::take(&mut self.processed);
            for other in &processed {
                if constraint != *other {
                    tracing::trace!(
                        target: "ty_python_semantic::types::constraints::SequentMap",
                        left = %constraint.display(db),
                        right = %other.display(db),
                        "add sequents for constraint pair",
                    );
                    self.add_sequents_for_pair(db, constraint, *other);
                }
            }
            self.processed = processed;
        }
    }

    fn enqueue_constraint(&mut self, constraint: ConstrainedTypeVar<'db>) {
        // If we've already processed this constraint, we can skip it.
        if self.processed.contains(&constraint) {
            return;
        }
        self.enqueued.push(constraint);
    }

    fn pair_key(
        db: &'db dyn Db,
        ante1: ConstrainedTypeVar<'db>,
        ante2: ConstrainedTypeVar<'db>,
    ) -> (ConstrainedTypeVar<'db>, ConstrainedTypeVar<'db>) {
        if ante1.ordering(db) < ante2.ordering(db) {
            (ante1, ante2)
        } else {
            (ante2, ante1)
        }
    }

    fn add_single_tautology(&mut self, db: &'db dyn Db, ante: ConstrainedTypeVar<'db>) {
        if self.single_tautologies.insert(ante) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                sequent = %format_args!("¬{} → false", ante.display(db)),
                "add sequent",
            );
        }
    }

    fn add_pair_impossibility(
        &mut self,
        db: &'db dyn Db,
        ante1: ConstrainedTypeVar<'db>,
        ante2: ConstrainedTypeVar<'db>,
    ) {
        if self
            .pair_impossibilities
            .insert(Self::pair_key(db, ante1, ante2))
        {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                sequent = %format_args!("{} ∧ {} → false", ante1.display(db), ante2.display(db)),
                "add sequent",
            );
        }
    }

    fn add_pair_implication(
        &mut self,
        db: &'db dyn Db,
        ante1: ConstrainedTypeVar<'db>,
        ante2: ConstrainedTypeVar<'db>,
        post: ConstrainedTypeVar<'db>,
    ) {
        // If either antecedent implies the consequent on its own, this new sequent is redundant.
        if ante1.implies(db, post) || ante2.implies(db, post) {
            return;
        }
        if self
            .pair_implications
            .entry(Self::pair_key(db, ante1, ante2))
            .or_default()
            .insert(post)
        {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                sequent = %format_args!(
                    "{} ∧ {} → {}",
                    ante1.display(db),
                    ante2.display(db),
                    post.display(db),
                ),
                "add sequent",
            );
        }
    }

    fn add_single_implication(
        &mut self,
        db: &'db dyn Db,
        ante: ConstrainedTypeVar<'db>,
        post: ConstrainedTypeVar<'db>,
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
                    ante.display(db),
                    post.display(db),
                ),
                "add sequent",
            );
        }
    }

    fn add_sequents_for_single(&mut self, db: &'db dyn Db, constraint: ConstrainedTypeVar<'db>) {
        // If this constraint binds its typevar to `Never ≤ T ≤ object`, then the typevar can take
        // on any type, and the constraint is always satisfied.
        let lower = constraint.lower(db);
        let upper = constraint.upper(db);
        if lower.is_never() && upper.is_object() {
            self.add_single_tautology(db, constraint);
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
                    ConstrainedTypeVar::new(db, lower_typevar, Type::Never, upper)
                } else {
                    return;
                }
            }

            // Case 2
            (Type::TypeVar(lower_typevar), _) => {
                ConstrainedTypeVar::new(db, lower_typevar, Type::Never, upper)
            }

            // Case 3
            (_, Type::TypeVar(upper_typevar)) => {
                ConstrainedTypeVar::new(db, upper_typevar, lower, Type::object())
            }

            _ => return,
        };

        self.add_single_implication(db, constraint, post_constraint);
        self.enqueue_constraint(post_constraint);
    }

    fn add_sequents_for_pair(
        &mut self,
        db: &'db dyn Db,
        left_constraint: ConstrainedTypeVar<'db>,
        right_constraint: ConstrainedTypeVar<'db>,
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
        let left_typevar = left_constraint.typevar(db);
        let right_typevar = right_constraint.typevar(db);
        if !left_typevar.is_same_typevar_as(db, right_typevar) {
            self.add_mutual_sequents_for_different_typevars(db, left_constraint, right_constraint);
        } else if left_constraint.lower(db).is_type_var()
            || left_constraint.upper(db).is_type_var()
            || right_constraint.lower(db).is_type_var()
            || right_constraint.upper(db).is_type_var()
        {
            self.add_mutual_sequents_for_same_typevars(db, left_constraint, right_constraint);
        } else {
            self.add_concrete_sequents(db, left_constraint, right_constraint);
        }
    }

    fn add_mutual_sequents_for_different_typevars(
        &mut self,
        db: &'db dyn Db,
        left_constraint: ConstrainedTypeVar<'db>,
        right_constraint: ConstrainedTypeVar<'db>,
    ) {
        // We've structured our constraints so that a typevar's upper/lower bound can only
        // be another typevar if the bound is "later" in our arbitrary ordering. That means
        // we only have to check this pair of constraints in one direction — though we do
        // have to figure out which of the two typevars is constrained, and which one is
        // the upper/lower bound.
        let left_typevar = left_constraint.typevar(db);
        let right_typevar = right_constraint.typevar(db);
        let (bound_typevar, bound_constraint, constrained_typevar, constrained_constraint) =
            if left_typevar.can_be_bound_for(db, right_typevar) {
                (
                    left_typevar,
                    left_constraint,
                    right_typevar,
                    right_constraint,
                )
            } else {
                (
                    right_typevar,
                    right_constraint,
                    left_typevar,
                    left_constraint,
                )
            };

        // We then look for cases where the "constrained" typevar's upper and/or lower bound
        // matches the "bound" typevar. If so, we're going to add an implication sequent that
        // replaces the upper/lower bound that matched with the bound constraint's corresponding
        // bound.
        let (new_lower, new_upper) = match (
            constrained_constraint.lower(db),
            constrained_constraint.upper(db),
        ) {
            // (B ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ BU)
            (Type::TypeVar(constrained_lower), Type::TypeVar(constrained_upper))
                if constrained_lower.is_same_typevar_as(db, bound_typevar)
                    && constrained_upper.is_same_typevar_as(db, bound_typevar) =>
            {
                (bound_constraint.lower(db), bound_constraint.upper(db))
            }

            // (CL ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (CL ≤ C ≤ BU)
            (constrained_lower, Type::TypeVar(constrained_upper))
                if constrained_upper.is_same_typevar_as(db, bound_typevar) =>
            {
                (constrained_lower, bound_constraint.upper(db))
            }

            // (B ≤ C ≤ CU) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ CU)
            (Type::TypeVar(constrained_lower), constrained_upper)
                if constrained_lower.is_same_typevar_as(db, bound_typevar) =>
            {
                (bound_constraint.lower(db), constrained_upper)
            }

            // (CL ≤ C ≤ pivot) ∧ (pivot ≤ B ≤ BU) → (CL ≤ C ≤ B)
            (constrained_lower, constrained_upper)
                if constrained_upper == bound_constraint.lower(db)
                    && !constrained_upper.is_never()
                    && !constrained_upper.is_object() =>
            {
                (constrained_lower, Type::TypeVar(bound_typevar))
            }

            // (pivot ≤ C ≤ CU) ∧ (BL ≤ B ≤ pivot) → (B ≤ C ≤ CU)
            (constrained_lower, constrained_upper)
                if constrained_lower == bound_constraint.upper(db)
                    && !constrained_lower.is_never()
                    && !constrained_lower.is_object() =>
            {
                (Type::TypeVar(bound_typevar), constrained_upper)
            }

            _ => return,
        };

        let post_constraint =
            ConstrainedTypeVar::new(db, constrained_typevar, new_lower, new_upper);
        self.add_pair_implication(db, left_constraint, right_constraint, post_constraint);
        self.enqueue_constraint(post_constraint);
    }

    fn add_mutual_sequents_for_same_typevars(
        &mut self,
        db: &'db dyn Db,
        left_constraint: ConstrainedTypeVar<'db>,
        right_constraint: ConstrainedTypeVar<'db>,
    ) {
        let mut try_one_direction =
            |left_constraint: ConstrainedTypeVar<'db>,
             right_constraint: ConstrainedTypeVar<'db>| {
                let left_lower = left_constraint.lower(db);
                let left_upper = left_constraint.upper(db);
                let right_lower = right_constraint.lower(db);
                let right_upper = right_constraint.upper(db);
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
                    ConstrainedTypeVar::new(db, bound_typevar, right_lower, right_upper)
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
                self.add_pair_implication(db, left_constraint, right_constraint, post_constraint);
                self.enqueue_constraint(post_constraint);
            };

        try_one_direction(left_constraint, right_constraint);
        try_one_direction(right_constraint, left_constraint);
    }

    fn add_concrete_sequents(
        &mut self,
        db: &'db dyn Db,
        left_constraint: ConstrainedTypeVar<'db>,
        right_constraint: ConstrainedTypeVar<'db>,
    ) {
        // These might seem redundant with the intersection check below, since `a → b` means that
        // `a ∧ b = a`. But we are not normalizing constraint bounds, and these clauses help us
        // identify constraints that are identical besides e.g. ordering of union/intersection
        // elements. (For instance, when processing `T ≤ τ₁ & τ₂` and `T ≤ τ₂ & τ₁`, these clauses
        // would add sequents for `(T ≤ τ₁ & τ₂) → (T ≤ τ₂ & τ₁)` and vice versa.)
        if left_constraint.implies(db, right_constraint) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left_constraint.display(db),
                right = %right_constraint.display(db),
                "left implies right",
            );
            self.add_single_implication(db, left_constraint, right_constraint);
        }
        if right_constraint.implies(db, left_constraint) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left_constraint.display(db),
                right = %right_constraint.display(db),
                "right implies left",
            );
            self.add_single_implication(db, right_constraint, left_constraint);
        }

        match left_constraint.intersect(db, right_constraint) {
            IntersectionResult::Simplified(intersection_constraint) => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::SequentMap",
                    left = %left_constraint.display(db),
                    right = %right_constraint.display(db),
                    intersection = %intersection_constraint.display(db),
                    "left and right overlap",
                );
                self.add_pair_implication(
                    db,
                    left_constraint,
                    right_constraint,
                    intersection_constraint,
                );
                self.add_single_implication(db, intersection_constraint, left_constraint);
                self.add_single_implication(db, intersection_constraint, right_constraint);
                self.enqueue_constraint(intersection_constraint);
            }

            // The sequent map only needs to include constraints that might appear in a BDD. If the
            // intersection does not collapse to a single constraint, then there's no new
            // constraint that we need to add to the sequent map.
            IntersectionResult::CannotSimplify => {}

            IntersectionResult::Disjoint => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::SequentMap",
                    left = %left_constraint.display(db),
                    right = %right_constraint.display(db),
                    "left and right are disjoint",
                );
                self.add_pair_impossibility(db, left_constraint, right_constraint);
            }
        }
    }

    #[expect(dead_code)] // Keep this around for debugging purposes
    fn display<'a>(&'a self, db: &'db dyn Db, prefix: &'a dyn Display) -> impl Display + 'a {
        struct DisplaySequentMap<'a, 'db> {
            map: &'a SequentMap<'db>,
            prefix: &'a dyn Display,
            db: &'db dyn Db,
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
                        ante1.display(self.db),
                        ante2.display(self.db),
                    )?;
                }

                for ((ante1, ante2), posts) in &self.map.pair_implications {
                    for post in posts {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} ∧ {} → {}",
                            ante1.display(self.db),
                            ante2.display(self.db),
                            post.display(self.db),
                        )?;
                    }
                }

                for (ante, posts) in &self.map.single_implications {
                    for post in posts {
                        maybe_write_prefix(f)?;
                        write!(f, "{} → {}", ante.display(self.db), post.display(self.db))?;
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
        }
    }
}

/// The collection of constraints that we know to be true or false at a certain point when
/// traversing a BDD.
#[derive(Debug, Default)]
pub(crate) struct PathAssignments<'db> {
    assignments: FxOrderMap<ConstraintAssignment<'db>, usize>,
}

impl<'db> PathAssignments<'db> {
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
    fn walk_edge<R>(
        &mut self,
        db: &'db dyn Db,
        map: &SequentMap<'db>,
        assignment: ConstraintAssignment<'db>,
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
                self.assignments[..start].iter().map(|(assignment, _)| assignment.display(db)).format(", "),
            ),
            edge = %assignment.display(db),
            "walk edge",
        );
        let found_conflict = self.add_assignment(db, map, assignment, source_order);
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
                    self.assignments[start..].iter().map(|(assignment, _)| assignment.display(db)).format(", "),
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

    pub(crate) fn positive_constraints(
        &self,
    ) -> impl Iterator<Item = (ConstrainedTypeVar<'db>, usize)> + '_ {
        self.assignments
            .iter()
            .filter_map(|(assignment, source_order)| match assignment {
                ConstraintAssignment::Positive(constraint) => Some((*constraint, *source_order)),
                ConstraintAssignment::Negative(_) => None,
            })
    }

    fn assignment_holds(&self, assignment: ConstraintAssignment<'db>) -> bool {
        self.assignments.contains_key(&assignment)
    }

    /// Adds a new assignment, along with any derived information that we can infer from the new
    /// assignment combined with the assignments we've already seen. If any of this causes the path
    /// to become invalid, due to a contradiction, returns a [`PathAssignmentConflict`] error.
    fn add_assignment(
        &mut self,
        db: &'db dyn Db,
        map: &SequentMap<'db>,
        assignment: ConstraintAssignment<'db>,
        source_order: usize,
    ) -> Result<(), PathAssignmentConflict> {
        // First add this assignment. If it causes a conflict, return that as an error. If we've
        // already know this assignment holds, just return.
        if self.assignments.contains_key(&assignment.negated()) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::PathAssignment",
                assignment = %assignment.display(db),
                facts = %format_args!(
                    "[{}]",
                    self.assignments.iter().map(|(assignment, _)| assignment.display(db)).format(", "),
                ),
                "found contradiction",
            );
            return Err(PathAssignmentConflict);
        }
        if self.assignments.insert(assignment, source_order).is_some() {
            return Ok(());
        }

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

        for ante in &map.single_tautologies {
            if self.assignment_holds(ante.when_false()) {
                // The sequent map says (ante1) is always true, and the current path asserts that
                // it's false.
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    ante = %ante.display(db),
                    facts = %format_args!(
                        "[{}]",
                        self.assignments.iter().map(|(assignment, _)| assignment.display(db)).format(", "),
                    ),
                    "found contradiction",
                );
                return Err(PathAssignmentConflict);
            }
        }

        for (ante1, ante2) in &map.pair_impossibilities {
            if self.assignment_holds(ante1.when_true()) && self.assignment_holds(ante2.when_true())
            {
                // The sequent map says (ante1 ∧ ante2) is an impossible combination, and the
                // current path asserts that both are true.
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::PathAssignment",
                    ante1 = %ante1.display(db),
                    ante2 = %ante2.display(db),
                    facts = %format_args!(
                        "[{}]",
                        self.assignments.iter().map(|(assignment, _)| assignment.display(db)).format(", "),
                    ),
                    "found contradiction",
                );
                return Err(PathAssignmentConflict);
            }
        }

        for ((ante1, ante2), posts) in &map.pair_implications {
            for post in posts {
                if self.assignment_holds(ante1.when_true())
                    && self.assignment_holds(ante2.when_true())
                {
                    self.add_assignment(db, map, post.when_true(), source_order)?;
                }
            }
        }

        for (ante, posts) in &map.single_implications {
            for post in posts {
                if self.assignment_holds(ante.when_true()) {
                    self.add_assignment(db, map, post.when_true(), source_order)?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
struct PathAssignmentConflict;

/// A single clause in the DNF representation of a BDD
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SatisfiedClause<'db> {
    constraints: Vec<ConstraintAssignment<'db>>,
}

impl<'db> SatisfiedClause<'db> {
    fn push(&mut self, constraint: ConstraintAssignment<'db>) {
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
    fn remove_prefix(&mut self, prefix: &SatisfiedClause<'db>) -> bool {
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
    fn simplify(&mut self, db: &'db dyn Db) -> bool {
        let mut changes_made = false;
        let mut i = 0;
        // Loop through each constraint, comparing it with any constraints that appear later in the
        // list.
        'outer: while i < self.constraints.len() {
            let mut j = i + 1;
            while j < self.constraints.len() {
                if self.constraints[j].implies(db, self.constraints[i]) {
                    // If constraint `i` is removed, then we don't need to compare it with any
                    // later constraints in the list. Note that we continue the outer loop, instead
                    // of breaking from the inner loop, so that we don't bump index `i` below.
                    // (We'll have swapped another element into place at that index, and want to
                    // make sure that we process it.)
                    self.constraints.swap_remove(i);
                    changes_made = true;
                    continue 'outer;
                } else if self.constraints[i].implies(db, self.constraints[j]) {
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

    fn display(&self, db: &'db dyn Db) -> String {
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
                ConstraintAssignment::Positive(constraint) => constraint.display(db).to_string(),
                ConstraintAssignment::Negative(constraint) => {
                    constraint.display_negated(db).to_string()
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
struct SatisfiedClauses<'db> {
    clauses: Vec<SatisfiedClause<'db>>,
}

impl<'db> SatisfiedClauses<'db> {
    fn push(&mut self, clause: SatisfiedClause<'db>) {
        self.clauses.push(clause);
    }

    /// Simplifies the DNF representation, removing redundancies that do not change the underlying
    /// function. (This is used when displaying a BDD, to make sure that the representation that we
    /// show is as simple as possible while still producing the same results.)
    fn simplify(&mut self, db: &'db dyn Db) {
        // First simplify each clause individually, by removing constraints that are implied by
        // other constraints in the clause.
        for clause in &mut self.clauses {
            clause.simplify(db);
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

    fn display(&self, db: &'db dyn Db) -> String {
        // This is a bit heavy-handed, but we need to output the clauses in a consistent order
        // even though Salsa IDs are assigned non-deterministically. This Display output is only
        // used in test cases, so we don't need to over-optimize it.

        if self.clauses.is_empty() {
            return String::from("never");
        }
        let mut clauses: Vec<_> = self
            .clauses
            .iter()
            .map(|clause| clause.display(db))
            .collect();
        clauses.sort();
        clauses.join(" ∨ ")
    }
}

impl<'db> BoundTypeVarInstance<'db> {
    /// Returns the valid specializations of a typevar. This is used when checking a constraint set
    /// when this typevar is in inferable position, where we only need _some_ specialization to
    /// satisfy the constraint set.
    fn valid_specializations(self, db: &'db dyn Db) -> Node<'db> {
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
            None => Node::AlwaysTrue,
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                let bound = bound.top_materialization(db);
                ConstrainedTypeVar::new_node(db, self, Type::Never, bound)
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                let mut specializations = Node::AlwaysFalse;
                for constraint in constraints.elements(db) {
                    let constraint_lower = constraint.bottom_materialization(db);
                    let constraint_upper = constraint.top_materialization(db);
                    specializations = specializations.or_with_offset(
                        db,
                        ConstrainedTypeVar::new_node(db, self, constraint_lower, constraint_upper),
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
    fn required_specializations(self, db: &'db dyn Db) -> (Node<'db>, Vec<Node<'db>>) {
        // For upper bounds and constraints, we are free to choose any materialization that makes
        // the check succeed. In non-inferable positions, it is most helpful to choose a
        // materialization that is as restrictive as possible, since that minimizes the number of
        // valid specializations that must satisfy the check. We therefore take the bottom
        // materialization of the bound or constraints.
        match self.typevar(db).bound_or_constraints(db) {
            None => (Node::AlwaysTrue, Vec::new()),
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                let bound = bound.bottom_materialization(db);
                (
                    ConstrainedTypeVar::new_node(db, self, Type::Never, bound),
                    Vec::new(),
                )
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                let mut non_gradual_constraints = Node::AlwaysFalse;
                let mut gradual_constraints = Vec::new();
                for constraint in constraints.elements(db) {
                    let constraint_lower = constraint.bottom_materialization(db);
                    let constraint_upper = constraint.top_materialization(db);
                    let constraint =
                        ConstrainedTypeVar::new_node(db, self, constraint_lower, constraint_upper);
                    if constraint_lower == constraint_upper {
                        non_gradual_constraints =
                            non_gradual_constraints.or_with_offset(db, constraint);
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
    pub(crate) fn specialize_constrained(
        self,
        db: &'db dyn Db,
        constraints: ConstraintSet<'db>,
    ) -> Result<Specialization<'db>, ()> {
        tracing::trace!(
            target: "ty_python_semantic::types::constraints::specialize_constrained",
            generic_context = %self.display_full(db),
            constraints = %constraints.node.display(db),
            "create specialization for constraint set",
        );

        // If the constraint set is cyclic, don't even try to construct a specialization.
        if constraints.is_cyclic(db) {
            tracing::error!(
                target: "ty_python_semantic::types::constraints::specialize_constrained",
                constraints = %constraints.node.display(db),
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
            .fold(Node::AlwaysTrue, |constraints, bound_typevar| {
                constraints.and_with_offset(db, bound_typevar.valid_specializations(db))
            })
            .and_with_offset(db, constraints.node);
        tracing::trace!(
            target: "ty_python_semantic::types::constraints::specialize_constrained",
            valid = %abstracted.display(db),
            "limited to valid specializations",
        );

        // Then we find all of the "representative types" for each typevar in the constraint set.
        let mut error_occurred = false;
        let mut representatives = Vec::new();
        let types =
            self.variables(db).map(|bound_typevar| {
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
                    abstracted = %abstracted.retain_one(db, identity).display(db),
                    "find specialization for typevar",
                );
                representatives.clear();
                abstracted.find_representative_types(db, identity, |representative| {
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

    #[test]
    fn test_display_graph_output() {
        let expected = indoc! {r#"
            (T = str) 3/4
            ┡━₁ (T = bool) 4/4
            │   ┡━₁ (U = str) 1/2
            │   │   ┡━₁ (U = bool) 2/2
            │   │   │   ┡━₁ always
            │   │   │   └─₀ always
            │   │   └─₀ (U = bool) 2/2
            │   │       ┡━₁ always
            │   │       └─₀ never
            │   └─₀ (U = str) 1/2
            │       ┡━₁ (U = bool) 2/2
            │       │   ┡━₁ always
            │       │   └─₀ always
            │       └─₀ (U = bool) 2/2
            │           ┡━₁ always
            │           └─₀ never
            └─₀ (T = bool) 4/4
                ┡━₁ (U = str) 1/2
                │   ┡━₁ (U = bool) 2/2
                │   │   ┡━₁ always
                │   │   └─₀ always
                │   └─₀ (U = bool) 2/2
                │       ┡━₁ always
                │       └─₀ never
                └─₀ never
        "#}
        .trim_end();

        let db = setup_db();
        let t = BoundTypeVarInstance::synthetic(&db, "T", TypeVarVariance::Invariant);
        let u = BoundTypeVarInstance::synthetic(&db, "U", TypeVarVariance::Invariant);
        let bool_type = KnownClass::Bool.to_instance(&db);
        let str_type = KnownClass::Str.to_instance(&db);
        let t_str = ConstraintSet::range(&db, str_type, t, str_type);
        let t_bool = ConstraintSet::range(&db, bool_type, t, bool_type);
        let u_str = ConstraintSet::range(&db, str_type, u, str_type);
        let u_bool = ConstraintSet::range(&db, bool_type, u, bool_type);
        // Construct this in a different order than above to make the source_orders more
        // interesting.
        let constraints = (u_str.or(&db, || u_bool)).and(&db, || t_str.or(&db, || t_bool));
        let actual = constraints.node.display_graph(&db, &"").to_string();
        assert_eq!(actual, expected);
    }
}
