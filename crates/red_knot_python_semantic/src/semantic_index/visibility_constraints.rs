//! # Visibility constraints
//!
//! During semantic index building, we collect visibility constraints for each binding and
//! declaration. These constraints are then used during type-checking to determine the static
//! visibility of a certain definition. This allows us to re-analyze control flow during type
//! checking, potentially "hiding" some branches that we can statically determine to never be
//! taken. Consider the following example first. We added implicit "unbound" definitions at the
//! start of the scope. Note how visibility constraints can apply to bindings outside of the
//! if-statement:
//! ```py
//! x = <unbound>  # not a live binding for the use of x below, shadowed by `x = 1`
//! y = <unbound>  # visibility constraint: ~test
//!
//! x = 1  # visibility constraint: ~test
//! if test:
//!     x = 2  # visibility constraint: test
//!
//!     y = 2  # visibility constraint: test
//!
//! use(x)
//! use(y)
//! ```
//! The static truthiness of the `test` condition can either be always-false, ambiguous, or
//! always-true. Similarly, we have the same three options when evaluating a visibility constraint.
//! This outcome determines the visibility of a definition: always-true means that the definition
//! is definitely visible for a given use, always-false means that the definition is definitely
//! not visible, and ambiguous means that we might see this definition or not. In the latter case,
//! we need to consider both options during type inference and boundness analysis. For the example
//! above, these are the possible type inference / boundness results for the uses of `x` and `y`:
//!
//! ```text
//!       | `test` truthiness | `~test` truthiness | type of `x`     | boundness of `y` |
//!       |-------------------|--------------------|-----------------|------------------|
//!       | always false      | always true        | `Literal[1]`    | unbound          |
//!       | ambiguous         | ambiguous          | `Literal[1, 2]` | possibly unbound |
//!       | always true       | always false       | `Literal[2]`    | bound            |
//! ```
//!
//! ### Sequential constraints (ternary AND)
//!
//! As we have seen above, visibility constraints can apply outside of a control flow element.
//! So we need to consider the possibility that multiple constraints apply to the same binding.
//! Here, we consider what happens if multiple `if`-statements lead to a sequence of constraints.
//! Consider the following example:
//! ```py
//! x = 0
//!
//! if test1:
//!     x = 1
//!
//! if test2:
//!     x = 2
//! ```
//! The binding `x = 2` is easy to analyze. Its visibility corresponds to the truthiness of `test2`.
//! For the `x = 1` binding, things are a bit more interesting. It is always visible if `test1` is
//! always-true *and* `test2` is always-false. It is never visible if `test1` is always-false *or*
//! `test2` is always-true. And it is ambiguous otherwise. This corresponds to a ternary *test1 AND
//! ~test2* operation in three-valued Kleene logic [Kleene]:
//!
//! ```text
//!       | AND          | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | always-false | always-false |
//!       | ambiguous    | always-false | ambiguous    | ambiguous    |
//!       | always true  | always-false | ambiguous    | always-true  |
//! ```
//!
//! The `x = 0` binding can be handled similarly, with the difference that both `test1` and `test2`
//! are negated:
//! ```py
//! x = 0  # ~test1 AND ~test2
//!
//! if test1:
//!     x = 1  # test1 AND ~test2
//!
//! if test2:
//!     x = 2  # test2
//! ```
//!
//! ### Merged constraints (ternary OR)
//!
//! Finally, we consider what happens in "parallel" control flow. Consider the following example
//! where we have omitted the test condition for the outer `if` for clarity:
//! ```py
//! x = 0
//!
//! if <…>:
//!     if test1:
//!         x = 1
//! else:
//!     if test2:
//!         x = 2
//!
//! use(x)
//! ```
//! At the usage of `x`, i.e. after control flow has been merged again, the visibility of the `x =
//! 0` binding behaves as follows: the binding is always visible if `test1` is always-false *or*
//! `test2` is always-false; and it is never visible if `test1` is always-true *and* `test2` is
//! always-true. This corresponds to a ternary *OR* operation in Kleene logic:
//!
//! ```text
//!       | OR           | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | ambiguous    | always-true  |
//!       | ambiguous    | ambiguous    | ambiguous    | always-true  |
//!       | always true  | always-true  | always-true  | always-true  |
//! ```
//!
//! Using this, we can annotate the visibility constraints for the example above:
//! ```py
//! x = 0  # ~test1 OR ~test2
//!
//! if <…>:
//!     if test1:
//!         x = 1  # test1
//! else:
//!     if test2:
//!         x = 2  # test2
//!
//! use(x)
//! ```
//!
//! ### Explicit ambiguity
//!
//! In some cases, we explicitly add an “ambiguous” constraint to all bindings
//! in a certain control flow path. We do this when branching on something that we can not (or
//! intentionally do not want to) analyze statically. `for` loops are one example:
//! ```py
//! x = <unbound>
//!
//! for _ in range(2):
//!    x = 1
//! ```
//! Here, we report an ambiguous visibility constraint before branching off. If we don't do this,
//! the `x = <unbound>` binding would be considered unconditionally visible in the no-loop case.
//! And since the other branch does not have the live `x = <unbound>` binding, we would incorrectly
//! create a state where the `x = <unbound>` binding is always visible.
//!
//!
//! ### Representing formulas
//!
//! Given everything above, we can represent a visibility constraint as a _ternary formula_. This
//! is like a boolean formula (which maps several true/false variables to a single true/false
//! result), but which allows the third "ambiguous" value in addition to "true" and "false".
//!
//! [_Binary decision diagrams_][bdd] (BDDs) are a common way to represent boolean formulas when
//! doing program analysis. We extend this to a _ternary decision diagram_ (TDD) to support
//! ambiguous values.
//!
//! A TDD is a graph, and a ternary formula is represented by a node in this graph. There are three
//! possible leaf nodes representing the "true", "false", and "ambiguous" constant functions.
//! Interior nodes consist of a ternary variable to evaluate, and outgoing edges for whether the
//! variable evaluates to true, false, or ambiguous.
//!
//! Our TDDs are _reduced_ and _ordered_ (as is typical for BDDs).
//!
//! An ordered TDD means that variables appear in the same order in all paths within the graph.
//!
//! A reduced TDD means two things: First, we intern the graph nodes, so that we only keep a single
//! copy of interior nodes with the same contents. Second, we eliminate any nodes that are "noops",
//! where the "true" and "false" outgoing edges lead to the same node. (This implies that it
//! doesn't matter what value that variable has when evaluating the formula, and we can leave it
//! out of the evaluation chain completely.)
//!
//! Reduced and ordered decision diagrams are _normal forms_, which means that two equivalent
//! formulas (which have the same outputs for every combination of inputs) are represented by
//! exactly the same graph node. (Because of interning, this is not _equal_ nodes, but _identical_
//! ones.) That means that we can compare formulas for equivalence in constant time, and in
//! particular, can check whether a visibility constraint is statically always true or false,
//! regardless of any Python program state, by seeing if the constraint's formula is the "true" or
//! "false" leaf node.
//!
//! [Kleene]: <https://en.wikipedia.org/wiki/Three-valued_logic#Kleene_and_Priest_logics>
//! [bdd]: https://en.wikipedia.org/wiki/Binary_decision_diagram

use std::cmp::Ordering;

use ruff_index::{Idx, IndexVec};
use rustc_hash::FxHashMap;

use crate::semantic_index::expression::Expression;
use crate::semantic_index::predicate::{
    PatternPredicate, PatternPredicateKind, Predicate, PredicateNode, Predicates, ScopedPredicateId,
};
use crate::types::{infer_expression_type, Truthiness, Type};
use crate::Db;

/// A ternary formula that defines under what conditions a binding is visible. (A ternary formula
/// is just like a boolean formula, but with `Ambiguous` as a third potential result. See the
/// module documentation for more details.)
///
/// The primitive atoms of the formula are [`Predicate`]s, which express some property of the
/// runtime state of the code that we are analyzing.
///
/// We assume that each atom has a stable value each time that the formula is evaluated. An atom
/// that resolves to `Ambiguous` might be true or false, and we can't tell which — but within that
/// evaluation, we assume that the atom has the _same_ unknown value each time it appears. That
/// allows us to perform simplifications like `A ∨ !A → true` and `A ∧ !A → false`.
///
/// That means that when you are constructing a formula, you might need to create distinct atoms
/// for a particular [`Predicate`], if your formula needs to consider how a particular runtime
/// property might be different at different points in the execution of the program.
///
/// Visibility constraints are normalized, so equivalent constraints are guaranteed to have equal
/// IDs.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct ScopedVisibilityConstraintId(u32);

impl std::fmt::Debug for ScopedVisibilityConstraintId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_tuple("ScopedVisibilityConstraintId");
        match *self {
            // We use format_args instead of rendering the strings directly so that we don't get
            // any quotes in the output: ScopedVisibilityConstraintId(AlwaysTrue) instead of
            // ScopedVisibilityConstraintId("AlwaysTrue").
            ALWAYS_TRUE => f.field(&format_args!("AlwaysTrue")),
            AMBIGUOUS => f.field(&format_args!("Ambiguous")),
            ALWAYS_FALSE => f.field(&format_args!("AlwaysFalse")),
            _ => f.field(&self.0),
        };
        f.finish()
    }
}

// Internal details:
//
// There are 3 terminals, with hard-coded constraint IDs: true, ambiguous, and false.
//
// _Atoms_ are the underlying Predicates, which are the variables that are evaluated by the
// ternary function.
//
// _Interior nodes_ provide the TDD structure for the formula. Interior nodes are stored in an
// arena Vec, with the constraint ID providing an index into the arena.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct InteriorNode {
    /// A "variable" that is evaluated as part of a TDD ternary function. For visibility
    /// constraints, this is a `Predicate` that represents some runtime property of the Python
    /// code that we are evaluating.
    atom: ScopedPredicateId,
    if_true: ScopedVisibilityConstraintId,
    if_ambiguous: ScopedVisibilityConstraintId,
    if_false: ScopedVisibilityConstraintId,
}

impl ScopedVisibilityConstraintId {
    /// A special ID that is used for an "always true" / "always visible" constraint.
    pub(crate) const ALWAYS_TRUE: ScopedVisibilityConstraintId =
        ScopedVisibilityConstraintId(0xffff_ffff);

    /// A special ID that is used for an ambiguous constraint.
    pub(crate) const AMBIGUOUS: ScopedVisibilityConstraintId =
        ScopedVisibilityConstraintId(0xffff_fffe);

    /// A special ID that is used for an "always false" / "never visible" constraint.
    pub(crate) const ALWAYS_FALSE: ScopedVisibilityConstraintId =
        ScopedVisibilityConstraintId(0xffff_fffd);

    fn is_terminal(self) -> bool {
        self.0 >= SMALLEST_TERMINAL.0
    }
}

impl Idx for ScopedVisibilityConstraintId {
    #[inline]
    fn new(value: usize) -> Self {
        assert!(value <= (SMALLEST_TERMINAL.0 as usize));
        #[allow(clippy::cast_possible_truncation)]
        Self(value as u32)
    }

    #[inline]
    fn index(self) -> usize {
        debug_assert!(!self.is_terminal());
        self.0 as usize
    }
}

// Rebind some constants locally so that we don't need as many qualifiers below.
const ALWAYS_TRUE: ScopedVisibilityConstraintId = ScopedVisibilityConstraintId::ALWAYS_TRUE;
const AMBIGUOUS: ScopedVisibilityConstraintId = ScopedVisibilityConstraintId::AMBIGUOUS;
const ALWAYS_FALSE: ScopedVisibilityConstraintId = ScopedVisibilityConstraintId::ALWAYS_FALSE;
const SMALLEST_TERMINAL: ScopedVisibilityConstraintId = ALWAYS_FALSE;

/// A collection of visibility constraints for a given scope.
#[derive(Debug, PartialEq, Eq, salsa::Update)]
pub(crate) struct VisibilityConstraints {
    interiors: IndexVec<ScopedVisibilityConstraintId, InteriorNode>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct VisibilityConstraintsBuilder {
    interiors: IndexVec<ScopedVisibilityConstraintId, InteriorNode>,
    interior_cache: FxHashMap<InteriorNode, ScopedVisibilityConstraintId>,
    not_cache: FxHashMap<ScopedVisibilityConstraintId, ScopedVisibilityConstraintId>,
    and_cache: FxHashMap<
        (ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
        ScopedVisibilityConstraintId,
    >,
    or_cache: FxHashMap<
        (ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
        ScopedVisibilityConstraintId,
    >,
}

impl VisibilityConstraintsBuilder {
    pub(crate) fn build(self) -> VisibilityConstraints {
        VisibilityConstraints {
            interiors: self.interiors,
        }
    }

    /// Returns whether `a` or `b` has a "larger" atom. TDDs are ordered such that interior nodes
    /// can only have edges to "larger" nodes. Terminals are considered to have a larger atom than
    /// any internal node, since they are leaf nodes.
    fn cmp_atoms(
        &self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> Ordering {
        if a == b || (a.is_terminal() && b.is_terminal()) {
            Ordering::Equal
        } else if a.is_terminal() {
            Ordering::Greater
        } else if b.is_terminal() {
            Ordering::Less
        } else {
            self.interiors[a].atom.cmp(&self.interiors[b].atom)
        }
    }

    /// Adds an interior node, ensuring that we always use the same visibility constraint ID for
    /// equal nodes.
    fn add_interior(&mut self, node: InteriorNode) -> ScopedVisibilityConstraintId {
        // If the true and false branches lead to the same node, we can override the ambiguous
        // branch to go there too. And this node is then redundant and can be reduced.
        if node.if_true == node.if_false {
            return node.if_true;
        }

        *self
            .interior_cache
            .entry(node)
            .or_insert_with(|| self.interiors.push(node))
    }

    /// Adds a new visibility constraint that checks a single [`Predicate`].
    ///
    /// [`ScopedPredicateId`]s are the “variables” that are evaluated by a TDD. A TDD variable has
    /// the same value no matter how many times it appears in the ternary formula that the TDD
    /// represents.
    ///
    /// However, we sometimes have to model how a `Predicate` can have a different runtime
    /// value at different points in the execution of the program. To handle this, you can take
    /// advantage of the fact that the [`Predicates`] arena does not deduplicate `Predicate`s.
    /// You can add a `Predicate` multiple times, yielding different `ScopedPredicateId`s, which
    /// you can then create separate TDD atoms for.
    pub(crate) fn add_atom(
        &mut self,
        predicate: ScopedPredicateId,
    ) -> ScopedVisibilityConstraintId {
        self.add_interior(InteriorNode {
            atom: predicate,
            if_true: ALWAYS_TRUE,
            if_ambiguous: AMBIGUOUS,
            if_false: ALWAYS_FALSE,
        })
    }

    /// Adds a new visibility constraint that is the ternary NOT of an existing one.
    pub(crate) fn add_not_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        if a == ALWAYS_TRUE {
            return ALWAYS_FALSE;
        } else if a == AMBIGUOUS {
            return AMBIGUOUS;
        } else if a == ALWAYS_FALSE {
            return ALWAYS_TRUE;
        }

        if let Some(cached) = self.not_cache.get(&a) {
            return *cached;
        }
        let a_node = self.interiors[a];
        let if_true = self.add_not_constraint(a_node.if_true);
        let if_ambiguous = self.add_not_constraint(a_node.if_ambiguous);
        let if_false = self.add_not_constraint(a_node.if_false);
        let result = self.add_interior(InteriorNode {
            atom: a_node.atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.not_cache.insert(a, result);
        result
    }

    /// Adds a new visibility constraint that is the ternary OR of two existing ones.
    pub(crate) fn add_or_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        match (a, b) {
            (ALWAYS_TRUE, _) | (_, ALWAYS_TRUE) => return ALWAYS_TRUE,
            (ALWAYS_FALSE, other) | (other, ALWAYS_FALSE) => return other,
            (AMBIGUOUS, AMBIGUOUS) => return AMBIGUOUS,
            _ => {}
        }

        // OR is commutative, which lets us halve the cache requirements
        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.or_cache.get(&(a, b)) {
            return *cached;
        }

        let (atom, if_true, if_ambiguous, if_false) = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interiors[a];
                let b_node = self.interiors[b];
                let if_true = self.add_or_constraint(a_node.if_true, b_node.if_true);
                let if_false = self.add_or_constraint(a_node.if_false, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a_node.if_ambiguous, b_node.if_ambiguous)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Less => {
                let a_node = self.interiors[a];
                let if_true = self.add_or_constraint(a_node.if_true, b);
                let if_false = self.add_or_constraint(a_node.if_false, b);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a_node.if_ambiguous, b)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Greater => {
                let b_node = self.interiors[b];
                let if_true = self.add_or_constraint(a, b_node.if_true);
                let if_false = self.add_or_constraint(a, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a, b_node.if_ambiguous)
                };
                (b_node.atom, if_true, if_ambiguous, if_false)
            }
        };

        let result = self.add_interior(InteriorNode {
            atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.or_cache.insert((a, b), result);
        result
    }

    /// Adds a new visibility constraint that is the ternary AND of two existing ones.
    pub(crate) fn add_and_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        match (a, b) {
            (ALWAYS_FALSE, _) | (_, ALWAYS_FALSE) => return ALWAYS_FALSE,
            (ALWAYS_TRUE, other) | (other, ALWAYS_TRUE) => return other,
            (AMBIGUOUS, AMBIGUOUS) => return AMBIGUOUS,
            _ => {}
        }

        // AND is commutative, which lets us halve the cache requirements
        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.and_cache.get(&(a, b)) {
            return *cached;
        }

        let (atom, if_true, if_ambiguous, if_false) = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interiors[a];
                let b_node = self.interiors[b];
                let if_true = self.add_and_constraint(a_node.if_true, b_node.if_true);
                let if_false = self.add_and_constraint(a_node.if_false, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a_node.if_ambiguous, b_node.if_ambiguous)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Less => {
                let a_node = self.interiors[a];
                let if_true = self.add_and_constraint(a_node.if_true, b);
                let if_false = self.add_and_constraint(a_node.if_false, b);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a_node.if_ambiguous, b)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Greater => {
                let b_node = self.interiors[b];
                let if_true = self.add_and_constraint(a, b_node.if_true);
                let if_false = self.add_and_constraint(a, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a, b_node.if_ambiguous)
                };
                (b_node.atom, if_true, if_ambiguous, if_false)
            }
        };

        let result = self.add_interior(InteriorNode {
            atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.and_cache.insert((a, b), result);
        result
    }
}

impl VisibilityConstraints {
    /// Analyze the statically known visibility for a given visibility constraint.
    pub(crate) fn evaluate<'db>(
        &self,
        db: &'db dyn Db,
        predicates: &Predicates<'db>,
        mut id: ScopedVisibilityConstraintId,
    ) -> Truthiness {
        loop {
            let node = match id {
                ALWAYS_TRUE => return Truthiness::AlwaysTrue,
                AMBIGUOUS => return Truthiness::Ambiguous,
                ALWAYS_FALSE => return Truthiness::AlwaysFalse,
                _ => self.interiors[id],
            };
            let predicate = &predicates[node.atom];
            match Self::analyze_single(db, predicate) {
                Truthiness::AlwaysTrue => id = node.if_true,
                Truthiness::Ambiguous => id = node.if_ambiguous,
                Truthiness::AlwaysFalse => id = node.if_false,
            }
        }
    }

    fn analyze_single_pattern_predicate_kind<'db>(
        db: &'db dyn Db,
        predicate_kind: &PatternPredicateKind<'db>,
        subject: Expression<'db>,
    ) -> Truthiness {
        match predicate_kind {
            PatternPredicateKind::Value(value) => {
                let subject_ty = infer_expression_type(db, subject);
                let value_ty = infer_expression_type(db, *value);

                if subject_ty.is_single_valued(db) {
                    Truthiness::from(subject_ty.is_equivalent_to(db, value_ty))
                } else {
                    Truthiness::Ambiguous
                }
            }
            PatternPredicateKind::Singleton(singleton) => {
                let subject_ty = infer_expression_type(db, subject);

                let singleton_ty = match singleton {
                    ruff_python_ast::Singleton::None => Type::none(db),
                    ruff_python_ast::Singleton::True => Type::BooleanLiteral(true),
                    ruff_python_ast::Singleton::False => Type::BooleanLiteral(false),
                };

                debug_assert!(singleton_ty.is_singleton(db));

                if subject_ty.is_equivalent_to(db, singleton_ty) {
                    Truthiness::AlwaysTrue
                } else if subject_ty.is_disjoint_from(db, singleton_ty) {
                    Truthiness::AlwaysFalse
                } else {
                    Truthiness::Ambiguous
                }
            }
            PatternPredicateKind::Or(predicates) => {
                use std::ops::ControlFlow;
                let (ControlFlow::Break(truthiness) | ControlFlow::Continue(truthiness)) =
                    predicates
                        .iter()
                        .map(|p| Self::analyze_single_pattern_predicate_kind(db, p, subject))
                        // this is just a "max", but with a slight optimization: `AlwaysTrue` is the "greatest" possible element, so we short-circuit if we get there
                        .try_fold(Truthiness::AlwaysFalse, |acc, next| match (acc, next) {
                            (Truthiness::AlwaysTrue, _) | (_, Truthiness::AlwaysTrue) => {
                                ControlFlow::Break(Truthiness::AlwaysTrue)
                            }
                            (Truthiness::Ambiguous, _) | (_, Truthiness::Ambiguous) => {
                                ControlFlow::Continue(Truthiness::Ambiguous)
                            }
                            (Truthiness::AlwaysFalse, Truthiness::AlwaysFalse) => {
                                ControlFlow::Continue(Truthiness::AlwaysFalse)
                            }
                        });
                truthiness
            }
            PatternPredicateKind::Class(class_expr) => {
                let subject_ty = infer_expression_type(db, subject);
                let class_ty = infer_expression_type(db, *class_expr).to_instance(db);

                class_ty.map_or(Truthiness::Ambiguous, |class_ty| {
                    if subject_ty.is_subtype_of(db, class_ty) {
                        Truthiness::AlwaysTrue
                    } else if subject_ty.is_disjoint_from(db, class_ty) {
                        Truthiness::AlwaysFalse
                    } else {
                        Truthiness::Ambiguous
                    }
                })
            }
            PatternPredicateKind::Unsupported => Truthiness::Ambiguous,
        }
    }

    fn analyze_single_pattern_predicate(db: &dyn Db, predicate: PatternPredicate) -> Truthiness {
        let truthiness = Self::analyze_single_pattern_predicate_kind(
            db,
            predicate.kind(db),
            predicate.subject(db),
        );

        if truthiness == Truthiness::AlwaysTrue && predicate.guard(db).is_some() {
            // Fall back to ambiguous, the guard might change the result.
            // TODO: actually analyze guard truthiness
            Truthiness::Ambiguous
        } else {
            truthiness
        }
    }

    fn analyze_single(db: &dyn Db, predicate: &Predicate) -> Truthiness {
        match predicate.node {
            PredicateNode::Expression(test_expr) => {
                let ty = infer_expression_type(db, test_expr);
                ty.bool(db).negate_if(!predicate.is_positive)
            }
            PredicateNode::Pattern(inner) => Self::analyze_single_pattern_predicate(db, inner),
        }
    }
}
