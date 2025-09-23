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
//! An individual constraint restricts the specialization of a single typevar. You can then build
//! up more complex constraint sets using union, intersection, and negation operations. We use a
//! disjunctive normal form (DNF) representation, just like we do for types: a [constraint
//! set][ConstraintSet] is the union of zero or more [clauses][ConstraintClause], each of which is
//! the intersection of zero or more [individual constraints][ConstrainedTypeVar]. Note that the
//! constraint set that contains no clauses is never satisfiable (`⋃ {} = 0`); and the constraint
//! set that contains a single clause, where that clause contains no constraints, is always
//! satisfiable (`⋃ {⋂ {}} = 1`).
//!
//! An individual constraint consists of a _positive range_ and zero or more _negative holes_. The
//! positive range and each negative hole consists of a lower and upper bound. A type is within a
//! lower and upper bound if it is a supertype of the lower bound and a subtype of the upper bound.
//! The typevar can specialize to any type that is within the positive range, and is not within any
//! of the negative holes. (You can think of the constraint as the set of types that are within the
//! positive range, with the negative holes "removed" from that set.)
//!
//! Note that all lower and upper bounds in a constraint must be fully static. We take the bottom
//! and top materializations of the types to remove any gradual forms if needed.
//!
//! NOTE: This module is currently in a transitional state. We've added the DNF [`ConstraintSet`]
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

use std::cmp::Ordering;
use std::fmt::Display;

use rustc_hash::FxHashSet;

use crate::Db;
use crate::types::{BoundTypeVarInstance, IntersectionType, Type, UnionType};

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
    /// [`is_always_satisfied`][ConstraintSet::is_always_satisfied] true, then the overall result
    /// must be as well, and we stop consuming elements from the iterator.
    fn when_any<'db>(
        self,
        db: &'db dyn Db,
        f: impl FnMut(T) -> ConstraintSet<'db>,
    ) -> ConstraintSet<'db>;

    /// Returns the constraints under which every element of the iterator holds.
    ///
    /// This method short-circuits; if we encounter any element that
    /// [`is_never_satisfied`][ConstraintSet::is_never_satisfied] true, then the overall result
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
            if result.union(db, &f(child)).is_always_satisfied() {
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
            if result.intersect(db, &f(child)).is_never_satisfied() {
                return result;
            }
        }
        result
    }
}

/// A set of constraints under which a type property holds.
///
/// We use a DNF representation, so a set contains a list of zero or more
/// [clauses][ConstraintClause], each of which is an intersection of zero or more
/// [constraints][ConstrainedTypeVar].
///
/// This is called a "set of constraint sets", and denoted _𝒮_, in [[POPL2015][]].
///
/// ### Invariants
///
/// - The clauses are simplified as much as possible — there are no two clauses in the set that can
///   be simplified into a single clause.
///
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct ConstraintSet<'db> {
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

    /// Returns whether this constraint set never holds
    pub(crate) fn is_never_satisfied(&self) -> bool {
        self.node.is_never_satisfied()
    }

    /// Returns whether this constraint set always holds
    pub(crate) fn is_always_satisfied(&self) -> bool {
        self.node.is_always_satisfied()
    }

    /// Updates this constraint set to hold the union of itself and another constraint set.
    pub(crate) fn union(&mut self, db: &'db dyn Db, other: &Self) -> &Self {
        self.node = self.node.or(db, other.node).simplify(db);
        self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    pub(crate) fn intersect(&mut self, db: &'db dyn Db, other: &Self) -> &Self {
        self.node = self.node.and(db, other.node).simplify(db);
        self
    }

    /// Returns the negation of this constraint set.
    pub(crate) fn negate(&self, db: &'db dyn Db) -> Self {
        Self {
            node: self.node.negate(db).simplify(db),
        }
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    pub(crate) fn and(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_never_satisfied() {
            self.intersect(db, &other());
        }
        self
    }

    /// Returns the union of this constraint set and another. The other constraint set is provided
    /// as a thunk, to implement short-circuiting: the thunk is not forced if the constraint set is
    /// already saturated.
    pub(crate) fn or(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_always_satisfied() {
            self.union(db, &other());
        }
        self
    }

    pub(crate) fn range(
        db: &'db dyn Db,
        lower: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        let lower = lower.bottom_materialization(db);
        let upper = upper.top_materialization(db);
        Self {
            node: RangeConstraint::new_node(db, lower, typevar, upper),
        }
    }

    pub(crate) fn negated_range(
        db: &'db dyn Db,
        lower: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Self {
        Self::range(db, lower, typevar, upper).negate(db)
    }

    pub(crate) fn display(&self, db: &'db dyn Db) -> impl Display {
        self.node.display(db)
    }
}

impl From<bool> for ConstraintSet<'_> {
    fn from(b: bool) -> Self {
        if b { Self::always() } else { Self::never() }
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub(crate) struct ConstrainedTypeVar<'db> {
    typevar: BoundTypeVarInstance<'db>,
    constraint: RangeConstraint<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ConstrainedTypeVar<'_> {}

#[salsa::tracked]
impl<'db> ConstrainedTypeVar<'db> {
    fn contains(self, db: &'db dyn Db, other: Self) -> bool {
        if self.typevar(db) != other.typevar(db) {
            return false;
        }
        self.constraint(db).contains(db, other.constraint(db))
    }

    fn display(self, db: &'db dyn Db) -> impl Display {
        self.constraint(db)
            .display(db, self.typevar(db).display(db))
    }

    fn display_negated(self, db: &'db dyn Db) -> impl Display {
        self.constraint(db)
            .display_negated(db, self.typevar(db).display(db))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct RangeConstraint<'db> {
    lower: Type<'db>,
    upper: Type<'db>,
}

impl<'db> RangeConstraint<'db> {
    /// Returns a new range constraint.
    ///
    /// Panics if `lower` and `upper` are not both fully static.
    fn new_node(
        db: &'db dyn Db,
        lower: Type<'db>,
        typevar: BoundTypeVarInstance<'db>,
        upper: Type<'db>,
    ) -> Node<'db> {
        debug_assert_eq!(lower, lower.bottom_materialization(db));
        debug_assert_eq!(upper, upper.top_materialization(db));

        // If `lower ≰ upper`, then the constraint cannot be satisfied, since there is no type that
        // is both greater than `lower`, and less than `upper`.
        if !lower.is_subtype_of(db, upper) {
            return Node::AlwaysFalse;
        }

        // If the requested constraint is `Never ≤ T ≤ object`, then the typevar can be specialized
        // to _any_ type, and the constraint does nothing.
        let constraint = RangeConstraint { lower, upper };
        if constraint.is_always() {
            return Node::AlwaysTrue;
        }

        Node::new_constraint(db, ConstrainedTypeVar::new(db, typevar, constraint))
    }

    fn contains(self, db: &'db dyn Db, other: RangeConstraint<'db>) -> bool {
        self.lower.is_subtype_of(db, other.lower) && other.upper.is_subtype_of(db, self.upper)
    }

    fn is_always(self) -> bool {
        self.lower.is_never() && self.upper.is_object()
    }

    /// Returns the intersection of two range constraints, or `None` if the intersection is empty.
    fn intersect(&self, db: &'db dyn Db, other: RangeConstraint<'db>) -> Option<Self> {
        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α ≤ (t₁ ∩ t₂))
        let lower = UnionType::from_elements(db, [self.lower, other.lower]);
        let upper = IntersectionType::from_elements(db, [self.upper, other.upper]);

        // If `lower ≰ upper`, then the intersection is empty, since there is no type that is both
        // greater than `lower`, and less than `upper`.
        if !lower.is_subtype_of(db, upper) {
            return None;
        }

        Some(Self { lower, upper })
    }

    fn display(self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        self.display_inner(db, typevar, false)
    }

    fn display_negated(self, db: &'db dyn Db, typevar: impl Display) -> impl Display {
        self.display_inner(db, typevar, true)
    }

    fn display_inner(self, db: &'db dyn Db, typevar: impl Display, negated: bool) -> impl Display {
        struct DisplayRangeConstraint<'db, D> {
            constraint: RangeConstraint<'db>,
            typevar: D,
            negated: bool,
            db: &'db dyn Db,
        }

        impl<D: Display> Display for DisplayRangeConstraint<'_, D> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if (self.constraint.lower).is_equivalent_to(self.db, self.constraint.upper) {
                    return write!(
                        f,
                        "({} {} {})",
                        &self.typevar,
                        if self.negated { "≠" } else { "=" },
                        self.constraint.lower.display(self.db)
                    );
                }

                if self.negated {
                    f.write_str("¬")?;
                }
                f.write_str("(")?;
                if !self.constraint.lower.is_never() {
                    write!(f, "{} ≤ ", self.constraint.lower.display(self.db))?;
                }
                self.typevar.fmt(f)?;
                if !self.constraint.upper.is_object() {
                    write!(f, " ≤ {}", self.constraint.upper.display(self.db))?;
                }
                f.write_str(")")
            }
        }

        DisplayRangeConstraint {
            constraint: self,
            typevar,
            negated,
            db,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
enum Node<'db> {
    AlwaysFalse,
    AlwaysTrue,
    Interior(InteriorNode<'db>),
}

impl<'db> Node<'db> {
    fn new(
        db: &'db dyn Db,
        constraint: ConstrainedTypeVar<'db>,
        if_true: Node<'db>,
        if_false: Node<'db>,
    ) -> Self {
        debug_assert!(if_true.atom(db).is_none_or(|atom| atom > constraint));
        debug_assert!(if_false.atom(db).is_none_or(|atom| atom > constraint));
        if if_true == if_false {
            return if_true;
        }
        Self::Interior(InteriorNode::new(db, constraint, if_true, if_false))
    }

    fn new_constraint(db: &'db dyn Db, constraint: ConstrainedTypeVar<'db>) -> Self {
        Self::Interior(InteriorNode::new(
            db,
            constraint,
            Node::AlwaysTrue,
            Node::AlwaysFalse,
        ))
    }

    fn new_satisfied_constraint(db: &'db dyn Db, constraint: SatisfiedConstraint<'db>) -> Self {
        match constraint {
            SatisfiedConstraint::Positive(constraint) => Self::Interior(InteriorNode::new(
                db,
                constraint,
                Node::AlwaysTrue,
                Node::AlwaysFalse,
            )),
            SatisfiedConstraint::Negative(constraint) => Self::Interior(InteriorNode::new(
                db,
                constraint,
                Node::AlwaysFalse,
                Node::AlwaysTrue,
            )),
        }
    }

    fn atom(self, db: &'db dyn Db) -> Option<ConstrainedTypeVar<'db>> {
        match self {
            Node::Interior(interior) => Some(interior.atom(db)),
            _ => None,
        }
    }

    fn is_always_satisfied(self) -> bool {
        matches!(self, Node::AlwaysTrue)
    }

    fn is_never_satisfied(self) -> bool {
        matches!(self, Node::AlwaysFalse)
    }

    fn interior_node_count(self, db: &'db dyn Db) -> usize {
        match self {
            Node::AlwaysTrue | Node::AlwaysFalse => 0,
            Node::Interior(interior) => interior.interior_node_count(db),
        }
    }

    fn negate(self, db: &'db dyn Db) -> Self {
        match self {
            Node::AlwaysTrue => Node::AlwaysFalse,
            Node::AlwaysFalse => Node::AlwaysTrue,
            Node::Interior(interior) => interior.negate(db),
        }
    }

    fn or(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::AlwaysTrue, _) | (_, Node::AlwaysTrue) => Node::AlwaysTrue,
            (Node::AlwaysFalse, other) | (other, Node::AlwaysFalse) => other,
            (Node::Interior(a), Node::Interior(b)) => {
                // OR is commutative, which lets us halve the cache requirements
                let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
                a.or(db, b)
            }
        }
    }

    fn and(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::AlwaysFalse, _) | (_, Node::AlwaysFalse) => Node::AlwaysFalse,
            (Node::AlwaysTrue, other) | (other, Node::AlwaysTrue) => other,
            (Node::Interior(a), Node::Interior(b)) => {
                // AND is commutative, which lets us halve the cache requirements
                let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
                a.and(db, b)
            }
        }
    }

    fn iff(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::AlwaysFalse, Node::AlwaysFalse) | (Node::AlwaysTrue, Node::AlwaysTrue) => {
                Node::AlwaysTrue
            }
            (Node::AlwaysTrue, Node::AlwaysFalse) | (Node::AlwaysFalse, Node::AlwaysTrue) => {
                Node::AlwaysFalse
            }
            (Node::AlwaysTrue | Node::AlwaysFalse, Node::Interior(interior)) => Node::new(
                db,
                interior.atom(db),
                self.iff(db, interior.if_true(db)),
                self.iff(db, interior.if_false(db)),
            ),
            (Node::Interior(interior), Node::AlwaysTrue | Node::AlwaysFalse) => Node::new(
                db,
                interior.atom(db),
                interior.if_true(db).iff(db, other),
                interior.if_false(db).iff(db, other),
            ),
            (Node::Interior(a), Node::Interior(b)) => {
                // IFF is commutative, which lets us halve the cache requirements
                let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
                a.iff(db, b)
            }
        }
    }

    fn ite(self, db: &'db dyn Db, then_node: Self, else_node: Self) -> Self {
        self.and(db, then_node)
            .or(db, self.negate(db).and(db, else_node))
    }

    fn restrict(
        self,
        db: &'db dyn Db,
        assignment: impl IntoIterator<Item = SatisfiedConstraint<'db>>,
    ) -> (Node<'db>, bool) {
        //eprintln!("==> restrict {}", self.display(db));
        assignment
            .into_iter()
            .fold((self, true), |(restricted, found), assignment| {
                /*
                eprintln!(" -> restricted {}", restricted.display(db));
                eprintln!(
                    " -> {} == {}",
                    assignment.constraint().display(db),
                    match assignment {
                        SatisfiedConstraint::Positive(_) => "1",
                        SatisfiedConstraint::Negative(_) => "0",
                    },
                );
                */
                let (restricted, found_this) = restricted.restrict_one(db, assignment);
                (restricted, found && found_this)
            })
    }

    fn restrict_one(
        self,
        db: &'db dyn Db,
        assignment: SatisfiedConstraint<'db>,
    ) -> (Node<'db>, bool) {
        match self {
            Node::AlwaysTrue => (Node::AlwaysTrue, false),
            Node::AlwaysFalse => (Node::AlwaysFalse, false),
            Node::Interior(interior) => interior.restrict_one(db, assignment),
        }
    }

    fn substitute_intersection(
        self,
        db: &'db dyn Db,
        left: SatisfiedConstraint<'db>,
        right: SatisfiedConstraint<'db>,
        replacement_node: Node<'db>,
    ) -> Self {
        /*
        eprintln!(
            "==> substitute {} for ({}) ∧ ({})",
            replacement_node.display(db),
            left.display(db),
            right.display(db)
        );
        eprintln!(" -> in {}", self.display(db));
        */
        let (when_not_left, _) = self.restrict(db, [left.flipped()]);
        //eprintln!(" -> (x ∧ y)[x=0] = {}", when_not_left.display(db));
        let (when_left_but_not_right, _) = self.restrict(db, [left, right.flipped()]);
        /*
        eprintln!(
            " -> (x ∧ y)[x=1,y=0] = {}",
            when_left_but_not_right.display(db)
        );
        */
        let (when_left_and_right, both_found) = self.restrict(db, [left, right]);
        if !both_found {
            return self;
        }
        //eprintln!(" -> (x ∧ y)[x=1,y=1] = {}", when_left_and_right.display(db));
        let left_node = Node::new_satisfied_constraint(db, left);
        let right_node = Node::new_satisfied_constraint(db, right);

        let right_result = right_node.ite(db, Node::AlwaysFalse, when_left_but_not_right);
        //eprintln!(" -> right_result = {}", right_result.display(db));
        let left_result = left_node.ite(db, right_result, when_not_left);
        //eprintln!(" -> left_result  = {}", left_result.display(db));
        let result = replacement_node.ite(db, when_left_and_right, left_result);
        //eprintln!(" -> result       = {}", result.display(db));

        let validity = replacement_node.iff(db, left_node.and(db, right_node));
        //eprintln!(" -> validity     = {}", validity.display(db));
        let constrained_original = self.and(db, validity);
        //eprintln!(" -> **original   = {}", constrained_original.display(db));
        let constrained_replacement = result.and(db, validity);
        //eprintln!(" -> **result     = {}", constrained_replacement.display(db));
        if constrained_original == constrained_replacement {
            //eprintln!(" -> using replacement");
            result
        } else {
            //eprintln!(" -> using original");
            self
        }
    }

    fn substitute_union(
        self,
        db: &'db dyn Db,
        left: SatisfiedConstraint<'db>,
        right: SatisfiedConstraint<'db>,
        replacement_node: Node<'db>,
    ) -> Self {
        /*
        eprintln!(
            "==> substitute {} for ({}) ∨ ({})",
            replacement_node.display(db),
            left.display(db),
            right.display(db)
        );
        eprintln!(" -> in {}", self.display(db));
        */
        let (when_l0_r0, _) = self.restrict(db, [left.flipped(), right.flipped()]);
        //eprintln!(" -> (x ∧ y)[x=0,y=0] = {}", when_l0_r0.display(db));
        let (when_l1_r0, _) = self.restrict(db, [left, right.flipped()]);
        //eprintln!(" -> (x ∧ y)[x=1,y=0] = {}", when_l1_r0.display(db));
        let (when_l0_r1, _) = self.restrict(db, [left.flipped(), right]);
        //eprintln!(" -> (x ∧ y)[x=0,y=1] = {}", when_l0_r1.display(db));
        let (when_l1_r1, both_found) = self.restrict(db, [left, right]);
        if !both_found {
            return self;
        }
        //eprintln!(" -> (x ∧ y)[x=1,y=1] = {}", when_l1_r1.display(db));
        let left_node = Node::new_satisfied_constraint(db, left);
        let right_node = Node::new_satisfied_constraint(db, right);

        let result = replacement_node.ite(
            db,
            when_l1_r0.or(db, when_l0_r1.or(db, when_l1_r1)),
            when_l0_r0,
        );
        //eprintln!(" -> result       = {}", result.display(db));

        let validity = replacement_node.iff(db, left_node.or(db, right_node));
        //eprintln!(" -> validity     = {}", validity.display(db));
        let constrained_original = self.and(db, validity);
        //eprintln!(" -> **original   = {}", constrained_original.display(db));
        let constrained_replacement = result.and(db, validity);
        //eprintln!(" -> **result     = {}", constrained_replacement.display(db));
        if constrained_original == constrained_replacement {
            //eprintln!(" -> using replacement");
            result
        } else {
            //eprintln!(" -> using original");
            self
        }
    }

    fn for_each_constraint(self, db: &'db dyn Db, f: &mut dyn FnMut(ConstrainedTypeVar<'db>)) {
        let Node::Interior(interior) = self else {
            return;
        };
        f(interior.atom(db));
        interior.if_true(db).for_each_constraint(db, f);
        interior.if_false(db).for_each_constraint(db, f);
    }

    fn simplify(self, db: &'db dyn Db) -> Self {
        match self {
            Node::AlwaysTrue | Node::AlwaysFalse => self,
            Node::Interior(interior) => interior.simplify(db),
        }
    }

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
                        let interior_atom = interior.atom(db);
                        self.current_clause
                            .push(SatisfiedConstraint::Positive(interior_atom));
                        self.visit_node(db, interior.if_true(db));
                        self.current_clause.pop();
                        self.current_clause
                            .push(SatisfiedConstraint::Negative(interior_atom));
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
                        clauses.simplify();
                        clauses.display(self.db).fmt(f)
                    }
                }
            }
        }

        DisplayNode { node: self, db }
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct InteriorNode<'db> {
    atom: ConstrainedTypeVar<'db>,
    if_true: Node<'db>,
    if_false: Node<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for InteriorNode<'_> {}

#[salsa::tracked]
impl<'db> InteriorNode<'db> {
    #[salsa::tracked]
    fn interior_node_count(self, db: &'db dyn Db) -> usize {
        self.if_true(db).interior_node_count(db) + self.if_false(db).interior_node_count(db) + 1
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn negate(self, db: &'db dyn Db) -> Node<'db> {
        Node::new(
            db,
            self.atom(db),
            self.if_true(db).negate(db),
            self.if_false(db).negate(db),
        )
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn or(self, db: &'db dyn Db, other: Self) -> Node<'db> {
        let self_atom = self.atom(db);
        let other_atom = other.atom(db);
        match self_atom.cmp(&other_atom) {
            Ordering::Equal => Node::new(
                db,
                self_atom,
                self.if_true(db).or(db, other.if_true(db)),
                self.if_false(db).or(db, other.if_false(db)),
            ),
            Ordering::Less => Node::new(
                db,
                self_atom,
                self.if_true(db).or(db, Node::Interior(other)),
                self.if_false(db).or(db, Node::Interior(other)),
            ),
            Ordering::Greater => Node::new(
                db,
                other_atom,
                Node::Interior(self).or(db, other.if_true(db)),
                Node::Interior(self).or(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn and(self, db: &'db dyn Db, other: Self) -> Node<'db> {
        let self_atom = self.atom(db);
        let other_atom = other.atom(db);
        match self_atom.cmp(&other_atom) {
            Ordering::Equal => Node::new(
                db,
                self_atom,
                self.if_true(db).and(db, other.if_true(db)),
                self.if_false(db).and(db, other.if_false(db)),
            ),
            Ordering::Less => Node::new(
                db,
                self_atom,
                self.if_true(db).and(db, Node::Interior(other)),
                self.if_false(db).and(db, Node::Interior(other)),
            ),
            Ordering::Greater => Node::new(
                db,
                other_atom,
                Node::Interior(self).and(db, other.if_true(db)),
                Node::Interior(self).and(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn iff(self, db: &'db dyn Db, other: Self) -> Node<'db> {
        let self_atom = self.atom(db);
        let other_atom = other.atom(db);
        match self_atom.cmp(&other_atom) {
            Ordering::Equal => Node::new(
                db,
                self_atom,
                self.if_true(db).iff(db, other.if_true(db)),
                self.if_false(db).iff(db, other.if_false(db)),
            ),
            Ordering::Less => Node::new(
                db,
                self_atom,
                self.if_true(db).iff(db, Node::Interior(other)),
                self.if_false(db).iff(db, Node::Interior(other)),
            ),
            Ordering::Greater => Node::new(
                db,
                other_atom,
                Node::Interior(self).iff(db, other.if_true(db)),
                Node::Interior(self).iff(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn restrict_one(
        self,
        db: &'db dyn Db,
        assignment: SatisfiedConstraint<'db>,
    ) -> (Node<'db>, bool) {
        // If this node's variable is larger than the assignment's variable, then we have reached a
        // point in the BDD where the assignment can no longer affect the result,
        // and we can return early.
        let self_atom = self.atom(db);
        if assignment.constraint() < self_atom {
            return (Node::Interior(self), false);
        }

        // Otherwise, check if this node's variable is in the assignment. If so, substitute the
        // variable by replacing this node with its if_false/if_true edge, accordingly.
        if assignment == SatisfiedConstraint::Positive(self_atom) {
            (self.if_true(db), true)
        } else if assignment == SatisfiedConstraint::Negative(self_atom) {
            (self.if_false(db), true)
        } else {
            let (if_true, found_in_true) = self.if_true(db).restrict_one(db, assignment);
            let (if_false, found_in_false) = self.if_false(db).restrict_one(db, assignment);
            (
                Node::new(db, self_atom, if_true, if_false),
                found_in_true || found_in_false,
            )
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn simplify(self, db: &'db dyn Db) -> Node<'db> {
        let mut visited_atoms = FxHashSet::default();
        let mut new_atoms = vec![self.atom(db)];
        let mut simplified = Node::Interior(self);
        while let Some(new_atom) = new_atoms.pop() {
            visited_atoms.insert(new_atom);
            Node::Interior(self).for_each_constraint(db, &mut |nested_atom| {
                if new_atom == nested_atom {
                    return;
                }

                let typevar = new_atom.typevar(db);
                if typevar != nested_atom.typevar(db) {
                    return;
                }

                let larger_smaller = if new_atom.contains(db, nested_atom) {
                    Some((new_atom, nested_atom))
                } else if nested_atom.contains(db, new_atom) {
                    Some((nested_atom, new_atom))
                } else {
                    None
                };
                if let Some((larger_atom, smaller_atom)) = larger_smaller {
                    /*
                    eprintln!(
                        "==> {} contains {}",
                        new_atom.display(db),
                        nested_atom.display(db),
                    );
                    */
                    simplified = simplified.substitute_union(
                        db,
                        SatisfiedConstraint::Positive(larger_atom),
                        SatisfiedConstraint::Positive(smaller_atom),
                        Node::new_satisfied_constraint(
                            db,
                            SatisfiedConstraint::Positive(larger_atom),
                        ),
                    );
                    simplified = simplified.substitute_intersection(
                        db,
                        SatisfiedConstraint::Negative(larger_atom),
                        SatisfiedConstraint::Negative(smaller_atom),
                        Node::new_satisfied_constraint(
                            db,
                            SatisfiedConstraint::Negative(larger_atom),
                        ),
                    );

                    simplified = simplified.substitute_intersection(
                        db,
                        SatisfiedConstraint::Negative(larger_atom),
                        SatisfiedConstraint::Positive(smaller_atom),
                        Node::AlwaysFalse,
                    );
                }

                let new_constraint = new_atom.constraint(db);
                let nested_constraint = nested_atom.constraint(db);
                match new_constraint.intersect(db, nested_constraint) {
                    Some(intersection) => {
                        let intersection_constraint =
                            ConstrainedTypeVar::new(db, typevar, intersection);
                        if !visited_atoms.contains(&intersection_constraint) {
                            new_atoms.push(intersection_constraint);
                        }
                        let positive_intersection_node = Node::new_satisfied_constraint(
                            db,
                            SatisfiedConstraint::Positive(intersection_constraint),
                        );
                        let negative_intersection_node = Node::new_satisfied_constraint(
                            db,
                            SatisfiedConstraint::Negative(intersection_constraint),
                        );

                        simplified = simplified.substitute_intersection(
                            db,
                            SatisfiedConstraint::Positive(new_atom),
                            SatisfiedConstraint::Positive(nested_atom),
                            positive_intersection_node,
                        );
                        simplified = simplified.substitute_union(
                            db,
                            SatisfiedConstraint::Negative(new_atom),
                            SatisfiedConstraint::Negative(nested_atom),
                            negative_intersection_node,
                        );

                        simplified = simplified.substitute_intersection(
                            db,
                            SatisfiedConstraint::Positive(new_atom),
                            SatisfiedConstraint::Negative(nested_atom),
                            Node::new_satisfied_constraint(
                                db,
                                SatisfiedConstraint::Positive(new_atom),
                            )
                            .and(db, negative_intersection_node),
                        );
                        simplified = simplified.substitute_intersection(
                            db,
                            SatisfiedConstraint::Negative(new_atom),
                            SatisfiedConstraint::Positive(nested_atom),
                            Node::new_satisfied_constraint(
                                db,
                                SatisfiedConstraint::Positive(nested_atom),
                            )
                            .and(db, negative_intersection_node),
                        );
                    }
                    None => {
                        /*
                        eprintln!(
                            "==> {} ∧ {} is empty",
                            new_atom.display(db),
                            nested_atom.display(db)
                        );
                        */
                        simplified = simplified.substitute_intersection(
                            db,
                            SatisfiedConstraint::Positive(new_atom),
                            SatisfiedConstraint::Positive(nested_atom),
                            Node::AlwaysFalse,
                        );
                        simplified = simplified.substitute_union(
                            db,
                            SatisfiedConstraint::Negative(new_atom),
                            SatisfiedConstraint::Negative(nested_atom),
                            Node::AlwaysTrue,
                        );
                        simplified = simplified.substitute_intersection(
                            db,
                            SatisfiedConstraint::Positive(new_atom),
                            SatisfiedConstraint::Negative(nested_atom),
                            Node::new_constraint(db, new_atom),
                        );
                        simplified = simplified.substitute_intersection(
                            db,
                            SatisfiedConstraint::Negative(new_atom),
                            SatisfiedConstraint::Positive(nested_atom),
                            Node::new_constraint(db, nested_atom),
                        );
                    }
                }
            });
        }
        simplified
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum SatisfiedConstraint<'db> {
    Positive(ConstrainedTypeVar<'db>),
    Negative(ConstrainedTypeVar<'db>),
}

impl<'db> SatisfiedConstraint<'db> {
    fn constraint(self) -> ConstrainedTypeVar<'db> {
        match self {
            SatisfiedConstraint::Positive(constraint) => constraint,
            SatisfiedConstraint::Negative(constraint) => constraint,
        }
    }

    fn flipped(self) -> Self {
        match self {
            SatisfiedConstraint::Positive(constraint) => SatisfiedConstraint::Negative(constraint),
            SatisfiedConstraint::Negative(constraint) => SatisfiedConstraint::Positive(constraint),
        }
    }

    fn flip(&mut self) {
        *self = self.flipped();
    }

    // Keep this for future debugging needs, even though it's not used when rendering constraint
    // sets.
    #[expect(dead_code)]
    fn display(self, db: &'db dyn Db) -> impl Display {
        struct DisplaySatisfiedConstraint<'db> {
            constraint: SatisfiedConstraint<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplaySatisfiedConstraint<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.constraint {
                    SatisfiedConstraint::Positive(constraint) => constraint.display(self.db).fmt(f),
                    SatisfiedConstraint::Negative(constraint) => {
                        constraint.display_negated(self.db).fmt(f)
                    }
                }
            }
        }

        DisplaySatisfiedConstraint {
            constraint: self,
            db,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SatisfiedClause<'db> {
    constraints: Vec<SatisfiedConstraint<'db>>,
}

impl<'db> SatisfiedClause<'db> {
    fn push(&mut self, constraint: SatisfiedConstraint<'db>) {
        self.constraints.push(constraint);
    }

    fn pop(&mut self) {
        self.constraints
            .pop()
            .expect("clause vector should not be empty");
    }

    fn with_flipped_last_constraint(&mut self, f: impl for<'a> FnOnce(&'a Self)) {
        if self.constraints.is_empty() {
            return;
        }
        let last_index = self.constraints.len() - 1;
        self.constraints[last_index].flip();
        f(self);
        self.constraints[last_index].flip();
    }

    fn remove_prefix(&mut self, prefix: &SatisfiedClause<'db>) -> bool {
        if self.constraints.starts_with(&prefix.constraints) {
            self.constraints.drain(0..prefix.constraints.len());
            return true;
        }
        false
    }

    fn display(&self, db: &'db dyn Db) -> impl Display {
        struct DisplaySatisfiedClause<'a, 'db> {
            clause: &'a SatisfiedClause<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplaySatisfiedClause<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if self.clause.constraints.len() > 1 {
                    f.write_str("(")?;
                }
                for (i, constraint) in self.clause.constraints.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ∧ ")?;
                    }
                    match constraint {
                        SatisfiedConstraint::Positive(constraint) => {
                            write!(f, "{}", constraint.display(self.db))?;
                        }
                        SatisfiedConstraint::Negative(constraint) => {
                            write!(f, "{}", constraint.display_negated(self.db))?;
                        }
                    }
                }
                if self.clause.constraints.len() > 1 {
                    f.write_str(")")?;
                }
                Ok(())
            }
        }

        DisplaySatisfiedClause { clause: self, db }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SatisfiedClauses<'db> {
    clauses: Vec<SatisfiedClause<'db>>,
}

impl<'db> SatisfiedClauses<'db> {
    fn push(&mut self, clause: SatisfiedClause<'db>) {
        self.clauses.push(clause);
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

        for i in 0..self.clauses.len() {
            let (clause, rest) = self.clauses[..=i]
                .split_last_mut()
                .expect("index should be in range");
            clause.with_flipped_last_constraint(|clause| {
                for existing in rest {
                    changes_made |= existing.remove_prefix(clause);
                }
            });

            let (clause, rest) = self.clauses[i..]
                .split_first_mut()
                .expect("index should be in range");
            clause.with_flipped_last_constraint(|clause| {
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

    fn simplify(&mut self) {
        while self.simplify_one_round() {
            // Keep going
        }
    }

    fn display(&self, db: &'db dyn Db) -> impl Display {
        struct DisplaySatisfiedClauses<'a, 'db> {
            clauses: &'a SatisfiedClauses<'db>,
            db: &'db dyn Db,
        }

        impl Display for DisplaySatisfiedClauses<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if self.clauses.clauses.is_empty() {
                    return f.write_str("always");
                }
                for (i, clause) in self.clauses.clauses.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ∨ ")?;
                    }
                    clause.display(self.db).fmt(f)?;
                }
                Ok(())
            }
        }

        DisplaySatisfiedClauses { clauses: self, db }
    }
}
