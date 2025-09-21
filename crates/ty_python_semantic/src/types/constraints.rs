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
use std::fmt::{Display, Write};

use itertools::{EitherOrBoth, Itertools};
use smallvec::{SmallVec, smallvec};

use crate::Db;
use crate::types::{BoundTypeVarInstance, IntersectionType, Type, UnionType};

fn comparable<'db>(db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> bool {
    left.is_subtype_of(db, right) || right.is_subtype_of(db, left)
}

fn incomparable<'db>(db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> bool {
    !comparable(db, left, right)
}

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
            if result.union(db, f(child)).is_always_satisfied() {
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
    pub(crate) fn union(&mut self, db: &'db dyn Db, other: Self) -> &Self {
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
            self.union(db, other());
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

    fn always() -> Self {
        Self {
            lower: Type::Never,
            upper: Type::object(),
        }
    }

    fn contains(self, db: &'db dyn Db, other: RangeConstraint<'db>) -> bool {
        self.lower.is_subtype_of(db, other.lower) && other.upper.is_subtype_of(db, self.upper)
    }

    fn is_always(self) -> bool {
        self.lower.is_never() && self.upper.is_object()
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

    fn atom(self, db: &'db dyn Db) -> Option<ConstrainedTypeVar<'db>> {
        match self {
            Node::Interior(interior) => Some(interior.atom(db)),
            _ => None,
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Node::AlwaysFalse | Node::AlwaysTrue)
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

    fn implies(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::AlwaysFalse, _) | (_, Node::AlwaysTrue) => Node::AlwaysTrue,
            (Node::AlwaysTrue, other) | (other, Node::AlwaysFalse) => other,
            (Node::Interior(a), Node::Interior(b)) => {
                // Implies is _not_ commutative, so we can't use the same trick as above.
                a.implies(db, b)
            }
        }
    }

    fn simplify_relative_to(self, db: &'db dyn Db, relative_to: Self) -> Self {
        // relative_to should describe variable combinations that are impossible (for instance
        // `x ∧ ¬y` if `x → y`). For those variable assignments, we don't care what value the BDD
        // resolves to. We can try to simplify the BDD by seeing if mapping those variables to 0 or
        // to 1 give a smaller BDD. Given `x → y`, that would simplify `x ∧ y` to `x`. (That might
        // require mapping `x ∧ ¬y` to 0 or to 1, depending on how `x` and `y` relate to each other
        // in the variable ordering.)

        // First try forcing the don't-care assignments to 0. `relative_to` already encodes this,
        // since it maps everything we care about to 1, and everything we don't care about to 0. So
        // we can just AND the two BDDs together.
        let mapped_to_zero = self.and(db, relative_to);

        // And then we try forcing them to 1. To that, we negate `relative_to`, so that things we
        // care about map to 0, and things we don't care about map to 1. And then we OR that with
        // the original BDD.
        let mapped_to_one = self.or(db, relative_to.negate(db));

        // Then we keep the result that has the fewest interior nodes, using that as a proxy for
        // the complexity of the underlying function.
        let mapped_to_zero_size = mapped_to_zero.interior_node_count(db);
        let mapped_to_one_size = mapped_to_one.interior_node_count(db);
        let (smaller_size, smaller_updated) = if mapped_to_zero_size < mapped_to_one_size {
            (mapped_to_zero_size, mapped_to_zero)
        } else {
            (mapped_to_one_size, mapped_to_one)
        };

        let original_size = self.interior_node_count(db);
        if smaller_size < original_size {
            smaller_updated
        } else {
            self
        }
    }

    fn for_each_constraint(
        self,
        db: &'db dyn Db,
        f: &mut dyn FnMut(ConstrainedTypeVar<'db>) -> (),
    ) {
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

    fn display(self, db: &'db dyn Db) -> impl Display {
        // To render a BDD in DNF form, you perform a depth-first search of the BDD tree, looking
        // for any path that leads to the AlwaysTrue terminal. Each such path represents one of the
        // intersection clauses in the DNF Form. The path traverses zero or more interior nodes,
        // and takes either the true or false edge from each one. That gives you the positive or
        // negative individual constraints in the path's clause.
        //
        // To make things simpler, we're going to perform that search eagerly, rendering the result
        // into a `String`, and just return that `String` as our `Display` impl.

        struct DisplayNode {
            result: String,
            current_clause: String,
            first_clause: bool,
        }

        impl DisplayNode {
            fn display_node<'db>(&mut self, db: &'db dyn Db, node: Node<'db>) {
                match node {
                    Node::AlwaysFalse => return,

                    Node::AlwaysTrue => {
                        if self.first_clause {
                            self.first_clause = false;
                        } else {
                            self.result.push_str(" ∨ ");
                        }
                        if self.current_clause.contains("∧") {
                            self.result.push_str("(");
                        }
                        self.result.push_str(&self.current_clause);
                        if self.current_clause.contains("∧") {
                            self.result.push_str(")");
                        }
                    }

                    Node::Interior(interior) => {
                        let interior_atom = interior.atom(db);
                        let current_length = self.current_clause.len();

                        if current_length > 1 {
                            self.current_clause.push_str(" ∧ ");
                        }
                        let _ = write!(&mut self.current_clause, "{}", interior_atom.display(db));
                        self.display_node(db, interior.if_true(db));
                        self.current_clause.truncate(current_length);

                        if current_length > 1 {
                            self.current_clause.push_str(" ∧ ");
                        }
                        let _ = write!(
                            &mut self.current_clause,
                            "{}",
                            interior.atom(db).display_negated(db)
                        );
                        self.display_node(db, interior.if_false(db));
                        self.current_clause.truncate(current_length);
                    }
                }
            }
        }

        match self {
            Node::AlwaysTrue => String::from("always"),
            Node::AlwaysFalse => String::from("never"),
            Node::Interior(_) => {
                let mut display = DisplayNode {
                    result: String::new(),
                    current_clause: String::new(),
                    first_clause: true,
                };
                display.display_node(db, self);
                display.result
            }
        }
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
    fn implies(self, db: &'db dyn Db, other: Self) -> Node<'db> {
        let self_atom = self.atom(db);
        let other_atom = other.atom(db);
        match self_atom.cmp(&other_atom) {
            Ordering::Equal => Node::new(
                db,
                self_atom,
                self.if_true(db).implies(db, other.if_true(db)),
                self.if_false(db).implies(db, other.if_false(db)),
            ),
            Ordering::Less => Node::new(
                db,
                self_atom,
                self.if_true(db).implies(db, Node::Interior(other)),
                self.if_false(db).implies(db, Node::Interior(other)),
            ),
            Ordering::Greater => Node::new(
                db,
                other_atom,
                Node::Interior(self).implies(db, other.if_true(db)),
                Node::Interior(self).implies(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn simplify(self, db: &'db dyn Db) -> Node<'db> {
        let self_atom = self.atom(db);
        let mut simplified = Node::Interior(self);
        Node::Interior(self).for_each_constraint(db, &mut |nested_constraint| {
            let given = if self_atom == nested_constraint {
                return;
            } else if self_atom.contains(db, nested_constraint) {
                Node::new_constraint(db, nested_constraint)
                    .implies(db, Node::new_constraint(db, self_atom))
            } else if nested_constraint.contains(db, self_atom) {
                Node::new_constraint(db, self_atom)
                    .implies(db, Node::new_constraint(db, nested_constraint))
            } else {
                return;
            };
            simplified = simplified.simplify_relative_to(db, given);
        });
        simplified
    }
}
