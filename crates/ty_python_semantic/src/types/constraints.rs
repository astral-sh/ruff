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
//! [bdd]: https://en.wikipedia.org/wiki/Binary_decision_diagram

use std::cmp::Ordering;
use std::fmt::Display;

use itertools::Itertools;
use rustc_hash::FxHashSet;

use crate::Db;
use crate::types::{BoundTypeVarIdentity, IntersectionType, Type, UnionType};

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
            if result.intersect(db, f(child)).is_never_satisfied() {
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
/// [POPL2015]: https://doi.org/10.1145/2676726.2676991
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct ConstraintSet<'db> {
    /// The BDD representing this constraint set
    node: UnderspecifiedNode<'db>,
}

impl<'db> ConstraintSet<'db> {
    fn never() -> Self {
        Self {
            node: UnderspecifiedNode::AlwaysFalse,
        }
    }

    fn always() -> Self {
        Self {
            node: UnderspecifiedNode::AlwaysTrue,
        }
    }

    /// Returns whether this constraint set never holds
    pub(crate) fn is_never_satisfied(self) -> bool {
        self.node.is_never_satisfied()
    }

    /// Returns whether this constraint set always holds
    pub(crate) fn is_always_satisfied(self) -> bool {
        self.node.is_always_satisfied()
    }

    /// Updates this constraint set to hold the union of itself and another constraint set.
    pub(crate) fn union(&mut self, db: &'db dyn Db, other: Self) -> Self {
        self.node = self.node.or(db, other.node).simplify(db);
        *self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    pub(crate) fn intersect(&mut self, db: &'db dyn Db, other: Self) -> Self {
        self.node = self.node.and(db, other.node).simplify(db);
        *self
    }

    /// Returns the negation of this constraint set.
    pub(crate) fn negate(self, db: &'db dyn Db) -> Self {
        Self {
            node: self.node.negate(db).simplify(db),
        }
    }

    /// Returns the intersection of this constraint set and another. The other constraint set is
    /// provided as a thunk, to implement short-circuiting: the thunk is not forced if the
    /// constraint set is already saturated.
    pub(crate) fn and(mut self, db: &'db dyn Db, other: impl FnOnce() -> Self) -> Self {
        if !self.is_never_satisfied() {
            self.intersect(db, other());
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
        typevar: BoundTypeVarIdentity<'db>,
        upper: Type<'db>,
    ) -> Self {
        let lower = lower.bottom_materialization(db);
        let upper = upper.top_materialization(db);
        Self {
            node: ConstrainedTypeVar::new_node(db, lower, typevar, upper).into(),
        }
    }

    pub(crate) fn negated_range(
        db: &'db dyn Db,
        lower: Type<'db>,
        typevar: BoundTypeVarIdentity<'db>,
        upper: Type<'db>,
    ) -> Self {
        Self::range(db, lower, typevar, upper).negate(db)
    }

    pub(crate) fn display(self, db: &'db dyn Db) -> impl Display {
        self.node.display(db)
    }
}

impl From<bool> for ConstraintSet<'_> {
    fn from(b: bool) -> Self {
        if b { Self::always() } else { Self::never() }
    }
}

/// An individual constraint in a constraint set. This restricts a single typevar to be within a
/// lower and upper bound.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub(crate) struct ConstrainedTypeVar<'db> {
    typevar: BoundTypeVarIdentity<'db>,
    lower: Type<'db>,
    upper: Type<'db>,
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
        lower: Type<'db>,
        typevar: BoundTypeVarIdentity<'db>,
        upper: Type<'db>,
    ) -> UnderspecifiedNode<'db> {
        debug_assert_eq!(lower, lower.bottom_materialization(db));
        debug_assert_eq!(upper, upper.top_materialization(db));

        // If `lower ≰ upper`, then the constraint cannot be satisfied, since there is no type that
        // is both greater than `lower`, and less than `upper`.
        if !lower.is_subtype_of(db, upper) {
            return UnderspecifiedNode::AlwaysFalse;
        }

        // If the requested constraint is `Never ≤ T ≤ object`, then the typevar can be specialized
        // to _any_ type, and the constraint does nothing.
        if lower.is_never() && upper.is_object() {
            return UnderspecifiedNode::AlwaysTrue;
        }

        UnderspecifiedNode::new_constraint(db, ConstrainedTypeVar::new(db, typevar, lower, upper))
    }

    fn when_true(self) -> ConstraintAssignment<'db> {
        ConstraintAssignment::Positive(self)
    }

    fn when_false(self) -> ConstraintAssignment<'db> {
        ConstraintAssignment::Negative(self)
    }

    fn contains(self, db: &'db dyn Db, other: Self) -> bool {
        if self.typevar(db) != other.typevar(db) {
            return false;
        }
        self.lower(db).is_subtype_of(db, other.lower(db))
            && other.upper(db).is_subtype_of(db, self.upper(db))
    }

    /// Returns the intersection of two range constraints, or `None` if the intersection is empty.
    fn intersect(self, db: &'db dyn Db, other: Self) -> Option<Self> {
        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α ≤ (t₁ ∩ t₂))
        let lower = UnionType::from_elements(db, [self.lower(db), other.lower(db)]);
        let upper = IntersectionType::from_elements(db, [self.upper(db), other.upper(db)]);

        // If `lower ≰ upper`, then the intersection is empty, since there is no type that is both
        // greater than `lower`, and less than `upper`.
        if !lower.is_subtype_of(db, upper) {
            return None;
        }

        Some(Self::new(db, self.typevar(db), lower, upper))
    }

    fn display(self, db: &'db dyn Db) -> impl Display {
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
                if lower.is_equivalent_to(self.db, upper) {
                    return write!(
                        f,
                        "({} {} {})",
                        self.constraint.typevar(self.db).display(self.db),
                        if self.negated { "≠" } else { "=" },
                        lower.display(self.db)
                    );
                }

                if self.negated {
                    f.write_str("¬")?;
                }
                f.write_str("(")?;
                if !lower.is_never() {
                    write!(f, "{} ≤ ", lower.display(self.db))?;
                }
                self.constraint.typevar(self.db).display(self.db).fmt(f)?;
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
/// BDD nodes are _reduced_, which means that there are no duplicate nodes (which we handle via
/// Salsa interning), and that there are no redundant nodes, with `if_true` and `if_false` edges
/// that point at the same node.
///
/// BDD nodes are also _ordered_, meaning that every path from the root of a BDD to a terminal node
/// visits variables in the same order. [`ConstrainedTypeVar`]s are interned, so we can use the IDs
/// that salsa assigns to define this order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
enum Node<'db> {
    AlwaysFalse,
    AlwaysTrue,
    Interior(InteriorNode<'db>),
}

impl<'db> Node<'db> {
    /// Creates a new underspecified BDD node, ensuring that it is fully reduced.
    fn new(
        db: &'db dyn Db,
        constraint: ConstrainedTypeVar<'db>,
        if_true: Node<'db>,
        if_false: Node<'db>,
    ) -> Self {
        debug_assert!(
            (if_true.root_constraint(db))
                .is_none_or(|root_constraint| root_constraint > constraint)
        );
        debug_assert!(
            (if_false.root_constraint(db))
                .is_none_or(|root_constraint| root_constraint > constraint)
        );
        if if_true == if_false {
            return if_true;
        }
        Self::Interior(InteriorNode::new(db, constraint, if_true, if_false))
    }

    /// Returns the BDD variable of the root node of this BDD, or `None` if this BDD is a terminal
    /// node.
    fn root_constraint(self, db: &'db dyn Db) -> Option<ConstrainedTypeVar<'db>> {
        match self {
            Node::Interior(interior) => Some(interior.constraint(db)),
            _ => None,
        }
    }

    /// Returns the number of internal nodes in this BDD. This is a decent proxy for the complexity
    /// of the function that the BDD represents.
    fn interior_node_count(self, db: &'db dyn Db) -> usize {
        match self {
            Node::AlwaysTrue | Node::AlwaysFalse => 0,
            Node::Interior(interior) => interior.interior_node_count(db),
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
                        let interior_constraint = interior.constraint(db);
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
                        clauses.simplify();
                        clauses.display(self.db).fmt(f)
                    }
                }
            }
        }

        DisplayNode { node: self, db }
    }
}

/// An interior node of a BDD
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(Ord, PartialOrd)]
struct InteriorNode<'db> {
    constraint: ConstrainedTypeVar<'db>,
    if_true: Node<'db>,
    if_false: Node<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for InteriorNode<'_> {}

#[salsa::tracked]
impl<'db> InteriorNode<'db> {
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn interior_node_count(self, db: &'db dyn Db) -> usize {
        1 + self.if_true(db).interior_node_count(db) + self.if_false(db).interior_node_count(db)
    }
}

/// An "underspecified" BDD node.
///
/// This is just like a BDD [`Node`], but with an additional
/// [`Impossible`][UnderspecifiedNode::Impossible] terminal, which is used to represent situations
/// that are impossible. For instance, two constraints might be mutually incompatible, because they
/// constrain the same typevar and their bounds are disjoint. (That is, there is no type that
/// satisfies both constraints simultaneously.) Since that situation is not possible, it does not
/// matter what value the BDD evaluates to for any input with both of the corresponding variables
/// set to `true`. This is called _underspecified_ in the BDD literature, since we do not have a
/// single concrete desired output for every possible input. (The opposite is a _fully specified_
/// BDD, which we represent with a [`Node`].) The set of inputs where we don't care what output is
/// produced is called the _don't care set_ or _impossible set_; the inputs where we do care are
/// (unsurprisingly) called the _care set_ or _possible set_.
///
/// For any underspecified BDD, there are multiple fully specified BDDs that agree with it on all
/// inputs in the care set. At some point, we have to convert an underspecified BDD into a fully
/// specified one, but (a) we want to do that as late as possible, and (b) when we do, we want to
/// choose the simplest fully specified BDD that is still consistent wrt the care set.
///
/// We use a separate type for underspecified BDDs so that we can always tell in the code when we
/// are operating on a BDD that might be underspecified.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
enum UnderspecifiedNode<'db> {
    AlwaysFalse,
    AlwaysTrue,
    Impossible,
    /// An interior node that has at least one path to the [`Impossible`][Self::Impossible]
    /// terminal.
    Interior(UnderspecifiedInteriorNode<'db>),
    /// An interior node that does not have any path to the [`Impossible`][Self::Impossible]
    /// terminal. (This is an optimization that lets us short-circuit the conversion from
    /// underspecified to fully specified in many cases.)
    FullySpecified(InteriorNode<'db>),
}

impl<'db> UnderspecifiedNode<'db> {
    /// Creates a new underspecified BDD node, ensuring that it is fully reduced.
    fn new(
        db: &'db dyn Db,
        constraint: ConstrainedTypeVar<'db>,
        if_true: UnderspecifiedNode<'db>,
        if_false: UnderspecifiedNode<'db>,
    ) -> Self {
        debug_assert!(
            (if_true.root_constraint(db))
                .is_none_or(|root_constraint| root_constraint > constraint)
        );
        debug_assert!(
            (if_false.root_constraint(db))
                .is_none_or(|root_constraint| root_constraint > constraint)
        );
        if if_true == if_false {
            return if_true;
        }
        match (
            if_true.into_fully_specified(),
            if_false.into_fully_specified(),
        ) {
            (Some(if_true), Some(if_false)) => {
                Self::FullySpecified(InteriorNode::new(db, constraint, if_true, if_false))
            }
            _ => Self::Interior(UnderspecifiedInteriorNode::new(
                db, constraint, if_true, if_false,
            )),
        }
    }

    /// Creates a new BDD node for an individual constraint. (The BDD will evaluate to `true` when
    /// the constraint holds, and to `false` when it does not.)
    fn new_constraint(db: &'db dyn Db, constraint: ConstrainedTypeVar<'db>) -> Self {
        Self::FullySpecified(InteriorNode::new(
            db,
            constraint,
            Node::AlwaysTrue,
            Node::AlwaysFalse,
        ))
    }

    /// Creates a new BDD node for a positive or negative individual constraint. (For a positive
    /// constraint, this returns the same BDD node as [`new_constraint`][Self::new_constraint]. For
    /// a negative constraint, it returns the negation of that BDD node.)
    fn new_satisfied_constraint(db: &'db dyn Db, constraint: ConstraintAssignment<'db>) -> Self {
        match constraint {
            ConstraintAssignment::Positive(constraint) => Self::FullySpecified(InteriorNode::new(
                db,
                constraint,
                Node::AlwaysTrue,
                Node::AlwaysFalse,
            )),
            ConstraintAssignment::Negative(constraint) => Self::FullySpecified(InteriorNode::new(
                db,
                constraint,
                Node::AlwaysFalse,
                Node::AlwaysTrue,
            )),
        }
    }

    /// Returns the BDD variable of the root node of this BDD, or `None` if this BDD is a terminal
    /// node.
    fn root_constraint(self, db: &'db dyn Db) -> Option<ConstrainedTypeVar<'db>> {
        match self {
            UnderspecifiedNode::FullySpecified(interior) => Some(interior.constraint(db)),
            UnderspecifiedNode::Interior(interior) => Some(interior.constraint(db)),
            _ => None,
        }
    }

    /// Returns whether this BDD represent the constant function `true`.
    fn is_always_satisfied(self) -> bool {
        matches!(self, UnderspecifiedNode::AlwaysTrue)
    }

    /// Returns whether this BDD represent the constant function `false`.
    fn is_never_satisfied(self) -> bool {
        matches!(self, UnderspecifiedNode::AlwaysFalse)
    }

    /// If this BDD node is fully specified, returns the corresponding [`Node`]. Otherwise returns
    /// `None`.
    fn into_fully_specified(self) -> Option<Node<'db>> {
        // It might seem like this is only best-effort, since we're not doing a deep search of
        // `Interior` nodes to see if they actually have any path to the `Impossible` terminal. But
        // it's not — our constructor will always create `FullySpecified` variants whenever it can.
        // In effect, we're doing that search bottom-up at construction time.
        match self {
            UnderspecifiedNode::AlwaysFalse => Some(Node::AlwaysFalse),
            UnderspecifiedNode::AlwaysTrue => Some(Node::AlwaysTrue),
            UnderspecifiedNode::FullySpecified(interior) => Some(Node::Interior(interior)),
            _ => None,
        }
    }

    /// Returns the negation of this BDD.
    fn negate(self, db: &'db dyn Db) -> Self {
        match self {
            UnderspecifiedNode::AlwaysTrue => UnderspecifiedNode::AlwaysFalse,
            UnderspecifiedNode::AlwaysFalse => UnderspecifiedNode::AlwaysTrue,
            UnderspecifiedNode::Impossible => UnderspecifiedNode::Impossible,
            UnderspecifiedNode::FullySpecified(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).negate(db)
            }
            UnderspecifiedNode::Interior(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).negate(db)
            }
        }
    }

    /// Returns the `or` or union of two BDDs.
    fn or(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (UnderspecifiedNode::Impossible, _) | (_, UnderspecifiedNode::Impossible) => {
                UnderspecifiedNode::Impossible
            }
            (UnderspecifiedNode::AlwaysFalse, other) | (other, UnderspecifiedNode::AlwaysFalse) => {
                other
            }
            (UnderspecifiedNode::AlwaysTrue, UnderspecifiedNode::AlwaysTrue) => {
                UnderspecifiedNode::AlwaysTrue
            }
            (UnderspecifiedNode::FullySpecified(a), UnderspecifiedNode::FullySpecified(b)) => {
                // OR is commutative, which lets us halve the cache requirements
                let (a, b) = if b < a { (b, a) } else { (a, b) };
                PossiblySpecifiedInteriorNode::from(a).or(db, UnderspecifiedNode::FullySpecified(b))
            }
            (UnderspecifiedNode::Interior(a), UnderspecifiedNode::Interior(b)) => {
                // OR is commutative, which lets us halve the cache requirements
                let (a, b) = if b < a { (b, a) } else { (a, b) };
                PossiblySpecifiedInteriorNode::from(a).or(db, UnderspecifiedNode::Interior(b))
            }
            (other, UnderspecifiedNode::FullySpecified(interior))
            | (UnderspecifiedNode::FullySpecified(interior), other) => {
                PossiblySpecifiedInteriorNode::from(interior).or(db, other)
            }
            (other, UnderspecifiedNode::Interior(interior))
            | (UnderspecifiedNode::Interior(interior), other) => {
                PossiblySpecifiedInteriorNode::from(interior).or(db, other)
            }
        }
    }

    /// Returns the `and` or intersection of two BDDs.
    fn and(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (UnderspecifiedNode::Impossible, _) | (_, UnderspecifiedNode::Impossible) => {
                UnderspecifiedNode::Impossible
            }
            (UnderspecifiedNode::AlwaysTrue, other) | (other, UnderspecifiedNode::AlwaysTrue) => {
                other
            }
            (UnderspecifiedNode::AlwaysFalse, UnderspecifiedNode::AlwaysFalse) => {
                UnderspecifiedNode::AlwaysFalse
            }

            // at least one operand underspecified
            (UnderspecifiedNode::FullySpecified(a), UnderspecifiedNode::FullySpecified(b)) => {
                // AND is commutative, which lets us halve the cache requirements
                let (a, b) = if b < a { (b, a) } else { (a, b) };
                PossiblySpecifiedInteriorNode::from(a)
                    .and(db, UnderspecifiedNode::FullySpecified(b))
            }
            (UnderspecifiedNode::Interior(a), UnderspecifiedNode::Interior(b)) => {
                // AND is commutative, which lets us halve the cache requirements
                let (a, b) = if b < a { (b, a) } else { (a, b) };
                PossiblySpecifiedInteriorNode::from(a).and(db, UnderspecifiedNode::Interior(b))
            }
            (other, UnderspecifiedNode::FullySpecified(interior))
            | (UnderspecifiedNode::FullySpecified(interior), other) => {
                PossiblySpecifiedInteriorNode::from(interior).and(db, other)
            }
            (other, UnderspecifiedNode::Interior(interior))
            | (UnderspecifiedNode::Interior(interior), other) => {
                PossiblySpecifiedInteriorNode::from(interior).and(db, other)
            }
        }
    }

    /// Returns a new BDD that evaluates to `true` when both input BDDs evaluate to the same
    /// result.
    fn iff(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (UnderspecifiedNode::Impossible, _) | (_, UnderspecifiedNode::Impossible) => {
                UnderspecifiedNode::Impossible
            }
            (UnderspecifiedNode::AlwaysFalse, UnderspecifiedNode::AlwaysFalse)
            | (UnderspecifiedNode::AlwaysTrue, UnderspecifiedNode::AlwaysTrue) => {
                UnderspecifiedNode::AlwaysTrue
            }
            (UnderspecifiedNode::AlwaysTrue, UnderspecifiedNode::AlwaysFalse)
            | (UnderspecifiedNode::AlwaysFalse, UnderspecifiedNode::AlwaysTrue) => {
                UnderspecifiedNode::AlwaysFalse
            }
            (UnderspecifiedNode::FullySpecified(a), UnderspecifiedNode::FullySpecified(b)) => {
                // IFF is commutative, which lets us halve the cache requirements
                let (a, b) = if b < a { (b, a) } else { (a, b) };
                PossiblySpecifiedInteriorNode::from(a)
                    .iff(db, UnderspecifiedNode::FullySpecified(b))
            }
            (UnderspecifiedNode::Interior(a), UnderspecifiedNode::Interior(b)) => {
                // IFF is commutative, which lets us halve the cache requirements
                let (a, b) = if b < a { (b, a) } else { (a, b) };
                PossiblySpecifiedInteriorNode::from(a).iff(db, UnderspecifiedNode::Interior(b))
            }
            (other, UnderspecifiedNode::FullySpecified(interior))
            | (UnderspecifiedNode::FullySpecified(interior), other) => {
                PossiblySpecifiedInteriorNode::from(interior).iff(db, other)
            }
            (other, UnderspecifiedNode::Interior(interior))
            | (UnderspecifiedNode::Interior(interior), other) => {
                PossiblySpecifiedInteriorNode::from(interior).iff(db, other)
            }
        }
    }

    /// Returns the `if-then-else` of three BDDs: when `self` evaluates to `true`, it returns what
    /// `then_node` evaluates to; otherwise it returns what `else_node` evaluates to.
    fn ite(self, db: &'db dyn Db, then_node: Self, else_node: Self) -> Self {
        self.and(db, then_node)
            .or(db, self.negate(db).and(db, else_node))
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
            UnderspecifiedNode::AlwaysTrue => (UnderspecifiedNode::AlwaysTrue, false),
            UnderspecifiedNode::AlwaysFalse => (UnderspecifiedNode::AlwaysFalse, false),
            UnderspecifiedNode::Impossible => (UnderspecifiedNode::Impossible, false),
            UnderspecifiedNode::FullySpecified(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).restrict_one(db, assignment)
            }
            UnderspecifiedNode::Interior(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).restrict_one(db, assignment)
            }
        }
    }

    /// Returns a new BDD with any occurrence of `left ∧ right` replaced with `replacement`.
    fn substitute_intersection(
        self,
        db: &'db dyn Db,
        left: ConstraintAssignment<'db>,
        right: ConstraintAssignment<'db>,
        replacement: UnderspecifiedNode<'db>,
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
        let left_node = UnderspecifiedNode::new_satisfied_constraint(db, left);
        let right_node = UnderspecifiedNode::new_satisfied_constraint(db, right);
        let right_result =
            right_node.ite(db, UnderspecifiedNode::AlwaysFalse, when_left_but_not_right);
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
        right: ConstraintAssignment<'db>,
        replacement: UnderspecifiedNode<'db>,
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
        let left_node = UnderspecifiedNode::new_satisfied_constraint(db, left);
        let right_node = UnderspecifiedNode::new_satisfied_constraint(db, right);
        let validity = replacement.iff(db, left_node.or(db, right_node));
        let constrained_original = self.and(db, validity);
        let constrained_replacement = result.and(db, validity);
        if constrained_original == constrained_replacement {
            result
        } else {
            self
        }
    }

    /// Simplifies a BDD, replacing constraints with simpler or smaller constraints where possible.
    fn simplify(self, db: &'db dyn Db) -> Self {
        match self {
            UnderspecifiedNode::AlwaysTrue
            | UnderspecifiedNode::AlwaysFalse
            | UnderspecifiedNode::Impossible => self,
            UnderspecifiedNode::FullySpecified(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).simplify(db)
            }
            UnderspecifiedNode::Interior(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).simplify(db)
            }
        }
    }

    fn minimize(self, db: &'db dyn Db) -> Node<'db> {
        self.smallest_minimizations(db).take_one()
    }

    fn smallest_minimizations(self, db: &'db dyn Db) -> MinimizedNode<'db, 'db> {
        match self {
            UnderspecifiedNode::AlwaysTrue => MinimizedNode::One(Node::AlwaysTrue),
            UnderspecifiedNode::AlwaysFalse => MinimizedNode::One(Node::AlwaysFalse),
            UnderspecifiedNode::Impossible => {
                MinimizedNode::Two([Node::AlwaysTrue, Node::AlwaysFalse])
            }
            UnderspecifiedNode::FullySpecified(interior) => {
                MinimizedNode::One(Node::Interior(interior))
            }
            UnderspecifiedNode::Interior(interior) => MinimizedNode::Many(
                PossiblySpecifiedInteriorNode::from(interior)
                    .smallest_minimizations(db)
                    .as_ref(),
            ),
        }
    }

    fn display(self, db: &'db dyn Db) -> impl Display {
        self.minimize(db).display(db)
    }
}

impl<'db> From<Node<'db>> for UnderspecifiedNode<'db> {
    fn from(node: Node<'db>) -> Self {
        match node {
            Node::AlwaysFalse => UnderspecifiedNode::AlwaysFalse,
            Node::AlwaysTrue => UnderspecifiedNode::AlwaysTrue,
            Node::Interior(interior) => UnderspecifiedNode::FullySpecified(interior),
        }
    }
}

impl<'db> From<PossiblySpecifiedInteriorNode<'db>> for UnderspecifiedNode<'db> {
    fn from(node: PossiblySpecifiedInteriorNode<'db>) -> Self {
        match node {
            PossiblySpecifiedInteriorNode::FullySpecified(interior) => {
                UnderspecifiedNode::FullySpecified(interior)
            }
            PossiblySpecifiedInteriorNode::Underspecified(interior) => {
                UnderspecifiedNode::Interior(interior)
            }
        }
    }
}

impl<'db> From<UnderspecifiedInteriorNode<'db>> for UnderspecifiedNode<'db> {
    fn from(interior: UnderspecifiedInteriorNode<'db>) -> Self {
        UnderspecifiedNode::Interior(interior)
    }
}

enum MinimizedNode<'a, 'db> {
    One(Node<'db>),
    Two([Node<'db>; 2]),
    Many(&'a [Node<'db>]),
}

impl<'a, 'db> MinimizedNode<'a, 'db> {
    fn take_one(self) -> Node<'db> {
        match self {
            MinimizedNode::One(node) | MinimizedNode::Two([node, _]) => node,
            MinimizedNode::Many(nodes) => nodes
                .first()
                .copied()
                .expect("should always be able to minimize BDD"),
        }
    }

    fn as_slice(&'a self) -> &'a [Node<'db>] {
        match self {
            MinimizedNode::One(node) => std::slice::from_ref(node),
            MinimizedNode::Two(nodes) => nodes.as_slice(),
            MinimizedNode::Many(nodes) => nodes,
        }
    }
}

/// An interior node of an underspecified BDD
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(Ord, PartialOrd)]
struct UnderspecifiedInteriorNode<'db> {
    constraint: ConstrainedTypeVar<'db>,
    if_true: UnderspecifiedNode<'db>,
    if_false: UnderspecifiedNode<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for UnderspecifiedInteriorNode<'_> {}

/// The ordering of an interior node with another node, along with the typevar of the smaller node.
/// If the "left" interior node is greater than or equal to, then the right node must also be an
/// interior node, and we include that in the result too.
enum NodeOrdering<'db> {
    Less(ConstrainedTypeVar<'db>),
    Equal(ConstrainedTypeVar<'db>, PossiblySpecifiedInteriorNode<'db>),
    Greater(ConstrainedTypeVar<'db>, PossiblySpecifiedInteriorNode<'db>),
}

impl<'db> NodeOrdering<'db> {
    fn from_constraints(
        db: &'db dyn Db,
        left_constraint: ConstrainedTypeVar<'db>,
        right: PossiblySpecifiedInteriorNode<'db>,
    ) -> NodeOrdering<'db> {
        let right_constraint = right.constraint(db);
        match left_constraint.cmp(&right_constraint) {
            Ordering::Less => NodeOrdering::Less(left_constraint),
            Ordering::Equal => NodeOrdering::Equal(left_constraint, right),
            Ordering::Greater => NodeOrdering::Greater(right_constraint, right),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Supertype)]
enum PossiblySpecifiedInteriorNode<'db> {
    Underspecified(UnderspecifiedInteriorNode<'db>),
    FullySpecified(InteriorNode<'db>),
}

#[salsa::tracked]
impl<'db> PossiblySpecifiedInteriorNode<'db> {
    fn constraint(self, db: &'db dyn Db) -> ConstrainedTypeVar<'db> {
        match self {
            PossiblySpecifiedInteriorNode::Underspecified(interior) => interior.constraint(db),
            PossiblySpecifiedInteriorNode::FullySpecified(interior) => interior.constraint(db),
        }
    }

    fn if_true(self, db: &'db dyn Db) -> UnderspecifiedNode<'db> {
        match self {
            PossiblySpecifiedInteriorNode::Underspecified(interior) => interior.if_true(db),
            PossiblySpecifiedInteriorNode::FullySpecified(interior) => interior.if_true(db).into(),
        }
    }

    fn if_false(self, db: &'db dyn Db) -> UnderspecifiedNode<'db> {
        match self {
            PossiblySpecifiedInteriorNode::Underspecified(interior) => interior.if_false(db),
            PossiblySpecifiedInteriorNode::FullySpecified(interior) => interior.if_false(db).into(),
        }
    }

    fn cmp_constraints(self, db: &'db dyn Db, other: UnderspecifiedNode<'db>) -> NodeOrdering<'db> {
        let self_constraint = self.constraint(db);
        match other {
            // Terminal nodes come after all interior nodes
            UnderspecifiedNode::AlwaysTrue
            | UnderspecifiedNode::AlwaysFalse
            | UnderspecifiedNode::Impossible => NodeOrdering::Less(self_constraint),
            UnderspecifiedNode::FullySpecified(other) => {
                NodeOrdering::from_constraints(db, self_constraint, other.into())
            }
            UnderspecifiedNode::Interior(other) => {
                NodeOrdering::from_constraints(db, self_constraint, other.into())
            }
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn negate(self, db: &'db dyn Db) -> UnderspecifiedNode<'db> {
        UnderspecifiedNode::new(
            db,
            self.constraint(db),
            self.if_true(db).negate(db),
            self.if_false(db).negate(db),
        )
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn or(self, db: &'db dyn Db, other: UnderspecifiedNode<'db>) -> UnderspecifiedNode<'db> {
        match self.cmp_constraints(db, other) {
            NodeOrdering::Equal(constraint, other) => UnderspecifiedNode::new(
                db,
                constraint,
                self.if_true(db).or(db, other.if_true(db)),
                self.if_false(db).or(db, other.if_false(db)),
            ),
            NodeOrdering::Less(constraint) => UnderspecifiedNode::new(
                db,
                constraint,
                self.if_true(db).or(db, other),
                self.if_false(db).or(db, other),
            ),
            NodeOrdering::Greater(constraint, other) => UnderspecifiedNode::new(
                db,
                constraint,
                self.or(db, other.if_true(db)),
                self.or(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn and(self, db: &'db dyn Db, other: UnderspecifiedNode<'db>) -> UnderspecifiedNode<'db> {
        match self.cmp_constraints(db, other) {
            NodeOrdering::Equal(constraint, other) => UnderspecifiedNode::new(
                db,
                constraint,
                self.if_true(db).and(db, other.if_true(db)),
                self.if_false(db).and(db, other.if_false(db)),
            ),
            NodeOrdering::Less(constraint) => UnderspecifiedNode::new(
                db,
                constraint,
                self.if_true(db).and(db, other),
                self.if_false(db).and(db, other),
            ),
            NodeOrdering::Greater(constraint, other) => UnderspecifiedNode::new(
                db,
                constraint,
                self.and(db, other.if_true(db)),
                self.and(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn iff(self, db: &'db dyn Db, other: UnderspecifiedNode<'db>) -> UnderspecifiedNode<'db> {
        match self.cmp_constraints(db, other) {
            NodeOrdering::Equal(constraint, other) => UnderspecifiedNode::new(
                db,
                constraint,
                self.if_true(db).iff(db, other.if_true(db)),
                self.if_false(db).iff(db, other.if_false(db)),
            ),
            NodeOrdering::Less(constraint) => UnderspecifiedNode::new(
                db,
                constraint,
                self.if_true(db).iff(db, other),
                self.if_false(db).iff(db, other),
            ),
            NodeOrdering::Greater(constraint, other) => UnderspecifiedNode::new(
                db,
                constraint,
                self.iff(db, other.if_true(db)),
                self.iff(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn restrict_one(
        self,
        db: &'db dyn Db,
        assignment: ConstraintAssignment<'db>,
    ) -> (UnderspecifiedNode<'db>, bool) {
        // If this node's variable is larger than the assignment's variable, then we have reached a
        // point in the BDD where the assignment can no longer affect the result,
        // and we can return early.
        let self_constraint = self.constraint(db);
        if assignment.constraint() < self_constraint {
            return (UnderspecifiedNode::from(self), false);
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
                UnderspecifiedNode::new(db, self_constraint, if_true, if_false),
                found_in_true || found_in_false,
            )
        }
    }

    /// Invokes a closure for each constraint variable that appears anywhere in a BDD. (Any given
    /// constraint can appear multiple times in different paths from the root; we do not
    /// deduplicate those constraints, and will instead invoke the callback each time we encounter
    /// the constraint.)
    fn for_each_constraint(self, db: &'db dyn Db, f: &mut dyn FnMut(ConstrainedTypeVar<'db>)) {
        f(self.constraint(db));
        match self.if_true(db) {
            UnderspecifiedNode::Interior(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).for_each_constraint(db, f)
            }
            UnderspecifiedNode::FullySpecified(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).for_each_constraint(db, f)
            }
            _ => {}
        }
        match self.if_false(db) {
            UnderspecifiedNode::Interior(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).for_each_constraint(db, f)
            }
            UnderspecifiedNode::FullySpecified(interior) => {
                PossiblySpecifiedInteriorNode::from(interior).for_each_constraint(db, f)
            }
            _ => {}
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn simplify(self, db: &'db dyn Db) -> UnderspecifiedNode<'db> {
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
        self.for_each_constraint(db, &mut |constraint| {
            seen_constraints.insert(constraint);
        });
        let mut to_visit: Vec<(_, _)> = (seen_constraints.iter().copied())
            .tuple_combinations()
            .collect();

        // Repeatedly pop constraint pairs off of the visit queue, checking whether each pair can
        // be simplified.
        let mut simplified = UnderspecifiedNode::from(self);
        while let Some((left_constraint, right_constraint)) = to_visit.pop() {
            // If the constraints refer to different typevars, they trivially cannot be compared.
            // TODO: We might need to consider when one constraint's upper or lower bound refers to
            // the other constraint's typevar.
            let typevar = left_constraint.typevar(db);
            if typevar != right_constraint.typevar(db) {
                continue;
            }

            // Containment: The range of one constraint might completely contain the range of the
            // other. If so, there are several potential simplifications.
            let larger_smaller = if left_constraint.contains(db, right_constraint) {
                Some((left_constraint, right_constraint))
            } else if right_constraint.contains(db, left_constraint) {
                Some((right_constraint, left_constraint))
            } else {
                None
            };
            if let Some((larger_constraint, smaller_constraint)) = larger_smaller {
                // larger ∨ smaller = larger
                simplified = simplified.substitute_union(
                    db,
                    larger_constraint.when_true(),
                    smaller_constraint.when_true(),
                    UnderspecifiedNode::new_satisfied_constraint(db, larger_constraint.when_true()),
                );

                // ¬larger ∧ ¬smaller = ¬larger
                simplified = simplified.substitute_intersection(
                    db,
                    larger_constraint.when_false(),
                    smaller_constraint.when_false(),
                    UnderspecifiedNode::new_satisfied_constraint(
                        db,
                        larger_constraint.when_false(),
                    ),
                );

                // smaller ∧ ¬larger = false
                // (¬larger removes everything that's present in smaller)
                simplified = simplified.substitute_intersection(
                    db,
                    larger_constraint.when_false(),
                    smaller_constraint.when_true(),
                    UnderspecifiedNode::AlwaysFalse,
                );

                // larger ∨ ¬smaller = true
                // (larger fills in everything that's missing in ¬smaller)
                simplified = simplified.substitute_union(
                    db,
                    larger_constraint.when_true(),
                    smaller_constraint.when_false(),
                    UnderspecifiedNode::AlwaysTrue,
                );
            }

            // There are some simplifications we can make when the intersection of the two
            // constraints is empty, and others that we can make when the intersection is
            // non-empty.
            match left_constraint.intersect(db, right_constraint) {
                Some(intersection_constraint) => {
                    // If the intersection is non-empty, we need to create a new constraint to
                    // represent that intersection. We also need to add the new constraint to our
                    // seen set and (if we haven't already seen it) to the to-visit queue.
                    if seen_constraints.insert(intersection_constraint) {
                        to_visit.extend(
                            (seen_constraints.iter().copied())
                                .filter(|seen| *seen != intersection_constraint)
                                .map(|seen| (seen, intersection_constraint)),
                        );
                    }
                    let positive_intersection_node = UnderspecifiedNode::new_satisfied_constraint(
                        db,
                        intersection_constraint.when_true(),
                    );
                    let negative_intersection_node = UnderspecifiedNode::new_satisfied_constraint(
                        db,
                        intersection_constraint.when_false(),
                    );

                    // left ∧ right = intersection
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_true(),
                        right_constraint.when_true(),
                        positive_intersection_node,
                    );

                    // ¬left ∨ ¬right = ¬intersection
                    simplified = simplified.substitute_union(
                        db,
                        left_constraint.when_false(),
                        right_constraint.when_false(),
                        negative_intersection_node,
                    );

                    // left ∧ ¬right = left ∧ ¬intersection
                    // (clip the negative constraint to the smallest range that actually removes
                    // something from positive constraint)
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_true(),
                        right_constraint.when_false(),
                        UnderspecifiedNode::new_satisfied_constraint(
                            db,
                            left_constraint.when_true(),
                        )
                        .and(db, negative_intersection_node),
                    );

                    // ¬left ∧ right = ¬intersection ∧ right
                    // (save as above but reversed)
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_false(),
                        right_constraint.when_true(),
                        UnderspecifiedNode::new_satisfied_constraint(
                            db,
                            right_constraint.when_true(),
                        )
                        .and(db, negative_intersection_node),
                    );

                    // left ∨ ¬right = intersection ∨ ¬right
                    // (clip the positive constraint to the smallest range that actually adds
                    // something to the negative constraint)
                    simplified = simplified.substitute_union(
                        db,
                        left_constraint.when_true(),
                        right_constraint.when_false(),
                        UnderspecifiedNode::new_satisfied_constraint(
                            db,
                            right_constraint.when_false(),
                        )
                        .or(db, positive_intersection_node),
                    );

                    // ¬left ∨ right = ¬left ∨ intersection
                    // (save as above but reversed)
                    simplified = simplified.substitute_union(
                        db,
                        left_constraint.when_false(),
                        right_constraint.when_true(),
                        UnderspecifiedNode::new_satisfied_constraint(
                            db,
                            left_constraint.when_false(),
                        )
                        .or(db, positive_intersection_node),
                    );
                }

                None => {
                    // All of the below hold because we just proved that the intersection of left
                    // and right is empty.

                    // left ∧ right = false
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_true(),
                        right_constraint.when_true(),
                        UnderspecifiedNode::AlwaysFalse,
                    );

                    // ¬left ∨ ¬right = true
                    simplified = simplified.substitute_union(
                        db,
                        left_constraint.when_false(),
                        right_constraint.when_false(),
                        UnderspecifiedNode::AlwaysTrue,
                    );

                    // left ∧ ¬right = left
                    // (there is nothing in the hole of ¬right that overlaps with left)
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_true(),
                        right_constraint.when_false(),
                        UnderspecifiedNode::new_constraint(db, left_constraint),
                    );

                    // ¬left ∧ right = right
                    // (save as above but reversed)
                    simplified = simplified.substitute_intersection(
                        db,
                        left_constraint.when_false(),
                        right_constraint.when_true(),
                        UnderspecifiedNode::new_constraint(db, right_constraint),
                    );
                }
            }
        }

        simplified
    }

    #[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
    fn smallest_minimizations(self, db: &'db dyn Db) -> Box<[Node<'db>]> {
        let constraint = self.constraint(db);
        let if_true = self.if_true(db).smallest_minimizations(db);
        let if_false = self.if_false(db).smallest_minimizations(db);
        let mut minimizations =
            Vec::with_capacity(if_true.as_slice().len() * if_false.as_slice().len());
        for if_true in if_true.as_slice() {
            for if_false in if_false.as_slice() {
                minimizations.push(Node::new(db, constraint, *if_true, *if_false));
            }
        }
        let minimum_size = minimizations
            .iter()
            .map(|node| node.interior_node_count(db))
            .min()
            .unwrap_or_default();
        minimizations.retain(|node| node.interior_node_count(db) == minimum_size);
        minimizations.into_boxed_slice()
    }
}

impl<'db> From<UnderspecifiedInteriorNode<'db>> for PossiblySpecifiedInteriorNode<'db> {
    fn from(interior: UnderspecifiedInteriorNode<'db>) -> Self {
        PossiblySpecifiedInteriorNode::Underspecified(interior)
    }
}

impl<'db> From<InteriorNode<'db>> for PossiblySpecifiedInteriorNode<'db> {
    fn from(interior: InteriorNode<'db>) -> Self {
        PossiblySpecifiedInteriorNode::FullySpecified(interior)
    }
}

/// An assignment of one BDD variable to either `true` or `false`. (When evaluating a BDD, we
/// must provide an assignment for each variable present in the BDD.)
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ConstraintAssignment<'db> {
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

    // Keep this for future debugging needs, even though it's not currently used when rendering
    // constraint sets.
    #[expect(dead_code)]
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

    fn display(&self, db: &'db dyn Db) -> String {
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
    fn simplify(&mut self) {
        while self.simplify_one_round() {
            // Keep going
        }
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
            return String::from("always");
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
