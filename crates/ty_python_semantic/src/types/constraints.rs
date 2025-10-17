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
//! Lower and upper bounds must also be normalized. This lets us identify, for instance,
//! two constraints with equivalent but differently ordered unions as their bounds.
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
use salsa::plumbing::AsId;

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
#[derive(Clone, Copy, Debug, Hash, get_size2::GetSize, salsa::Update)]
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
        self.node = self.node.or(db, other.node);
        *self
    }

    /// Updates this constraint set to hold the intersection of itself and another constraint set.
    pub(crate) fn intersect(&mut self, db: &'db dyn Db, other: Self) -> Self {
        self.node = self.node.and(db, other.node);
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
            node: ConstrainedTypeVar::new_node(db, lower, typevar, upper),
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
        if lower.is_never() && upper.is_object() {
            return Node::AlwaysTrue;
        }

        let lower = lower.normalized(db);
        let upper = upper.normalized(db);
        Node::new_constraint(db, ConstrainedTypeVar::new(db, typevar, lower, upper))
    }

    fn when_true(self) -> ConstraintAssignment<'db> {
        ConstraintAssignment::Positive(self)
    }

    fn when_false(self) -> ConstraintAssignment<'db> {
        ConstraintAssignment::Negative(self)
    }

    fn cmp(self, db: &'db dyn Db, other: Self) -> Ordering {
        self.typevar(db)
            .cmp(&other.typevar(db))
            .then_with(|| self.as_id().cmp(&other.as_id()))
    }

    fn implies(self, db: &'db dyn Db, other: Self) -> bool {
        if self.typevar(db) != other.typevar(db) {
            return false;
        }
        other.lower(db).is_subtype_of(db, self.lower(db))
            && self.upper(db).is_subtype_of(db, other.upper(db))
    }

    /// Returns the intersection of two range constraints, or `None` if the intersection is empty.
    fn intersect(self, db: &'db dyn Db, other: Self) -> Option<Self> {
        // (s₁ ≤ α ≤ t₁) ∧ (s₂ ≤ α ≤ t₂) = (s₁ ∪ s₂) ≤ α ≤ (t₁ ∩ t₂))
        let lower = UnionType::from_elements(db, [self.lower(db), other.lower(db)]).normalized(db);
        let upper =
            IntersectionType::from_elements(db, [self.upper(db), other.upper(db)]).normalized(db);

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
    Impossible,
    Interior(InteriorNode<'db>),
}

impl<'db> Node<'db> {
    /// Creates a new BDD node, ensuring that it is fully reduced.
    fn new(
        db: &'db dyn Db,
        constraint: ConstrainedTypeVar<'db>,
        if_true: Node<'db>,
        if_false: Node<'db>,
    ) -> Self {
        debug_assert!((if_true.root_constraint(db)).is_none_or(|root_constraint| {
            (root_constraint.cmp(db, constraint)) == Ordering::Greater
        }));
        debug_assert!(
            (if_false.root_constraint(db)).is_none_or(|root_constraint| {
                (root_constraint.cmp(db, constraint)) == Ordering::Greater
            })
        );
        if if_true == if_false {
            return if_true;
        }
        Self::Interior(InteriorNode::new(db, constraint, if_true, if_false))
    }

    /// Creates a new BDD node for an individual constraint. (The BDD will evaluate to `true` when
    /// the constraint holds, and to `false` when it does not.)
    fn new_constraint(db: &'db dyn Db, constraint: ConstrainedTypeVar<'db>) -> Self {
        Self::Interior(InteriorNode::new(
            db,
            constraint,
            Node::AlwaysTrue,
            Node::AlwaysFalse,
        ))
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
            Node::AlwaysTrue | Node::AlwaysFalse | Node::Impossible => 0,
            Node::Interior(interior) => interior.interior_node_count(db),
        }
    }

    /// Returns whether this BDD represent the constant function `true`.
    fn is_always_satisfied(self) -> bool {
        matches!(self, Node::AlwaysTrue)
    }

    /// Returns whether this BDD represent the constant function `false`.
    fn is_never_satisfied(self) -> bool {
        matches!(self, Node::AlwaysFalse)
    }

    /// Given a BDD that evaluates to `false` for some conditions, returns a new BDD where those
    /// conditions are `impossible`.
    fn impossible(self, db: &'db dyn Db) -> Self {
        match self {
            Node::AlwaysTrue => Node::AlwaysTrue,
            Node::AlwaysFalse | Node::Impossible => Node::Impossible,
            Node::Interior(interior) => interior.impossible(db),
        }
    }

    /// Returns the negation of this BDD.
    fn negate(self, db: &'db dyn Db) -> Self {
        match self {
            Node::AlwaysTrue => Node::AlwaysFalse,
            Node::AlwaysFalse => Node::AlwaysTrue,
            Node::Impossible => Node::Impossible,
            Node::Interior(interior) => interior.negate(db),
        }
    }

    /// Returns the `or` or union of two BDDs.
    fn or(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::Impossible, _) | (_, Node::Impossible) => Node::Impossible,
            (Node::AlwaysTrue, Node::AlwaysTrue)
            | (Node::AlwaysTrue, Node::AlwaysFalse)
            | (Node::AlwaysFalse, Node::AlwaysTrue) => Node::AlwaysTrue,
            (Node::AlwaysFalse, Node::AlwaysFalse) => Node::AlwaysFalse,
            // OR is commutative, which lets us halve the cache requirements
            (Node::Interior(a), Node::Interior(b)) => {
                let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
                a.or(db, Node::Interior(b))
            }
            (Node::Interior(interior), terminal @ _) | (terminal @ _, Node::Interior(interior)) => {
                interior.or(db, terminal)
            }
        }
    }

    /// Returns the `and` or intersection of two BDDs.
    fn and(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::Impossible, _) | (_, Node::Impossible) => Node::Impossible,
            (Node::AlwaysFalse, Node::AlwaysFalse)
            | (Node::AlwaysTrue, Node::AlwaysFalse)
            | (Node::AlwaysFalse, Node::AlwaysTrue) => Node::AlwaysFalse,
            (Node::AlwaysTrue, Node::AlwaysTrue) => Node::AlwaysTrue,
            // OR is commutative, which lets us halve the cache requirements
            (Node::Interior(a), Node::Interior(b)) => {
                let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
                a.and(db, Node::Interior(b))
            }
            (Node::Interior(interior), terminal @ _) | (terminal @ _, Node::Interior(interior)) => {
                interior.and(db, terminal)
            }
        }
    }

    /// Returns the `xor` of two BDDs.
    fn xor(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::Impossible, _) | (_, Node::Impossible) => Node::Impossible,
            (Node::AlwaysTrue, Node::AlwaysFalse) | (Node::AlwaysFalse, Node::AlwaysTrue) => {
                Node::AlwaysTrue
            }
            (Node::AlwaysFalse, Node::AlwaysFalse) | (Node::AlwaysTrue, Node::AlwaysTrue) => {
                Node::AlwaysFalse
            }
            // XOR is commutative, which lets us halve the cache requirements
            (Node::Interior(a), Node::Interior(b)) => {
                let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
                a.xor(db, Node::Interior(b))
            }
            (Node::Interior(interior), terminal @ _) | (terminal @ _, Node::Interior(interior)) => {
                interior.xor(db, terminal)
            }
        }
    }

    /// Returns a new BDD that evaluates to `true` when both input BDDs evaluate to the same
    /// result.
    fn iff(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (Node::Impossible, _) | (_, Node::Impossible) => Node::Impossible,
            (Node::AlwaysFalse, Node::AlwaysFalse) | (Node::AlwaysTrue, Node::AlwaysTrue) => {
                Node::AlwaysTrue
            }
            (Node::AlwaysTrue, Node::AlwaysFalse) | (Node::AlwaysFalse, Node::AlwaysTrue) => {
                Node::AlwaysFalse
            }
            // IFF is commutative, which lets us halve the cache requirements
            (Node::Interior(a), Node::Interior(b)) => {
                let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
                a.iff(db, Node::Interior(b))
            }
            (Node::Interior(interior), terminal @ _) | (terminal @ _, Node::Interior(interior)) => {
                interior.iff(db, terminal)
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
            Node::AlwaysTrue => (Node::AlwaysTrue, false),
            Node::AlwaysFalse => (Node::AlwaysFalse, false),
            Node::Impossible => (Node::Impossible, false),
            Node::Interior(interior) => interior.restrict_one(db, assignment),
        }
    }

    /// Invokes a closure for each constraint variable that appears anywhere in a BDD. (Any given
    /// constraint can appear multiple times in different paths from the root; we do not
    /// deduplicate those constraints, and will instead invoke the callback each time we encounter
    /// the constraint.)
    fn for_each_constraint(self, db: &'db dyn Db, f: &mut dyn FnMut(ConstrainedTypeVar<'db>)) {
        let Node::Interior(interior) = self else {
            return;
        };
        f(interior.constraint(db));
        interior.if_true(db).for_each_constraint(db, f);
        interior.if_false(db).for_each_constraint(db, f);
    }

    /// Simplifies a BDD, by finding all invalid inputs and ensuring that the BDD always maps those
    /// to false.
    ///
    /// An input can be invalid because BDD variables represent constraints, and certain
    /// combinations constraints might be impossible. For instance, `T ≤ bool` implies `T ≤ int`,
    /// so we don't need to care what the BDD evaluates to when `T ≤ bool ∧ T ≰ int`, since that is
    /// not a valid combination of constraints.
    fn simplify(self, db: &'db dyn Db) -> Self {
        match self {
            Node::AlwaysTrue | Node::AlwaysFalse | Node::Impossible => self,
            Node::Interior(interior) => interior.simplify(db),
        }
    }

    fn minimize(self, db: &'db dyn Db) -> Node<'db> {
        self.minimizations(db).take_one()
    }

    fn minimizations(self, db: &'db dyn Db) -> MinimizedNode<'db, 'db> {
        match self {
            Node::AlwaysTrue => MinimizedNode::One(Node::AlwaysTrue),
            Node::AlwaysFalse => MinimizedNode::One(Node::AlwaysFalse),
            Node::Impossible => MinimizedNode::Two([Node::AlwaysFalse, Node::AlwaysTrue]),
            Node::Interior(interior) => MinimizedNode::Many(interior.minimizations(db).as_ref()),
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
                    Node::AlwaysFalse | Node::Impossible => {}
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
                    Node::Impossible => f.write_str("impossible"),
                    Node::Interior(_) => {
                        let mut clauses = self.node.satisfied_clauses(self.db);
                        clauses.simplify(self.db);
                        clauses.display(self.db).fmt(f)
                    }
                }
            }
        }

        let simplified = self.simplify(db);
        let minimized = simplified.minimize(db);
        let d = DisplayNode {
            node: self.simplify(db).minimize(db),
            db,
        }
        .to_string();

        eprintln!("====> display");
        eprintln!(" ---> original");
        eprintln!("  {}", self.display_graph(db, &"  "));
        eprintln!(" ---> simplified");
        eprintln!("  {}", simplified.display_graph(db, &"  "));
        eprintln!(" ---> minimized");
        eprintln!("  {}", minimized.display_graph(db, &"  "));
        eprintln!(" ---> rendered");
        eprintln!("  {d}");

        d
    }

    // Keep this around for debugging purposes
    #[expect(dead_code)]
    fn display_graph(self, db: &'db dyn Db, prefix: &dyn Display) -> impl Display {
        struct DisplayNode<'a, 'db> {
            node: Node<'db>,
            db: &'db dyn Db,
            prefix: &'a dyn Display,
        }

        impl Display for DisplayNode<'_, '_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.node {
                    Node::AlwaysTrue => write!(f, "always"),
                    Node::AlwaysFalse => write!(f, "never"),
                    Node::Impossible => write!(f, "impossible"),
                    Node::Interior(interior) => {
                        interior.constraint(self.db).display(self.db).fmt(f)?;
                        write!(
                            f,
                            "\n{}┡━₁ {}",
                            self.prefix,
                            interior
                                .if_true(self.db)
                                .display_graph(self.db, &format_args!("{}│   ", self.prefix))
                        )?;
                        write!(
                            f,
                            "\n{}└─₀ {}",
                            self.prefix,
                            interior
                                .if_false(self.db)
                                .display_graph(self.db, &format_args!("{}    ", self.prefix))
                        )?;
                        Ok(())
                    }
                }
            }
        }

        DisplayNode {
            node: self,
            db,
            prefix,
        }
    }
}

/// The ordering of an interior node with another node, along with the typevar of the smaller node.
/// If the "left" interior node is greater than or equal to, then the right node must also be an
/// interior node, and we include that in the result too.
enum NodeOrdering<'db> {
    Less(ConstrainedTypeVar<'db>),
    Equal(ConstrainedTypeVar<'db>, InteriorNode<'db>),
    Greater(ConstrainedTypeVar<'db>, InteriorNode<'db>),
}

impl<'db> NodeOrdering<'db> {
    fn from_constraints(
        db: &'db dyn Db,
        left_constraint: ConstrainedTypeVar<'db>,
        right: InteriorNode<'db>,
    ) -> NodeOrdering<'db> {
        let right_constraint = right.constraint(db);
        match left_constraint.cmp(db, right_constraint) {
            Ordering::Less => NodeOrdering::Less(left_constraint),
            Ordering::Equal => NodeOrdering::Equal(left_constraint, right),
            Ordering::Greater => NodeOrdering::Greater(right_constraint, right),
        }
    }
}

/// An interior node of a BDD
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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

    fn cmp_constraints(self, db: &'db dyn Db, other: Node<'db>) -> NodeOrdering<'db> {
        let self_constraint = self.constraint(db);
        match other {
            // Terminal nodes come after all interior nodes
            Node::AlwaysTrue | Node::AlwaysFalse | Node::Impossible => {
                NodeOrdering::Less(self_constraint)
            }
            Node::Interior(other) => NodeOrdering::from_constraints(db, self_constraint, other),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn impossible(self, db: &'db dyn Db) -> Node<'db> {
        Node::new(
            db,
            self.constraint(db),
            self.if_true(db).impossible(db),
            self.if_false(db).impossible(db),
        )
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn negate(self, db: &'db dyn Db) -> Node<'db> {
        Node::new(
            db,
            self.constraint(db),
            self.if_true(db).negate(db),
            self.if_false(db).negate(db),
        )
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn or(self, db: &'db dyn Db, other: Node<'db>) -> Node<'db> {
        match self.cmp_constraints(db, other) {
            NodeOrdering::Equal(constraint, other) => Node::new(
                db,
                constraint,
                self.if_true(db).or(db, other.if_true(db)),
                self.if_false(db).or(db, other.if_false(db)),
            ),
            NodeOrdering::Less(constraint) => Node::new(
                db,
                constraint,
                self.if_true(db).or(db, other),
                self.if_false(db).or(db, other),
            ),
            NodeOrdering::Greater(constraint, other) => Node::new(
                db,
                constraint,
                self.or(db, other.if_true(db)),
                self.or(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn and(self, db: &'db dyn Db, other: Node<'db>) -> Node<'db> {
        match self.cmp_constraints(db, other) {
            NodeOrdering::Equal(constraint, other) => Node::new(
                db,
                constraint,
                self.if_true(db).and(db, other.if_true(db)),
                self.if_false(db).and(db, other.if_false(db)),
            ),
            NodeOrdering::Less(constraint) => Node::new(
                db,
                constraint,
                self.if_true(db).and(db, other),
                self.if_false(db).and(db, other),
            ),
            NodeOrdering::Greater(constraint, other) => Node::new(
                db,
                constraint,
                self.and(db, other.if_true(db)),
                self.and(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn xor(self, db: &'db dyn Db, other: Node<'db>) -> Node<'db> {
        match self.cmp_constraints(db, other) {
            NodeOrdering::Equal(constraint, other) => Node::new(
                db,
                constraint,
                self.if_true(db).xor(db, other.if_true(db)),
                self.if_false(db).xor(db, other.if_false(db)),
            ),
            NodeOrdering::Less(constraint) => Node::new(
                db,
                constraint,
                self.if_true(db).xor(db, other),
                self.if_false(db).xor(db, other),
            ),
            NodeOrdering::Greater(constraint, other) => Node::new(
                db,
                constraint,
                self.xor(db, other.if_true(db)),
                self.xor(db, other.if_false(db)),
            ),
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn iff(self, db: &'db dyn Db, other: Node<'db>) -> Node<'db> {
        match self.cmp_constraints(db, other) {
            NodeOrdering::Equal(constraint, other) => Node::new(
                db,
                constraint,
                self.if_true(db).iff(db, other.if_true(db)),
                self.if_false(db).iff(db, other.if_false(db)),
            ),
            NodeOrdering::Less(constraint) => Node::new(
                db,
                constraint,
                self.if_true(db).iff(db, other),
                self.if_false(db).iff(db, other),
            ),
            NodeOrdering::Greater(constraint, other) => Node::new(
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
    ) -> (Node<'db>, bool) {
        // If this node's variable is larger than the assignment's variable, then we have reached a
        // point in the BDD where the assignment can no longer affect the result,
        // and we can return early.
        let self_constraint = self.constraint(db);
        if assignment.constraint().cmp(db, self_constraint) == Ordering::Less {
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
                Node::new(db, self_constraint, if_true, if_false),
                found_in_true || found_in_false,
            )
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn simplify(self, db: &'db dyn Db) -> Node<'db> {
        // To simplify a non-terminal BDD, we construct a new BDD representing the domain of valid
        // inputs. For instance, assume we have BDD variables `x` representing `T ≤ bool` and `y`
        // representing `T ≤ int`. Since `bool ≤ int`, `x → y` must always be true, so we will add
        // it to the domain BDD. (Or more accurately, we will _remove_ its negation `x ∧ ¬y` from
        // the domain BDD.)
        let mut all_constraints = FxHashSet::default();
        let mut simplified = Node::Interior(self);
        simplified.for_each_constraint(db, &mut |constraint| {
            all_constraints.insert(constraint);
        });

        let mut domain = Node::AlwaysTrue;
        for (&left, &right) in all_constraints.iter().tuple_combinations() {
            if left.typevar(db) != right.typevar(db) {
                continue;
            }

            let left_constraint = Node::new_constraint(db, left);
            let right_constraint = Node::new_constraint(db, right);

            if left.implies(db, right) {
                // left → right = ¬left ∨ right
                let implication = left_constraint.negate(db).or(db, right_constraint);
                domain = domain.and(db, implication.impossible(db));
            } else if right.implies(db, left) {
                // right → left = ¬right ∨ left
                let implication = right_constraint.negate(db).or(db, left_constraint);
                domain = domain.and(db, implication.impossible(db));
            }

            match left.intersect(db, right) {
                Some(intersection) => {
                    if intersection != left && intersection != right {
                        // (left ∧ right) → intersection

                        // If the intersection is non-empty, we want to add the resulting
                        // implication (shown above) to the domain, just like with all of the other
                        // simplifications.
                        let intersection_constraint = Node::new_constraint(db, intersection);
                        let implication = (left_constraint.negate(db))
                            .or(db, right_constraint.negate(db))
                            .or(db, intersection_constraint);
                        domain = domain.and(db, implication.impossible(db));

                        // But we also want to perform some replacements:
                        //   - `left ∧ right` becomes `intersection`
                        //   - `left ∧ ¬right` becomes `left ∧ ¬intersection`
                        //   - `¬left ∧ right` becomes `¬intersection ∧ right`
                        //
                        // To do that, we construct new BDDs that for each, of the form
                        // `if RHS then LHS else false`, and OR those together with the original
                        // BDD. That takes care of adding each replacement to the result;
                        // intersecting with the domain below will take care of removing each LHS.
                        let (left_and_right, _) = Node::Interior(self)
                            .restrict(db, [left.when_true(), right.when_true()]);
                        let (left_and_not_right, _) = Node::Interior(self)
                            .restrict(db, [left.when_true(), right.when_false()]);
                        let (not_left_and_right, _) = Node::Interior(self)
                            .restrict(db, [left.when_false(), right.when_true()]);
                        let replacement = intersection_constraint.ite(
                            db,
                            left_and_right,
                            left_constraint.ite(
                                db,
                                left_and_not_right,
                                right_constraint.ite(db, not_left_and_right, Node::AlwaysFalse),
                            ),
                        );
                        simplified = simplified.or(db, replacement);
                    }
                }

                None => {
                    // If left ∩ right == ∅, then left and right cannot both be true.
                    let no_conflict = left_constraint
                        .negate(db)
                        .or(db, right_constraint.negate(db));
                    domain = domain.and(db, no_conflict.impossible(db));
                }
            }
        }

        // Having done that, we just have to AND the original BDD with its domain. This will map
        // all invalid inputs to false.
        eprintln!("====> simplify");
        eprintln!(" ---> original");
        eprintln!("  {}", Node::Interior(self).display_graph(db, &"  "));
        eprintln!(" ---> simplified");
        eprintln!("  {}", simplified.display_graph(db, &"  "));
        eprintln!(" ---> domain");
        eprintln!("  {}", domain.display_graph(db, &"  "));
        simplified.and(db, domain)
    }

    #[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
    fn minimizations(self, db: &'db dyn Db) -> Box<[Node<'db>]> {
        let constraint = self.constraint(db);
        let if_true = self.if_true(db);
        let if_true_minimizations = if_true.minimizations(db);
        let if_false = self.if_false(db);
        let if_false_minimizations = if_false.minimizations(db);

        // This node's potential minimizations include each of the minimizations of its true and
        // false branches, combined back together into an interior node.
        let mut minimizations = Vec::with_capacity(
            if_true_minimizations.as_slice().len() * if_false_minimizations.as_slice().len(),
        );
        for if_true in if_true_minimizations.as_slice() {
            for if_false in if_false_minimizations.as_slice() {
                minimizations.push(Node::new(db, constraint, *if_true, *if_false));
            }
        }

        // If either of the original outgoing edges are impossible, we can also skip checking this
        // node's variable entirely, and just use the result of the possible edge.
        if matches!(if_true, Node::Impossible) {
            minimizations.extend_from_slice(if_false_minimizations.as_slice());
        } else if matches!(if_false, Node::Impossible) {
            minimizations.extend_from_slice(if_true_minimizations.as_slice());
        }

        minimizations.sort_by_key(|node| node.interior_node_count(db));
        minimizations.into_boxed_slice()
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

    fn implies(self, db: &'db dyn Db, other: Self) -> bool {
        match (self, other) {
            // For two positive constraints, one range has to fully contain the other; the larger
            // constraint implies the smaller.
            //
            //     ....|-----self------|....
            //     ......|---other---|......
            (
                ConstraintAssignment::Positive(self_constraint),
                ConstraintAssignment::Positive(other_constraint),
            ) => self_constraint.implies(db, other_constraint),

            // For two negative constraints, one range has to fully contain the other; the ranges
            // represent "holes", though, so the constraint with the smaller range implies the one
            // with the larger.
            //
            //     |-----|...self....|-----|
            //     |---|.....other.....|---|
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
            ) => self_constraint.intersect(db, other_constraint).is_none(),

            // It's theoretically possible for a negative constraint to imply a positive constraint
            // if the positive constraint is always satisfied (`Never ≤ T ≤ object`). But we never
            // create constraints of that form, so with our representation, a negative constraint
            // can never imply a positive constraint.
            //
            //     |-------self--------|
            //     |---|...other...|---|
            (ConstraintAssignment::Negative(_), ConstraintAssignment::Positive(_)) => false,
        }
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

    /// Simplifies this clause by removing constraints that imply other constraints in the clause.
    /// (Clauses are the intersection of constraints, so if two clauses are redundant, we want to
    /// remove the larger one and keep the smaller one.)
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
                    eprintln!(
                        "===> A {} implies {}, removing {}",
                        self.constraints[j].display(db),
                        self.constraints[i].display(db),
                        self.constraints[i].display(db),
                    );
                    self.constraints.swap_remove(i);
                    changes_made = true;
                    continue 'outer;
                } else if self.constraints[i].implies(db, self.constraints[j]) {
                    // If constraint `j` is removed, then we can continue the inner loop. We will
                    // swap a new element into place at index `j`, and will continue comparing the
                    // constraint at index `i` with later constraints.
                    eprintln!(
                        "===> B {} implies {}, removing {}",
                        self.constraints[i].display(db),
                        self.constraints[j].display(db),
                        self.constraints[j].display(db),
                    );
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
        while self.simplify_one_round(db) {
            // Keep going
        }
    }

    fn simplify_one_round(&mut self, db: &'db dyn Db) -> bool {
        let mut changes_made = false;

        // First simplify each clause individually, by removing constraints that are implied by
        // other constraints in the clause.
        for clause in &mut self.clauses {
            changes_made |= clause.simplify(db);
        }
        if changes_made {
            return true;
        }

        // Then remove any duplicate clauses. (The clause list will start out with no duplicates
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
