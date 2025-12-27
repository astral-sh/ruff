//! # Reachability constraints
//!
//! During semantic index building, we record so-called reachability constraints that keep track
//! of a set of conditions that need to apply in order for a certain statement or expression to
//! be reachable from the start of the scope. As an example, consider the following situation where
//! we have just processed an `if`-statement:
//! ```py
//! if test:
//!     <is this reachable?>
//! ```
//! In this case, we would record a reachability constraint of `test`, which would later allow us
//! to re-analyze the control flow during type-checking, once we actually know the static truthiness
//! of `test`. When evaluating a constraint, there are three possible outcomes: always true, always
//! false, or ambiguous. For a simple constraint like this, always-true and always-false correspond
//! to the case in which we can infer that the type of `test` is `Literal[True]` or `Literal[False]`.
//! In any other case, like if the type of `test` is `bool` or `Unknown`, we cannot statically
//! determine whether `test` is truthy or falsy, so the outcome would be "ambiguous".
//!
//!
//! ## Sequential constraints (ternary AND)
//!
//! Whenever control flow branches, we record reachability constraints. If we already have a
//! constraint, we create a new one using a ternary AND operation. Consider the following example:
//! ```py
//! if test1:
//!     if test2:
//!         <is this reachable?>
//! ```
//! Here, we would accumulate a reachability constraint of `test1 AND test2`. We can statically
//! determine that this position is *always* reachable only if both `test1` and `test2` are
//! always true. On the other hand, we can statically determine that this position is *never*
//! reachable if *either* `test1` or `test2` is always false. In any other case, we cannot
//! determine whether this position is reachable or not, so the outcome is "ambiguous". This
//! corresponds to a ternary *AND* operation in [Kleene] logic:
//!
//! ```text
//!       | AND          | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | always-false | always-false |
//!       | ambiguous    | always-false | ambiguous    | ambiguous    |
//!       | always true  | always-false | ambiguous    | always-true  |
//! ```
//!
//!
//! ## Merged constraints (ternary OR)
//!
//! We also need to consider the case where control flow merges again. Consider a case like this:
//! ```py
//! def _():
//!     if test1:
//!         pass
//!     elif test2:
//!         pass
//!     else:
//!         return
//!
//!     <is this reachable?>
//! ```
//! Here, the first branch has a `test1` constraint, and the second branch has a `test2` constraint.
//! The third branch ends in a terminal statement [^1]. When we merge control flow, we need to consider
//! the reachability through either the first or the second branch. The current position is only
//! *definitely* unreachable if both `test1` and `test2` are always false. It is definitely
//! reachable if *either* `test1` or `test2` is always true. In any other case, we cannot statically
//! determine whether it is reachable or not. This operation corresponds to a ternary *OR* operation:
//!
//! ```text
//!       | OR           | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | ambiguous    | always-true  |
//!       | ambiguous    | ambiguous    | ambiguous    | always-true  |
//!       | always true  | always-true  | always-true  | always-true  |
//! ```
//!
//! [^1]: What's actually happening here is that we merge all three branches using a ternary OR. The
//! third branch has a reachability constraint of `always-false`, and `t OR always-false` is equal
//! to `t` (see first column in that table), so it was okay to omit the third branch in the discussion
//! above.
//!
//!
//! ## Negation
//!
//! Control flow elements like `if-elif-else` or `match` statements can also lead to negated
//! constraints. For example, we record a constraint of `~test` for the `else` branch here:
//! ```py
//! if test:
//!     pass
//! else:
//!    <is this reachable?>
//! ```
//!
//! ## Explicit ambiguity
//!
//! In some cases, we explicitly record an “ambiguous” constraint. We do this when branching on
//! something that we cannot (or intentionally do not want to) analyze statically. `for` loops are
//! one example:
//! ```py
//! def _():
//!     for _ in range(2):
//!        return
//!
//!     <is this reachable?>
//! ```
//! If we would not record any constraints at the branching point, we would have an `always-true`
//! reachability for the no-loop branch, and a `always-true` reachability for the branch which enters
//! the loop. Merging those would lead to a reachability of `always-true OR always-true = always-true`,
//! i.e. we would consider the end of the scope to be unconditionally reachable, which is not correct.
//!
//! Recording an ambiguous constraint at the branching point modifies the constraints in both branches to
//! `always-true AND ambiguous = ambiguous`. Merging these two using OR correctly leads to `ambiguous` for
//! the end-of-scope reachability.
//!
//!
//! ## Reachability constraints and bindings
//!
//! To understand how reachability constraints apply to bindings in particular, consider the following
//! example:
//! ```py
//! x = <unbound>  # not a live binding for the use of x below, shadowed by `x = 1`
//! y = <unbound>  # reachability constraint: ~test
//!
//! x = 1  # reachability constraint: ~test
//! if test:
//!     x = 2  # reachability constraint: test
//!
//!     y = 2  # reachability constraint: test
//!
//! use(x)
//! use(y)
//! ```
//! Both the type and the boundness of `x` and `y` are affected by reachability constraints:
//!
//! ```text
//!       | `test` truthiness | type of `x`     | boundness of `y` |
//!       |-------------------|-----------------|------------------|
//!       | always false      | `Literal[1]`    | unbound          |
//!       | ambiguous         | `Literal[1, 2]` | possibly unbound |
//!       | always true       | `Literal[2]`    | bound            |
//! ```
//!
//! To achieve this, we apply reachability constraints retroactively to bindings that came before
//! the branching point. In the example above, the `x = 1` binding has a `test` constraint in the
//! `if` branch, and a `~test` constraint in the implicit `else` branch. Since it is shadowed by
//! `x = 2` in the `if` branch, we are only left with the `~test` constraint after control flow
//! has merged again.
//!
//! For live bindings, the reachability constraint therefore refers to the following question:
//! Is the binding reachable from the start of the scope, and is there a control flow path from
//! that binding to a use of that symbol at the current position?
//!
//! In the example above, `x = 1` is always reachable, but that binding can only reach the use of
//! `x` at the current position if `test` is falsy.
//!
//! To handle boundness correctly, we also add implicit `y = <unbound>` bindings at the start of
//! the scope. This allows us to determine whether a symbol is definitely bound (if that implicit
//! `y = <unbound>` binding is not visible), possibly unbound (if the reachability constraint
//! evaluates to `Ambiguous`), or definitely unbound (in case the `y = <unbound>` binding is
//! always visible).
//!
//!
//! ### Representing formulas
//!
//! Given everything above, we can represent a reachability constraint as a _ternary formula_. This
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
//! particular, can check whether a reachability constraint is statically always true or false,
//! regardless of any Python program state, by seeing if the constraint's formula is the "true" or
//! "false" leaf node.
//!
//! [Kleene]: <https://en.wikipedia.org/wiki/Three-valued_logic#Kleene_and_Priest_logics>
//! [bdd]: https://en.wikipedia.org/wiki/Binary_decision_diagram

use std::cmp::Ordering;

use ruff_index::{Idx, IndexVec};
use rustc_hash::FxHashMap;

use crate::Db;
use crate::dunder_all::dunder_all_names;
use crate::place::{RequiresExplicitReExport, imported_symbol};
use crate::rank::RankBitBox;
use crate::semantic_index::place_table;
use crate::semantic_index::predicate::{
    CallableAndCallExpr, PatternPredicate, PatternPredicateKind, Predicate, PredicateNode,
    Predicates, ScopedPredicateId,
};
use crate::types::{
    CallableTypes, IntersectionBuilder, Truthiness, Type, TypeContext, UnionBuilder, UnionType,
    infer_expression_type, static_expression_truthiness,
};

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
/// reachability constraints are normalized, so equivalent constraints are guaranteed to have equal
/// IDs.
#[derive(Clone, Copy, Eq, Hash, PartialEq, get_size2::GetSize)]
pub(crate) struct ScopedReachabilityConstraintId(u32);

impl std::fmt::Debug for ScopedReachabilityConstraintId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_tuple("ScopedReachabilityConstraintId");
        match *self {
            // We use format_args instead of rendering the strings directly so that we don't get
            // any quotes in the output: ScopedReachabilityConstraintId(AlwaysTrue) instead of
            // ScopedReachabilityConstraintId("AlwaysTrue").
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
struct InteriorNode {
    /// A "variable" that is evaluated as part of a TDD ternary function. For reachability
    /// constraints, this is a `Predicate` that represents some runtime property of the Python
    /// code that we are evaluating.
    atom: ScopedPredicateId,
    if_true: ScopedReachabilityConstraintId,
    if_ambiguous: ScopedReachabilityConstraintId,
    if_false: ScopedReachabilityConstraintId,
}

impl ScopedReachabilityConstraintId {
    /// A special ID that is used for an "always true" / "always visible" constraint.
    pub(crate) const ALWAYS_TRUE: ScopedReachabilityConstraintId =
        ScopedReachabilityConstraintId(0xffff_ffff);

    /// A special ID that is used for an ambiguous constraint.
    pub(crate) const AMBIGUOUS: ScopedReachabilityConstraintId =
        ScopedReachabilityConstraintId(0xffff_fffe);

    /// A special ID that is used for an "always false" / "never visible" constraint.
    pub(crate) const ALWAYS_FALSE: ScopedReachabilityConstraintId =
        ScopedReachabilityConstraintId(0xffff_fffd);

    fn is_terminal(self) -> bool {
        self.0 >= SMALLEST_TERMINAL.0
    }

    fn as_u32(self) -> u32 {
        self.0
    }
}

impl Idx for ScopedReachabilityConstraintId {
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

// Rebind some constants locally so that we don't need as many qualifiers below.
const ALWAYS_TRUE: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::ALWAYS_TRUE;
const AMBIGUOUS: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::AMBIGUOUS;
const ALWAYS_FALSE: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::ALWAYS_FALSE;
const SMALLEST_TERMINAL: ScopedReachabilityConstraintId = ALWAYS_FALSE;

fn singleton_to_type(db: &dyn Db, singleton: ruff_python_ast::Singleton) -> Type<'_> {
    let ty = match singleton {
        ruff_python_ast::Singleton::None => Type::none(db),
        ruff_python_ast::Singleton::True => Type::BooleanLiteral(true),
        ruff_python_ast::Singleton::False => Type::BooleanLiteral(false),
    };
    debug_assert!(ty.is_singleton(db));
    ty
}

/// Turn a `match` pattern kind into a type that represents the set of all values that would definitely
/// match that pattern.
fn pattern_kind_to_type<'db>(db: &'db dyn Db, kind: &PatternPredicateKind<'db>) -> Type<'db> {
    match kind {
        PatternPredicateKind::Singleton(singleton) => singleton_to_type(db, *singleton),
        PatternPredicateKind::Value(value) => {
            infer_expression_type(db, *value, TypeContext::default())
        }
        PatternPredicateKind::Class(class_expr, kind) => {
            if kind.is_irrefutable() {
                infer_expression_type(db, *class_expr, TypeContext::default())
                    .to_instance(db)
                    .unwrap_or(Type::Never)
                    .top_materialization(db)
            } else {
                Type::Never
            }
        }
        PatternPredicateKind::Or(predicates) => {
            UnionType::from_elements(db, predicates.iter().map(|p| pattern_kind_to_type(db, p)))
        }
        PatternPredicateKind::As(pattern, _) => pattern
            .as_deref()
            .map(|p| pattern_kind_to_type(db, p))
            .unwrap_or_else(Type::object),
        PatternPredicateKind::Unsupported => Type::Never,
    }
}

/// Go through the list of previous match cases, and accumulate a union of all types that were already
/// matched by these patterns.
fn type_excluded_by_previous_patterns<'db>(
    db: &'db dyn Db,
    mut predicate: PatternPredicate<'db>,
) -> Type<'db> {
    let mut builder = UnionBuilder::new(db);
    while let Some(previous) = predicate.previous_predicate(db) {
        predicate = *previous;

        if predicate.guard(db).is_none() {
            builder = builder.add(pattern_kind_to_type(db, predicate.kind(db)));
        }
    }
    builder.build()
}

/// Analyze a pattern predicate to determine its static truthiness.
///
/// This is a Salsa tracked function to enable memoization. Without memoization, for a match
/// statement with N cases where each case references the subject (e.g., `self`), we would
/// re-analyze each pattern O(N) times (once per reference), leading to O(N²) total work.
/// With memoization, each pattern is analyzed exactly once.
#[salsa::tracked(cycle_initial = analyze_pattern_predicate_cycle_initial, heap_size = get_size2::GetSize::get_heap_size)]
fn analyze_pattern_predicate<'db>(db: &'db dyn Db, predicate: PatternPredicate<'db>) -> Truthiness {
    let subject_ty = infer_expression_type(db, predicate.subject(db), TypeContext::default());

    let narrowed_subject = IntersectionBuilder::new(db)
        .add_positive(subject_ty)
        .add_negative(type_excluded_by_previous_patterns(db, predicate));

    let narrowed_subject_ty = narrowed_subject.clone().build();

    // Consider a case where we match on a subject type of `Self` with an upper bound of `Answer`,
    // where `Answer` is a {YES, NO} enum. After a previous pattern matching on `NO`, the narrowed
    // subject type is `Self & ~Literal[NO]`. This type is *not* equivalent to `Literal[YES]`,
    // because `Self` could also specialize to `Literal[NO]` or `Never`, making the intersection
    // empty. However, if the current pattern matches on `YES`, the *next* narrowed subject type
    // will be `Self & ~Literal[NO] & ~Literal[YES]`, which *is* always equivalent to `Never`. This
    // means that subsequent patterns can never match. And we know that if we reach this point,
    // the current pattern will have to match. We return `AlwaysTrue` here, since the call to
    // `analyze_single_pattern_predicate_kind` below would return `Ambiguous` in this case.
    let next_narrowed_subject_ty = narrowed_subject
        .add_negative(pattern_kind_to_type(db, predicate.kind(db)))
        .build();
    if !narrowed_subject_ty.is_never() && next_narrowed_subject_ty.is_never() {
        return Truthiness::AlwaysTrue;
    }

    let truthiness = ReachabilityConstraints::analyze_single_pattern_predicate_kind(
        db,
        predicate.kind(db),
        narrowed_subject_ty,
    );

    if truthiness == Truthiness::AlwaysTrue && predicate.guard(db).is_some() {
        // Fall back to ambiguous, the guard might change the result.
        // TODO: actually analyze guard truthiness
        Truthiness::Ambiguous
    } else {
        truthiness
    }
}

fn analyze_pattern_predicate_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _predicate: PatternPredicate<'db>,
) -> Truthiness {
    Truthiness::Ambiguous
}

/// A collection of reachability constraints for a given scope.
#[derive(Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct ReachabilityConstraints {
    /// The interior TDD nodes that were marked as used when being built.
    used_interiors: Box<[InteriorNode]>,
    /// A bit vector indicating which interior TDD nodes were marked as used. This is indexed by
    /// the node's [`ScopedReachabilityConstraintId`]. The rank of the corresponding bit gives the
    /// index of that node in the `used_interiors` vector.
    used_indices: RankBitBox,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ReachabilityConstraintsBuilder {
    interiors: IndexVec<ScopedReachabilityConstraintId, InteriorNode>,
    interior_used: IndexVec<ScopedReachabilityConstraintId, bool>,
    interior_cache: FxHashMap<InteriorNode, ScopedReachabilityConstraintId>,
    not_cache: FxHashMap<ScopedReachabilityConstraintId, ScopedReachabilityConstraintId>,
    and_cache: FxHashMap<
        (
            ScopedReachabilityConstraintId,
            ScopedReachabilityConstraintId,
        ),
        ScopedReachabilityConstraintId,
    >,
    or_cache: FxHashMap<
        (
            ScopedReachabilityConstraintId,
            ScopedReachabilityConstraintId,
        ),
        ScopedReachabilityConstraintId,
    >,
}

impl ReachabilityConstraintsBuilder {
    pub(crate) fn build(self) -> ReachabilityConstraints {
        let used_indices = RankBitBox::from_bits(self.interior_used.iter().copied());
        let used_interiors = (self.interiors.into_iter())
            .zip(self.interior_used)
            .filter_map(|(interior, used)| used.then_some(interior))
            .collect();
        ReachabilityConstraints {
            used_interiors,
            used_indices,
        }
    }

    /// Marks that a particular TDD node is used. This lets us throw away interior nodes that were
    /// only calculated for intermediate values, and which don't need to be included in the final
    /// built result.
    pub(crate) fn mark_used(&mut self, node: ScopedReachabilityConstraintId) {
        if !node.is_terminal() && !self.interior_used[node] {
            self.interior_used[node] = true;
            let node = self.interiors[node];
            self.mark_used(node.if_true);
            self.mark_used(node.if_ambiguous);
            self.mark_used(node.if_false);
        }
    }

    /// Implements the ordering that determines which level a TDD node appears at.
    ///
    /// Each interior node checks the value of a single variable (for us, a `Predicate`).
    /// TDDs are ordered such that every path from the root of the graph to the leaves must
    /// check each variable at most once, and must check each variable in the same order.
    ///
    /// We can choose any ordering that we want, as long as it's consistent — with the
    /// caveat that terminal nodes must always be last in the ordering, since they are the
    /// leaf nodes of the graph.
    ///
    /// We currently compare interior nodes by looking at the Salsa IDs of each variable's
    /// `Predicate`, since this is already available and easy to compare. We also _reverse_
    /// the comparison of those Salsa IDs. The Salsa IDs are assigned roughly sequentially
    /// while traversing the source code. Reversing the comparison means `Predicate`s that
    /// appear later in the source will tend to be placed "higher" (closer to the root) in
    /// the TDD graph. We have found empirically that this leads to smaller TDD graphs [1],
    /// since there are often repeated combinations of `Predicate`s from earlier in the
    /// file.
    ///
    /// [1]: https://github.com/astral-sh/ruff/pull/20098
    fn cmp_atoms(
        &self,
        a: ScopedReachabilityConstraintId,
        b: ScopedReachabilityConstraintId,
    ) -> Ordering {
        if a == b || (a.is_terminal() && b.is_terminal()) {
            Ordering::Equal
        } else if a.is_terminal() {
            Ordering::Greater
        } else if b.is_terminal() {
            Ordering::Less
        } else {
            // See https://github.com/astral-sh/ruff/pull/20098 for an explanation of why this
            // ordering is reversed.
            self.interiors[a]
                .atom
                .cmp(&self.interiors[b].atom)
                .reverse()
        }
    }

    /// Adds an interior node, ensuring that we always use the same reachability constraint ID for
    /// equal nodes.
    fn add_interior(&mut self, node: InteriorNode) -> ScopedReachabilityConstraintId {
        // If the true and false branches lead to the same node, we can override the ambiguous
        // branch to go there too. And this node is then redundant and can be reduced.
        if node.if_true == node.if_false {
            return node.if_true;
        }

        *self.interior_cache.entry(node).or_insert_with(|| {
            self.interior_used.push(false);
            self.interiors.push(node)
        })
    }

    /// Adds a new reachability constraint that checks a single [`Predicate`].
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
    ) -> ScopedReachabilityConstraintId {
        if predicate == ScopedPredicateId::ALWAYS_FALSE {
            ScopedReachabilityConstraintId::ALWAYS_FALSE
        } else if predicate == ScopedPredicateId::ALWAYS_TRUE {
            ScopedReachabilityConstraintId::ALWAYS_TRUE
        } else {
            self.add_interior(InteriorNode {
                atom: predicate,
                if_true: ALWAYS_TRUE,
                if_ambiguous: AMBIGUOUS,
                if_false: ALWAYS_FALSE,
            })
        }
    }

    /// Adds a new reachability constraint that is the ternary NOT of an existing one.
    pub(crate) fn add_not_constraint(
        &mut self,
        a: ScopedReachabilityConstraintId,
    ) -> ScopedReachabilityConstraintId {
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

    /// Adds a new reachability constraint that is the ternary OR of two existing ones.
    pub(crate) fn add_or_constraint(
        &mut self,
        a: ScopedReachabilityConstraintId,
        b: ScopedReachabilityConstraintId,
    ) -> ScopedReachabilityConstraintId {
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

    /// Adds a new reachability constraint that is the ternary AND of two existing ones.
    pub(crate) fn add_and_constraint(
        &mut self,
        a: ScopedReachabilityConstraintId,
        b: ScopedReachabilityConstraintId,
    ) -> ScopedReachabilityConstraintId {
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

impl ReachabilityConstraints {
    /// Analyze the statically known reachability for a given constraint.
    pub(crate) fn evaluate<'db>(
        &self,
        db: &'db dyn Db,
        predicates: &Predicates<'db>,
        mut id: ScopedReachabilityConstraintId,
    ) -> Truthiness {
        loop {
            let node = match id {
                ALWAYS_TRUE => return Truthiness::AlwaysTrue,
                AMBIGUOUS => return Truthiness::Ambiguous,
                ALWAYS_FALSE => return Truthiness::AlwaysFalse,
                _ => {
                    // `id` gives us the index of this node in the IndexVec that we used when
                    // constructing this BDD. When finalizing the builder, we threw away any
                    // interior nodes that weren't marked as used. The `used_indices` bit vector
                    // lets us verify that this node was marked as used, and the rank of that bit
                    // in the bit vector tells us where this node lives in the "condensed"
                    // `used_interiors` vector.
                    let raw_index = id.as_u32() as usize;
                    debug_assert!(
                        self.used_indices.get_bit(raw_index).unwrap_or(false),
                        "all used reachability constraints should have been marked as used",
                    );
                    let index = self.used_indices.rank(raw_index) as usize;
                    self.used_interiors[index]
                }
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
        subject_ty: Type<'db>,
    ) -> Truthiness {
        match predicate_kind {
            PatternPredicateKind::Value(value) => {
                let value_ty = infer_expression_type(db, *value, TypeContext::default());

                if subject_ty.is_single_valued(db) {
                    Truthiness::from(subject_ty.is_equivalent_to(db, value_ty))
                } else {
                    Truthiness::Ambiguous
                }
            }
            PatternPredicateKind::Singleton(singleton) => {
                let singleton_ty = singleton_to_type(db, *singleton);

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

                let mut excluded_types = vec![];
                let (ControlFlow::Break(truthiness) | ControlFlow::Continue(truthiness)) =
                    predicates
                        .iter()
                        .map(|p| {
                            let narrowed_subject_ty = IntersectionBuilder::new(db)
                                .add_positive(subject_ty)
                                .add_negative(UnionType::from_elements(db, excluded_types.iter()))
                                .build();

                            excluded_types.push(pattern_kind_to_type(db, p));

                            Self::analyze_single_pattern_predicate_kind(db, p, narrowed_subject_ty)
                        })
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
            PatternPredicateKind::Class(class_expr, kind) => {
                let class_ty = infer_expression_type(db, *class_expr, TypeContext::default())
                    .as_class_literal()
                    .map(|class| Type::instance(db, class.top_materialization(db)));

                class_ty.map_or(Truthiness::Ambiguous, |class_ty| {
                    if subject_ty.is_subtype_of(db, class_ty) {
                        if kind.is_irrefutable() {
                            Truthiness::AlwaysTrue
                        } else {
                            // A class pattern like `case Point(x=0, y=0)` is not irrefutable,
                            // i.e. it does not match all instances of `Point`. This means that
                            // we can't tell for sure if this pattern will match or not.
                            Truthiness::Ambiguous
                        }
                    } else if subject_ty.is_disjoint_from(db, class_ty) {
                        Truthiness::AlwaysFalse
                    } else {
                        Truthiness::Ambiguous
                    }
                })
            }
            PatternPredicateKind::As(pattern, _) => pattern
                .as_deref()
                .map(|p| Self::analyze_single_pattern_predicate_kind(db, p, subject_ty))
                .unwrap_or(Truthiness::AlwaysTrue),
            PatternPredicateKind::Unsupported => Truthiness::Ambiguous,
        }
    }

    fn analyze_single(db: &dyn Db, predicate: &Predicate) -> Truthiness {
        let _span = tracing::trace_span!("analyze_single", ?predicate).entered();

        match predicate.node {
            PredicateNode::Expression(test_expr) => {
                static_expression_truthiness(db, test_expr).negate_if(!predicate.is_positive)
            }
            PredicateNode::ReturnsNever(CallableAndCallExpr {
                callable,
                call_expr,
            }) => {
                // We first infer just the type of the callable. In the most likely case that the
                // function is not marked with `NoReturn`, or that it always returns `NoReturn`,
                // doing so allows us to avoid the more expensive work of inferring the entire call
                // expression (which could involve inferring argument types to possibly run the overload
                // selection algorithm).
                // Avoiding this on the happy-path is important because these constraints can be
                // very large in number, since we add them on all statement level function calls.
                let ty = infer_expression_type(db, callable, TypeContext::default());

                // Short-circuit for well known types that are known not to return `Never` when called.
                // Without the short-circuit, we've seen that threads keep blocking each other
                // because they all try to acquire Salsa's `CallableType` lock that ensures each type
                // is only interned once. The lock is so heavily congested because there are only
                // very few dynamic types, in which case Salsa's sharding the locks by value
                // doesn't help much.
                // See <https://github.com/astral-sh/ty/issues/968>.
                if matches!(ty, Type::Dynamic(_)) {
                    return Truthiness::AlwaysFalse.negate_if(!predicate.is_positive);
                }

                let overloads_iterator = if let Some(callable) = ty
                    .try_upcast_to_callable(db)
                    .and_then(CallableTypes::exactly_one)
                {
                    callable.signatures(db).overloads.iter()
                } else {
                    return Truthiness::AlwaysFalse.negate_if(!predicate.is_positive);
                };

                let (no_overloads_return_never, all_overloads_return_never) = overloads_iterator
                    .fold((true, true), |(none, all), overload| {
                        let overload_returns_never =
                            overload.return_ty.is_some_and(|return_type| {
                                return_type.is_equivalent_to(db, Type::Never)
                            });

                        (
                            none && !overload_returns_never,
                            all && overload_returns_never,
                        )
                    });

                if no_overloads_return_never {
                    Truthiness::AlwaysFalse
                } else if all_overloads_return_never {
                    Truthiness::AlwaysTrue
                } else {
                    let call_expr_ty = infer_expression_type(db, call_expr, TypeContext::default());
                    if call_expr_ty.is_equivalent_to(db, Type::Never) {
                        Truthiness::AlwaysTrue
                    } else {
                        Truthiness::AlwaysFalse
                    }
                }
                .negate_if(!predicate.is_positive)
            }
            PredicateNode::Pattern(inner) => analyze_pattern_predicate(db, inner),
            PredicateNode::StarImportPlaceholder(star_import) => {
                let place_table = place_table(db, star_import.scope(db));
                let symbol = place_table.symbol(star_import.symbol_id(db));
                let referenced_file = star_import.referenced_file(db);

                let requires_explicit_reexport = match dunder_all_names(db, referenced_file) {
                    Some(all_names) => {
                        if all_names.contains(symbol.name()) {
                            Some(RequiresExplicitReExport::No)
                        } else {
                            tracing::trace!(
                                "Symbol `{}` (via star import) not found in `__all__` of `{}`",
                                symbol.name(),
                                referenced_file.path(db)
                            );
                            return Truthiness::AlwaysFalse;
                        }
                    }
                    None => None,
                };

                match imported_symbol(
                    db,
                    referenced_file,
                    symbol.name(),
                    requires_explicit_reexport,
                )
                .place
                {
                    crate::place::Place::Defined(
                        _,
                        _,
                        crate::place::Definedness::AlwaysDefined,
                        _,
                    ) => Truthiness::AlwaysTrue,
                    crate::place::Place::Defined(
                        _,
                        _,
                        crate::place::Definedness::PossiblyUndefined,
                        _,
                    ) => Truthiness::Ambiguous,
                    crate::place::Place::Undefined => Truthiness::AlwaysFalse,
                }
            }
        }
    }
}
