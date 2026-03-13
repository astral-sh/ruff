//! # Reachability evaluation
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

use crate::{
    Db,
    dunder_all::dunder_all_names,
    place::{DefinedPlace, Definedness, Place, RequiresExplicitReExport, imported_symbol},
    types::{
        CallableTypes, IntersectionBuilder, KnownClass, NarrowingConstraint, Type, TypeContext,
        UnionBuilder, UnionType, infer_expression_type, infer_narrowing_constraint,
    },
};
use ruff_text_size::TextRange;
use ty_python_core::{
    BindingWithConstraints, DeclarationWithConstraint, DeclarationsIterator, FileScopeId,
    SemanticIndex, Truthiness, UseDefMap,
    definition::DefinitionState,
    place::ScopedPlaceId,
    place_table,
    predicate::{
        CallableAndCallExpr, PatternPredicate, PatternPredicateKind, Predicate, PredicateNode,
        Predicates,
    },
    reachability_constraints::{ReachabilityConstraints, ScopedReachabilityConstraintId},
};

fn singleton_to_type(db: &dyn Db, singleton: ruff_python_ast::Singleton) -> Type<'_> {
    let ty = match singleton {
        ruff_python_ast::Singleton::None => Type::none(db),
        ruff_python_ast::Singleton::True => Type::bool_literal(true),
        ruff_python_ast::Singleton::False => Type::bool_literal(false),
    };
    debug_assert!(ty.is_singleton(db));
    ty
}

fn mapping_pattern_type(db: &dyn Db) -> Type<'_> {
    KnownClass::Mapping.to_instance(db).top_materialization(db)
}

/// Turn a `match` pattern kind into a type that represents the set of all values that would definitely
/// match that pattern.
fn pattern_kind_to_type<'db>(db: &'db dyn Db, kind: &PatternPredicateKind<'db>) -> Type<'db> {
    match kind {
        PatternPredicateKind::Singleton(singleton) => singleton_to_type(db, *singleton),
        PatternPredicateKind::Value(value) => {
            let ty = infer_expression_type(db, *value, TypeContext::default());
            // Only return the type if it's single-valued. For non-single-valued types
            // (like `str`), we can't definitively exclude any specific type from
            // subsequent patterns because the pattern could match any value of that type.
            if ty.is_single_valued(db) {
                ty
            } else {
                Type::Never
            }
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
        PatternPredicateKind::Mapping(kind) => {
            if kind.is_irrefutable() {
                mapping_pattern_type(db)
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
#[salsa::tracked(
    cycle_initial = |_, _, _| Truthiness::Ambiguous,
    heap_size = get_size2::GetSize::get_heap_size
)]
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

    let truthiness =
        analyze_single_pattern_predicate_kind(db, predicate.kind(db), narrowed_subject_ty);

    if truthiness == Truthiness::AlwaysTrue && predicate.guard(db).is_some() {
        // Fall back to ambiguous, the guard might change the result.
        // TODO: actually analyze guard truthiness
        Truthiness::Ambiguous
    } else {
        truthiness
    }
}

/// AND a new optional narrowing constraint with an accumulated one.
fn accumulate_constraint<'db>(
    accumulated: Option<NarrowingConstraint<'db>>,
    new: Option<NarrowingConstraint<'db>>,
) -> Option<NarrowingConstraint<'db>> {
    match (accumulated, new) {
        (Some(acc), Some(new_c)) => Some(new_c.merge_constraint_and(acc)),
        (None, Some(new_c)) => Some(new_c),
        (Some(acc), None) => Some(acc),
        (None, None) => None,
    }
}

pub(crate) trait ReachabilityConstraintsExtension<'db> {
    /// Narrow a type by walking a TDD narrowing constraint.
    fn narrow_by_constraint(
        &self,
        db: &'db dyn Db,
        predicates: &Predicates<'db>,
        id: ScopedReachabilityConstraintId,
        base_ty: Type<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db>;

    /// Analyze the statically known reachability for a given constraint.
    fn evaluate(
        &self,
        db: &'db dyn Db,
        predicates: &Predicates<'db>,
        id: ScopedReachabilityConstraintId,
    ) -> Truthiness;
}

impl<'db> ReachabilityConstraintsExtension<'db> for ReachabilityConstraints {
    /// Narrow a type by walking a TDD narrowing constraint.
    ///
    /// The TDD represents a ternary formula over predicates that encodes which predicates
    /// hold along a particular control flow path. We walk from root to leaves, accumulating
    /// narrowing constraints.
    ///
    /// At each interior node, we branch based on whether the predicate is true or false:
    /// - True branch: apply positive narrowing from the predicate
    /// - False branch: apply negative narrowing from the predicate
    ///
    /// The "ambiguous" branch in the TDD is not followed for narrowing purposes, because
    /// narrowing constraints record which predicates hold along the control flow path.
    /// The predicates may be statically ambiguous (we can't determine their truthiness
    /// at analysis time), but they still hold dynamically at runtime and should be used
    /// for narrowing.
    ///
    /// At leaves:
    /// - `ALWAYS_TRUE` or `AMBIGUOUS`: apply all accumulated narrowing to the base type
    /// - `ALWAYS_FALSE`: this path is impossible → Never
    ///
    /// The final result is the union of all path results.
    fn narrow_by_constraint(
        &self,
        db: &'db dyn Db,
        predicates: &Predicates<'db>,
        id: ScopedReachabilityConstraintId,
        base_ty: Type<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        narrow_by_constraint_inner(db, self, predicates, id, base_ty, place, None)
    }

    /// Analyze the statically known reachability for a given constraint.
    fn evaluate(
        &self,
        db: &'db dyn Db,
        predicates: &Predicates<'db>,
        mut id: ScopedReachabilityConstraintId,
    ) -> Truthiness {
        type Id = ScopedReachabilityConstraintId;

        loop {
            let node = match id {
                Id::ALWAYS_TRUE => return Truthiness::AlwaysTrue,
                Id::AMBIGUOUS => return Truthiness::Ambiguous,
                Id::ALWAYS_FALSE => return Truthiness::AlwaysFalse,
                _ => {
                    // `id` gives us the index of this node in the IndexVec that we used when
                    // constructing this BDD. When finalizing the builder, we threw away any
                    // interior nodes that weren't marked as used. The `used_indices` bit vector
                    // lets us verify that this node was marked as used, and the rank of that bit
                    // in the bit vector tells us where this node lives in the "condensed"
                    // `used_interiors` vector.
                    let raw_index = id.as_u32() as usize;
                    debug_assert!(
                        self.used_indices().get_bit(raw_index).unwrap_or(false),
                        "all used reachability constraints should have been marked as used",
                    );
                    let index = self.used_indices().rank(raw_index) as usize;
                    self.used_interiors()[index]
                }
            };
            let predicate = &predicates[node.atom()];
            match analyze_single(db, predicate) {
                Truthiness::AlwaysTrue => id = node.if_true(),
                Truthiness::Ambiguous => id = node.if_ambiguous(),
                Truthiness::AlwaysFalse => id = node.if_false(),
            }
        }
    }
}

/// Inner recursive helper that accumulates narrowing constraints along each TDD path.
fn narrow_by_constraint_inner<'db>(
    db: &'db dyn Db,
    constraints: &ReachabilityConstraints,
    predicates: &Predicates<'db>,
    id: ScopedReachabilityConstraintId,
    base_ty: Type<'db>,
    place: ScopedPlaceId,
    accumulated: Option<NarrowingConstraint<'db>>,
) -> Type<'db> {
    type Id = ScopedReachabilityConstraintId;

    match id {
        Id::ALWAYS_TRUE | Id::AMBIGUOUS => {
            // Apply all accumulated narrowing constraints to the base type
            match accumulated {
                Some(constraint) => constraint.narrow_base_type(db, base_ty),
                None => base_ty,
            }
        }
        Id::ALWAYS_FALSE => Type::Never,
        _ => {
            let node = constraints.get_interior_node(id);
            let predicate = predicates[node.atom()];

            // `IsNonTerminalCall` predicates don't narrow any variable; they only
            // affect reachability. Evaluate the predicate to determine which
            // path(s) are reachable, rather than walking both branches.
            // `IsNonTerminalCall` always evaluates to `AlwaysTrue` or `AlwaysFalse`,
            // never `Ambiguous`.
            if matches!(predicate.node, PredicateNode::IsNonTerminalCall(_)) {
                return match analyze_single(db, &predicate) {
                    Truthiness::AlwaysTrue => narrow_by_constraint_inner(
                        db,
                        constraints,
                        predicates,
                        node.if_true(),
                        base_ty,
                        place,
                        accumulated,
                    ),
                    Truthiness::AlwaysFalse => narrow_by_constraint_inner(
                        db,
                        constraints,
                        predicates,
                        node.if_false(),
                        base_ty,
                        place,
                        accumulated,
                    ),
                    Truthiness::Ambiguous => {
                        unreachable!("`IsNonTerminalCall` predicates should never be Ambiguous")
                    }
                };
            }

            // Check if this predicate narrows the variable we're interested in.
            let pos_constraint = infer_narrowing_constraint(db, predicate, place);

            // If the true branch is statically unreachable, skip it entirely.
            if node.if_true() == Id::ALWAYS_FALSE {
                let neg_predicate = Predicate {
                    node: predicate.node,
                    is_positive: !predicate.is_positive,
                };
                let neg_constraint = infer_narrowing_constraint(db, neg_predicate, place);
                let false_accumulated = accumulate_constraint(accumulated, neg_constraint);
                return narrow_by_constraint_inner(
                    db,
                    constraints,
                    predicates,
                    node.if_false(),
                    base_ty,
                    place,
                    false_accumulated,
                );
            }

            // If the false branch is statically unreachable, skip it entirely.
            if node.if_false() == Id::ALWAYS_FALSE {
                let true_accumulated = accumulate_constraint(accumulated, pos_constraint);
                return narrow_by_constraint_inner(
                    db,
                    constraints,
                    predicates,
                    node.if_true(),
                    base_ty,
                    place,
                    true_accumulated,
                );
            }

            // True branch: predicate holds → accumulate positive narrowing
            let true_accumulated = accumulate_constraint(accumulated.clone(), pos_constraint);
            let true_ty = narrow_by_constraint_inner(
                db,
                constraints,
                predicates,
                node.if_true(),
                base_ty,
                place,
                true_accumulated,
            );

            // False branch: predicate doesn't hold → accumulate negative narrowing
            let neg_predicate = Predicate {
                node: predicate.node,
                is_positive: !predicate.is_positive,
            };
            let neg_constraint = infer_narrowing_constraint(db, neg_predicate, place);
            let false_accumulated = accumulate_constraint(accumulated, neg_constraint);
            let false_ty = narrow_by_constraint_inner(
                db,
                constraints,
                predicates,
                node.if_false(),
                base_ty,
                place,
                false_accumulated,
            );

            UnionType::from_two_elements(db, true_ty, false_ty)
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
            let (ControlFlow::Break(truthiness) | ControlFlow::Continue(truthiness)) = predicates
                .iter()
                .map(|p| {
                    let narrowed_subject_ty = IntersectionBuilder::new(db)
                        .add_positive(subject_ty)
                        .add_negative(UnionType::from_elements(db, excluded_types.iter()))
                        .build();

                    excluded_types.push(pattern_kind_to_type(db, p));

                    analyze_single_pattern_predicate_kind(db, p, narrowed_subject_ty)
                })
                // this is just a "max", but with a slight optimization:
                // `AlwaysTrue` is the "greatest" possible element, so we short-circuit if we get there
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
        PatternPredicateKind::Mapping(kind) => {
            let mapping_ty = mapping_pattern_type(db);
            if subject_ty.is_subtype_of(db, mapping_ty) {
                if kind.is_irrefutable() {
                    Truthiness::AlwaysTrue
                } else {
                    Truthiness::Ambiguous
                }
            } else if subject_ty.is_disjoint_from(db, mapping_ty) {
                Truthiness::AlwaysFalse
            } else {
                Truthiness::Ambiguous
            }
        }
        PatternPredicateKind::As(pattern, _) => pattern
            .as_deref()
            .map(|p| analyze_single_pattern_predicate_kind(db, p, subject_ty))
            .unwrap_or(Truthiness::AlwaysTrue),
        PatternPredicateKind::Unsupported => Truthiness::Ambiguous,
    }
}

fn analyze_single(db: &dyn Db, predicate: &Predicate) -> Truthiness {
    let _span = tracing::trace_span!("analyze_single", ?predicate).entered();

    match predicate.node {
        PredicateNode::Expression(test_expr) => {
            infer_expression_type(db, test_expr, TypeContext::default())
                .bool(db)
                .negate_if(!predicate.is_positive)
        }
        PredicateNode::IsNonTerminalCall(CallableAndCallExpr {
            callable,
            call_expr,
            is_await,
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
                return Truthiness::AlwaysTrue.negate_if(!predicate.is_positive);
            }

            let overloads_iterator = if let Some(callable) = ty
                .try_upcast_to_callable(db)
                .and_then(CallableTypes::exactly_one)
            {
                callable.signatures(db).overloads.iter()
            } else {
                return Truthiness::AlwaysTrue.negate_if(!predicate.is_positive);
            };

            let mut no_overloads_return_never = true;
            let mut all_overloads_return_never = true;
            let mut any_overload_is_generic = false;

            for overload in overloads_iterator {
                let returns_never = overload.return_ty.is_equivalent_to(db, Type::Never);
                no_overloads_return_never &= !returns_never;
                all_overloads_return_never &= returns_never;
                any_overload_is_generic |= overload.return_ty.has_typevar(db);
            }

            if no_overloads_return_never && !any_overload_is_generic && !is_await {
                Truthiness::AlwaysTrue
            } else if all_overloads_return_never {
                Truthiness::AlwaysFalse
            } else {
                let call_expr_ty = infer_expression_type(db, call_expr, TypeContext::default());
                if call_expr_ty.is_equivalent_to(db, Type::Never) {
                    Truthiness::AlwaysFalse
                } else {
                    Truthiness::AlwaysTrue
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
                Some(referenced_file),
                symbol.name(),
                requires_explicit_reexport,
            )
            .place
            {
                Place::Defined(DefinedPlace {
                    definedness: Definedness::AlwaysDefined,
                    ..
                }) => Truthiness::AlwaysTrue,
                Place::Defined(DefinedPlace {
                    definedness: Definedness::PossiblyUndefined,
                    ..
                }) => Truthiness::Ambiguous,
                Place::Undefined => Truthiness::AlwaysFalse,
            }
        }
    }
}

/// Check whether a diagnostic emitted at `range` is in reachable code, considering both
/// scope reachability and statement-level reachability within the scope.
pub(crate) fn is_range_reachable<'db>(
    db: &'db dyn crate::Db,
    index: &SemanticIndex<'db>,
    scope_id: FileScopeId,
    range: TextRange,
) -> bool {
    index.ancestor_scopes(scope_id).all(|(scope_id, _)| {
        let use_def = index.use_def_map(scope_id);
        !use_def
            .range_reachability()
            .any(|(entry_range, constraint)| {
                entry_range.contains_range(range) && !is_reachable(db, use_def, constraint)
            })
    })
}

pub(crate) fn is_reachable<'db>(
    db: &'db dyn Db,
    use_def: &UseDefMap<'db>,
    reachability: ScopedReachabilityConstraintId,
) -> bool {
    evaluate_reachability(db, use_def, reachability).may_be_true()
}

pub(crate) fn binding_reachability<'db, 'map>(
    db: &'db dyn Db,
    use_def: &'map UseDefMap<'db>,
    binding: &BindingWithConstraints<'map, 'db>,
) -> Truthiness {
    use_def.reachability_constraints().evaluate(
        db,
        use_def.predicates(),
        binding.reachability_constraint,
    )
}

pub(crate) fn evaluate_reachability(
    db: &dyn Db,
    use_def: &UseDefMap,
    reachability: ScopedReachabilityConstraintId,
) -> Truthiness {
    use_def
        .reachability_constraints()
        .evaluate(db, use_def.predicates(), reachability)
}

pub(crate) trait DeclarationsIteratorExtension<'db> {
    fn any_reachable(
        self,
        db: &'db dyn Db,
        predicate: impl FnMut(DefinitionState<'db>) -> bool,
    ) -> bool;
}

impl<'db> DeclarationsIteratorExtension<'db> for DeclarationsIterator<'_, 'db> {
    fn any_reachable(
        mut self,
        db: &'db dyn Db,
        mut predicate: impl FnMut(DefinitionState<'db>) -> bool,
    ) -> bool {
        let predicates = self.predicates();
        let reachability_constraints = self.reachability_constraints();

        self.any(
            |DeclarationWithConstraint {
                 declaration,
                 reachability_constraint,
             }| {
                predicate(declaration)
                    && !reachability_constraints
                        .evaluate(db, predicates, reachability_constraint)
                        .is_always_false()
            },
        )
    }
}
