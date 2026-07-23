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

use std::cell::RefCell;

use crate::{
    Db,
    dunder_all::dunder_all_names,
    place::{DefinedPlace, Definedness, Place, RequiresExplicitReExport, imported_symbol},
    types::{
        ActiveRecursionDetector, CallableTypes, ComparisonSoundnessPolicy, EnumClassLiteral,
        KnownInstanceType, NarrowingConstraint, SpecialFormType, Type, TypeContext, UnionType,
        callable_pattern_type, definite_match_pattern_type,
        definite_match_pattern_type_for_subject, equality_truthiness, expand_type,
        infer_narrowing_constraints, infer_same_file_expression_type, mapping_pattern_type,
        pattern_binding_fallthrough_type, sequence_pattern_type_builder, singleton_pattern_type,
    },
};
use ruff_index::{Idx, IndexSlice};
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};
use salsa::plumbing::AsId;
use smallvec::SmallVec;
use ty_python_core::{
    BindingWithConstraints, DeclarationWithConstraint, DeclarationsIterator, FileScopeId,
    ScopedDefinitionId, SemanticIndex, Truthiness, UseDefMap,
    definition::DefinitionState,
    expression::Expression,
    narrowing_constraints::{NarrowingConstraints, ScopedNarrowingConstraint},
    place::ScopedPlaceId,
    place_table,
    predicate::{
        CallableAndCallExpr, PatternPredicate, PatternPredicateKind, Predicate, PredicateNode,
        ScopedPredicateId,
    },
    reachability_constraints::{ReachabilityConstraints, ScopedReachabilityConstraintId},
    scope::ScopeId,
    use_def_map,
};

/// Narrow `subject_ty` by all preceding unguarded match patterns.
///
/// Caching each prefix lets the next case reuse the already-normalized subject instead of
/// rebuilding it from the union of all preceding patterns, which can repeatedly distribute the
/// same intersections.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, id, _, _| Type::divergent(id),
    cycle_fn = |db, cycle, previous: &Type<'db>, result: Type<'db>, _, _| {
        result.cycle_normalized(db, *previous, cycle)
    },
    heap_size = ruff_memory_usage::heap_size
)]
pub(crate) fn type_narrowed_by_previous_patterns<'db>(
    db: &'db dyn Db,
    predicate: PatternPredicate<'db>,
    subject_ty: Type<'db>,
) -> Type<'db> {
    let Some(previous) = predicate.previous_predicate(db) else {
        return subject_ty;
    };
    let previous = *previous;
    let narrowed_by_previous_patterns =
        type_narrowed_by_previous_patterns(db, previous, subject_ty);

    if previous.guard(db).is_some() {
        narrowed_by_previous_patterns
    } else {
        type_narrowed_by_pattern(db, previous, narrowed_by_previous_patterns)
    }
}

/// Narrow `subject_ty` by a match pattern.
///
/// This result is also the preceding-pattern prefix for the next unguarded case.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, id, _, _| Type::divergent(id),
    cycle_fn = |db, cycle, previous: &Type<'db>, result: Type<'db>, _, _| {
        result.cycle_normalized(db, *previous, cycle)
    },
    heap_size = ruff_memory_usage::heap_size
)]
fn type_narrowed_by_pattern<'db>(
    db: &'db dyn Db,
    predicate: PatternPredicate<'db>,
    subject_ty: Type<'db>,
) -> Type<'db> {
    pattern_binding_fallthrough_type(db, predicate.kind(db), subject_ty)
}

/// Return the enum class and canonical member names represented by an enum-literal subject type.
///
/// This succeeds only when the subject is a single enum literal, a union of enum literals from the
/// same enum class, or an alias to either form. Enum aliases are normalized to the canonical member
/// name so previous `match` cases can be compared by member identity.
fn enum_literal_subject_names<'db>(
    db: &'db dyn Db,
    subject_ty: Type<'db>,
) -> Option<(EnumClassLiteral<'db>, FxHashSet<Name>)> {
    fn add_enum_literal<'db>(
        db: &'db dyn Db,
        enum_class: &mut Option<EnumClassLiteral<'db>>,
        names: &mut FxHashSet<Name>,
        ty: Type<'db>,
    ) -> Option<()> {
        let enum_literal = ty.as_enum_literal()?;
        let class = enum_literal.enum_class_literal(db);

        if let Some(existing_class) = *enum_class {
            if existing_class != class {
                return None;
            }
        } else {
            *enum_class = Some(class);
        }

        let name = enum_literal.name(db);
        let canonical_name = class.resolve_member(db, name)?;
        names.insert(canonical_name.clone());
        Some(())
    }

    let mut enum_class = None;
    let mut names = FxHashSet::default();

    match subject_ty {
        Type::LiteralValue(_) => {
            add_enum_literal(db, &mut enum_class, &mut names, subject_ty)?;
        }
        Type::Union(union) => {
            for element in union.elements(db) {
                add_enum_literal(db, &mut enum_class, &mut names, *element)?;
            }
        }
        Type::TypeAlias(alias) => return enum_literal_subject_names(db, alias.value_type(db)),
        _ => return None,
    }

    Some((enum_class?, names))
}

/// Return the canonical enum-member name matched by a single value pattern.
///
/// This recognizes patterns like `case Color.RED:` only when the pattern expression is
/// single-valued and belongs to the expected enum class. Enum aliases are resolved to their
/// canonical member names before returning.
fn enum_member_pattern_name<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    kind: &PatternPredicateKind<'db>,
) -> Option<Name> {
    let value_ty = definite_match_pattern_type(db, kind);
    let enum_literal = value_ty.as_enum_literal()?;
    if enum_literal.enum_class_literal(db) != enum_class {
        return None;
    }

    let name = enum_literal.name(db);
    let canonical_name = enum_class.resolve_member(db, name)?;
    Some(canonical_name.clone())
}

struct EnumMemberPatternCoverage {
    /// Enum members that this pattern definitely matches.
    definitely_matched: FxHashSet<Name>,
    /// Whether the collected coverage is known to represent every possible matching enum member.
    is_exact: bool,
}

/// Returns enum-member coverage evidence for a pattern.
///
/// This recognizes patterns like `case Color.RED | Color.GREEN` when the pattern
/// belongs to the expected enum class. Enum aliases are resolved to their canonical member names
/// before returning. A pattern with additional alternatives such as `Color.GREEN | Color()`
/// produces only a lower bound: it definitely matches `Color.GREEN`, but can match other members.
fn enum_member_pattern_coverage<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    kind: &PatternPredicateKind<'db>,
) -> EnumMemberPatternCoverage {
    let mut coverage = EnumMemberPatternCoverage {
        definitely_matched: FxHashSet::default(),
        is_exact: true,
    };
    match kind {
        PatternPredicateKind::Or(alts) => {
            for alt in alts {
                let alt_coverage = enum_member_pattern_coverage(db, enum_class, alt);
                coverage
                    .definitely_matched
                    .extend(alt_coverage.definitely_matched);
                coverage.is_exact &= alt_coverage.is_exact;
            }
        }
        PatternPredicateKind::As(Some(inner), _) => {
            return enum_member_pattern_coverage(db, enum_class, inner);
        }
        _ => {
            if let Some(name) = enum_member_pattern_name(db, enum_class, kind) {
                coverage.definitely_matched.insert(name);
            } else {
                coverage.is_exact = false;
            }
        }
    }
    coverage
}

/// Determine the static truthiness of a `match` case over a union of enum literals.
///
/// The analysis removes enum members already matched by earlier unguarded cases, then decides
/// whether the current case is impossible, exhaustive, or still ambiguous. Guarded cases remain
/// ambiguous because the guard can reject an otherwise matching enum member.
fn analyze_enum_literal_union_pattern_predicate<'db>(
    db: &'db dyn Db,
    predicate: PatternPredicate<'db>,
    subject_ty: Type<'db>,
) -> Option<Truthiness> {
    let (enum_class, mut remaining_names) = enum_literal_subject_names(db, subject_ty)?;
    let current_coverage = enum_member_pattern_coverage(db, enum_class, predicate.kind(db));
    let current_names = &current_coverage.definitely_matched;
    if current_names.is_empty() {
        return None;
    }

    let mut previous_predicate = predicate;
    while let Some(previous) = previous_predicate.previous_predicate(db) {
        previous_predicate = *previous;

        if previous_predicate.guard(db).is_some() {
            continue;
        }

        let previous_coverage =
            enum_member_pattern_coverage(db, enum_class, previous_predicate.kind(db));
        #[expect(
            clippy::iter_over_hash_type,
            reason = "set removal is independent of iteration order"
        )]
        for previous_name in previous_coverage.definitely_matched {
            remaining_names.remove(&previous_name);
        }
    }

    if remaining_names.is_empty() {
        return Some(Truthiness::AlwaysFalse);
    }

    if remaining_names.is_subset(current_names) {
        if predicate.guard(db).is_some() {
            Some(Truthiness::Ambiguous)
        } else {
            Some(Truthiness::AlwaysTrue)
        }
    } else if current_coverage.is_exact {
        if remaining_names.is_disjoint(current_names) {
            Some(Truthiness::AlwaysFalse)
        } else {
            Some(Truthiness::Ambiguous)
        }
    } else {
        None
    }
}

/// Analyze a pattern predicate to determine its static truthiness.
///
/// This is a Salsa tracked function to enable memoization. Without memoization, for a match
/// statement with N cases where each case references the subject (e.g., `self`), we would
/// re-analyze each pattern O(N) times (once per reference), leading to O(N²) total work.
/// With memoization, each pattern is analyzed exactly once.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| Truthiness::Ambiguous,
    heap_size = get_size2::GetSize::get_heap_size
)]
fn analyze_pattern_predicate<'db>(db: &'db dyn Db, predicate: PatternPredicate<'db>) -> Truthiness {
    let subject_ty =
        infer_same_file_expression_type(db, predicate.subject(db), TypeContext::default());

    if let Some(truthiness) =
        analyze_enum_literal_union_pattern_predicate(db, predicate, subject_ty)
    {
        return truthiness;
    }

    let coverage_subject_ty = expand_type(db, subject_ty)
        .map(|types| UnionType::from_elements(db, types))
        .unwrap_or(subject_ty);
    let narrowed_subject_ty =
        type_narrowed_by_previous_patterns(db, predicate, coverage_subject_ty);

    // Consider a case where we match on a subject type of `Self` with an upper bound of `Answer`,
    // where `Answer` is a {YES, NO} enum. After a previous pattern matching on `NO`, the narrowed
    // subject type is `Self & ~Literal[NO]`. This type is *not* equivalent to `Literal[YES]`,
    // because `Self` could also specialize to `Literal[NO]` or `Never`, making the intersection
    // empty. However, if the current pattern matches on `YES`, the *next* narrowed subject type
    // will be `Self & ~Literal[NO] & ~Literal[YES]`, which *is* always equivalent to `Never`. This
    // means that subsequent patterns can never match. And we know that if we reach this point,
    // the current pattern will have to match. We return `AlwaysTrue` here, since the call to
    // `analyze_single_pattern_predicate_kind` below would return `Ambiguous` in this case.
    let next_narrowed_subject_ty = type_narrowed_by_pattern(db, predicate, narrowed_subject_ty);
    if !narrowed_subject_ty.is_never() && next_narrowed_subject_ty.is_never() {
        return Truthiness::AlwaysTrue;
    }

    let truthiness =
        analyze_single_pattern_predicate_kind(db, predicate.kind(db), narrowed_subject_ty, None);

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

std::thread_local! {
    static ACTIVE_NON_TERMINAL_CALL_PREFIXES: ActiveRecursionDetector<salsa::Id> = ActiveRecursionDetector::default();
}

const NON_TERMINAL_CALL_CHUNK_SIZE: usize = 16;
const REACHABILITY_EVALUATION_CHUNK_SIZE: usize = 256;

fn predicate_scope<'db>(db: &'db dyn Db, predicate: &Predicate<'db>) -> ScopeId<'db> {
    match predicate.node {
        PredicateNode::Expression(expression) => expression.scope(db),
        PredicateNode::IsNonTerminalCall(CallableAndCallExpr { callable, .. }) => {
            callable.scope(db)
        }
        PredicateNode::Pattern(pattern) => pattern.scope(db),
        PredicateNode::SubjectElementPattern(subject_element) => subject_element.pattern.scope(db),
        PredicateNode::IsNonEmptyIterable(expression) => expression.scope(db),
        PredicateNode::StarImportPlaceholder(star_import) => star_import.scope(db),
    }
}

/// Infers preceding call predicates in source order.
///
/// Predicate IDs are assigned in source order, but the decision diagrams intentionally order
/// predicates in reverse to reduce their size. Inferring a later call can depend on the
/// reachability of all preceding calls, which otherwise creates a deeply recursive Salsa query
/// chain. Inferring the expressions in source order turns that chain into cache lookups while
/// preserving normal reachability and narrowing during every inference.
///
/// Because the prefix is based on predicate indices rather than graph reachability, branch-heavy
/// code can warm calls from earlier source branches that this evaluation would not otherwise visit.
/// A demand-driven graph walk could avoid that work, but would require a more complex work list. We
/// accept the broader eager pass because it keeps the ordering simple, and checking a scope will
/// typically exercise most of its predicates eventually.
///
/// Reentrant analysis of the same predicate graph skips the prefix pass: because the outer pass is
/// proceeding in source order, any preceding call needed by the current expression has already
/// been inferred. A different predicate graph performs its own pass, which is necessary when
/// inferring a call crosses into another large scope.
fn analyze_non_terminal_call_prefix<'db>(
    db: &'db dyn Db,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    root_predicate: ScopedPredicateId,
) -> bool {
    let scope = predicate_scope(db, &predicates[root_predicate]);
    let has_many_calls = predicates
        .iter()
        .filter(|predicate| matches!(predicate.node, PredicateNode::IsNonTerminalCall(_)))
        .nth(NON_TERMINAL_CALL_CHUNK_SIZE)
        .is_some();

    ACTIVE_NON_TERMINAL_CALL_PREFIXES.with(|active| {
        active.visit(
            &scope.as_id(),
            || {},
            || {
                if !has_many_calls {
                    for predicate in &predicates.raw[..=root_predicate.index()] {
                        if matches!(predicate.node, PredicateNode::IsNonTerminalCall(_)) {
                            analyze_single(db, predicate);
                        }
                    }
                    return;
                }

                let call_predicates = non_terminal_call_predicates(db, scope);
                let call_count =
                    call_predicates.partition_point(|predicate| *predicate <= root_predicate);
                if call_count <= NON_TERMINAL_CALL_CHUNK_SIZE {
                    analyze_non_terminal_calls(db, predicates, &call_predicates[..call_count]);
                    return;
                }

                let mut start = 0;
                let mut remaining = call_count / NON_TERMINAL_CALL_CHUNK_SIZE;

                while remaining > 0 {
                    let level = remaining.ilog2();
                    let length = 1 << level;
                    analyze_non_terminal_call_range(db, scope, level, start >> level);
                    start += length;
                    remaining -= length;
                }

                let tail_start =
                    call_count / NON_TERMINAL_CALL_CHUNK_SIZE * NON_TERMINAL_CALL_CHUNK_SIZE;
                analyze_non_terminal_calls(
                    db,
                    predicates,
                    &call_predicates[tail_start..call_count],
                );
            },
        );
    });

    has_many_calls
}

/// Returns the statement-call predicates for `scope` in source order.
///
/// This tracked index is used only once a scope exceeds [`NON_TERMINAL_CALL_CHUNK_SIZE`], avoiding
/// a persistent allocation for the common case of scopes with few calls.
#[salsa::tracked(returns(deref), heap_size = get_size2::GetSize::get_heap_size)]
fn non_terminal_call_predicates<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
) -> Box<[ScopedPredicateId]> {
    use_def_map(db, scope)
        .predicates()
        .iter_enumerated()
        .filter_map(|(id, predicate)| {
            matches!(predicate.node, PredicateNode::IsNonTerminalCall(_)).then_some(id)
        })
        .collect()
}

fn analyze_non_terminal_calls<'db>(
    db: &'db dyn Db,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    call_predicates: &[ScopedPredicateId],
) {
    for id in call_predicates {
        analyze_single(db, &predicates[*id]);
    }
}

/// Analyzes a power-of-two range of call-predicate blocks in source order.
///
/// Prefixes can be decomposed into these canonical ranges and reused by later expression-inference
/// queries. Splitting ranges in half keeps the Salsa query stack logarithmic even when the first
/// requested prefix contains thousands of calls. Each leaf handles multiple calls iteratively to
/// avoid retaining a Salsa argument and query result for every individual predicate.
#[salsa::tracked(returns(copy), heap_size = get_size2::GetSize::get_heap_size)]
fn analyze_non_terminal_call_range<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    level: u32,
    index: usize,
) {
    if level == 0 {
        let use_def = use_def_map(db, scope);
        let call_predicates = non_terminal_call_predicates(db, scope);
        let start = index * NON_TERMINAL_CALL_CHUNK_SIZE;
        let end = start + NON_TERMINAL_CALL_CHUNK_SIZE;
        analyze_non_terminal_calls(db, use_def.predicates(), &call_predicates[start..end]);
        return;
    }

    let child_index = index * 2;
    analyze_non_terminal_call_range(db, scope, level - 1, child_index);
    analyze_non_terminal_call_range(db, scope, level - 1, child_index + 1);
}

/// Evaluates a reachability constraint after warming its statement-call prefix.
///
/// Large scopes reuse canonical call ranges and sparse decision-diagram checkpoints; small scopes
/// retain the direct evaluation path without creating either cached index.
fn evaluate_reachability_constraint<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    id: ScopedReachabilityConstraintId,
) -> Truthiness {
    if let Some(reachability) = terminal_reachability(id) {
        return reachability;
    }

    let use_def = use_def_map(db, scope);
    let constraints = use_def.reachability_constraints();
    let predicates = use_def.predicates();
    let root_predicate = constraints.get_interior_node(id).atom();
    let has_many_calls = analyze_non_terminal_call_prefix(db, predicates, root_predicate);
    let call_predicates = has_many_calls.then(|| non_terminal_call_predicates(db, scope));

    evaluate_reachability_path(
        db,
        scope,
        constraints,
        predicates,
        call_predicates,
        id,
        true,
    )
}

fn terminal_reachability(id: ScopedReachabilityConstraintId) -> Option<Truthiness> {
    match id {
        ScopedReachabilityConstraintId::ALWAYS_TRUE => Some(Truthiness::AlwaysTrue),
        ScopedReachabilityConstraintId::AMBIGUOUS => Some(Truthiness::Ambiguous),
        ScopedReachabilityConstraintId::ALWAYS_FALSE => Some(Truthiness::AlwaysFalse),
        _ => None,
    }
}

fn is_reachability_checkpoint(
    call_predicates: &[ScopedPredicateId],
    predicate: ScopedPredicateId,
) -> bool {
    call_predicates
        .binary_search(&predicate)
        .is_ok_and(|index| (index + 1) % REACHABILITY_EVALUATION_CHUNK_SIZE == 0)
}

/// Walks a reachability decision diagram until it reaches a terminal or reusable checkpoint.
///
/// `use_checkpoint` is false only when entering from a checkpoint query. In that case, the first
/// node is evaluated directly to prevent the query from immediately calling itself again.
fn evaluate_reachability_path<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    constraints: &ReachabilityConstraints,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    call_predicates: Option<&[ScopedPredicateId]>,
    mut id: ScopedReachabilityConstraintId,
    mut use_checkpoint: bool,
) -> Truthiness {
    loop {
        if let Some(reachability) = terminal_reachability(id) {
            return reachability;
        }

        let node = constraints.get_interior_node(id);
        if use_checkpoint
            && call_predicates.is_some_and(|call_predicates| {
                is_reachability_checkpoint(call_predicates, node.atom())
            })
        {
            return evaluate_reachability_checkpoint(db, scope, id);
        }

        id = match analyze_single(db, &predicates[node.atom()]) {
            Truthiness::AlwaysTrue => node.if_true(),
            Truthiness::Ambiguous => node.if_ambiguous(),
            Truthiness::AlwaysFalse => node.if_false(),
        };
        use_checkpoint = true;
    }
}

/// Evaluates a canonical suffix of a reachability decision diagram.
///
/// Only every [`REACHABILITY_EVALUATION_CHUNK_SIZE`]th non-terminal-call predicate is a checkpoint.
/// This lets later statements reuse the constraints accumulated by earlier statements without
/// retaining a Salsa query key and memo for every reachability constraint in the scope.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _| Truthiness::Ambiguous,
    heap_size = get_size2::GetSize::get_heap_size
)]
fn evaluate_reachability_checkpoint<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    id: ScopedReachabilityConstraintId,
) -> Truthiness {
    let use_def = use_def_map(db, scope);
    evaluate_reachability_path(
        db,
        scope,
        use_def.reachability_constraints(),
        use_def.predicates(),
        Some(non_terminal_call_predicates(db, scope)),
        id,
        false,
    )
}

pub(crate) trait ReachabilityConstraintsExtension<'db> {
    /// Analyze the statically known reachability for a given constraint.
    fn evaluate(
        &self,
        db: &'db dyn Db,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedReachabilityConstraintId,
    ) -> Truthiness;
}

impl<'db> ReachabilityConstraintsExtension<'db> for ReachabilityConstraints {
    /// Analyze the statically known reachability for a given constraint.
    fn evaluate(
        &self,
        db: &'db dyn Db,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedReachabilityConstraintId,
    ) -> Truthiness {
        if let Some(reachability) = terminal_reachability(id) {
            return reachability;
        }

        let root_predicate = self.get_interior_node(id).atom();
        analyze_non_terminal_call_prefix(db, predicates, root_predicate);
        evaluate_reachability_path(
            db,
            predicate_scope(db, &predicates[root_predicate]),
            self,
            predicates,
            None,
            id,
            true,
        )
    }
}

pub(crate) fn narrow_type_by_constraint<'db>(
    db: &'db dyn Db,
    constraints: &NarrowingConstraints,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    id: ScopedNarrowingConstraint,
    base_ty: Type<'db>,
    place: ScopedPlaceId,
) -> Type<'db> {
    match id {
        ScopedNarrowingConstraint::ALWAYS_TRUE => return base_ty,
        ScopedNarrowingConstraint::ALWAYS_FALSE => return Type::Never,
        _ => {}
    }

    let mut projector = NarrowingProjector::new(db, constraints, predicates, place);
    let projected_root = projector.project(id);
    let mut context = ProjectedNarrowingContext {
        db,
        base_ty,
        graph: &projector.graph,
        joins: projector.graph.joins(projected_root),
        join_cache: FxHashMap::default(),
    };
    context.narrow(projected_root, None)
}

fn apply_accumulated_narrowing<'db>(
    db: &'db dyn Db,
    base_ty: Type<'db>,
    accumulated: Option<NarrowingConstraint<'db>>,
) -> Type<'db> {
    match accumulated {
        Some(constraint) => NarrowingConstraint::intersection(base_ty)
            .merge_constraint_and(constraint)
            .evaluate_constraint_type(db),
        None => base_ty,
    }
}

/// Identifier for a node in a projected narrowing graph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProjectedNarrowingNodeId(usize);

impl ProjectedNarrowingNodeId {
    /// Terminal node for paths that remain reachable.
    const ALWAYS_TRUE: Self = Self(usize::MAX);
    /// Terminal node for paths that are statically unreachable.
    const ALWAYS_FALSE: Self = Self(usize::MAX - 1);

    fn is_terminal(self) -> bool {
        self == Self::ALWAYS_TRUE || self == Self::ALWAYS_FALSE
    }
}

/// Interior node in a projected narrowing graph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProjectedNarrowingNode {
    atom: ScopedPredicateId,
    if_true: ProjectedNarrowingNodeId,
    if_uncertain: ProjectedNarrowingNodeId,
    if_false: ProjectedNarrowingNodeId,
}

/// Narrowing graph containing only predicates that can narrow one place.
#[derive(Default)]
struct ProjectedNarrowingGraph<'db> {
    nodes: Vec<ProjectedNarrowingNode>,
    node_cache: FxHashMap<ProjectedNarrowingNode, ProjectedNarrowingNodeId>,
    or_cache:
        FxHashMap<(ProjectedNarrowingNodeId, ProjectedNarrowingNodeId), ProjectedNarrowingNodeId>,
    predicate_constraints_cache: FxHashMap<
        ScopedPredicateId,
        (
            Option<NarrowingConstraint<'db>>,
            Option<NarrowingConstraint<'db>>,
        ),
    >,
}

impl ProjectedNarrowingGraph<'_> {
    /// Returns an interior projected node by ID.
    fn node(&self, id: ProjectedNarrowingNodeId) -> ProjectedNarrowingNode {
        self.nodes[id.0]
    }

    /// Interns a projected node, collapsing nodes with identical branches.
    fn add_node(&mut self, node: ProjectedNarrowingNode) -> ProjectedNarrowingNodeId {
        if node.if_uncertain == ProjectedNarrowingNodeId::ALWAYS_TRUE {
            return ProjectedNarrowingNodeId::ALWAYS_TRUE;
        }
        if node.if_true == node.if_false && node.if_true == node.if_uncertain {
            return node.if_true;
        }

        // Find and absorb cofactors if we can. (See `ty_python_core::narrowing_constraints` for
        // more details.)
        // `if_uncertain` contributes to both cofactors. If either cofactor is already true,
        // then the remaining cofactor can be lifted into `if_uncertain`, avoiding shapes like
        // `A or (not A and B)`.
        let when_true = self.or(node.if_true, node.if_uncertain);
        let when_false = self.or(node.if_false, node.if_uncertain);
        if when_true == when_false {
            return when_true;
        }
        if when_true == ProjectedNarrowingNodeId::ALWAYS_TRUE
            && !(node.if_true == ProjectedNarrowingNodeId::ALWAYS_TRUE
                && node.if_false == ProjectedNarrowingNodeId::ALWAYS_FALSE)
        {
            return self.add_node(ProjectedNarrowingNode {
                atom: node.atom,
                if_true: ProjectedNarrowingNodeId::ALWAYS_TRUE,
                if_uncertain: when_false,
                if_false: ProjectedNarrowingNodeId::ALWAYS_FALSE,
            });
        }
        if when_false == ProjectedNarrowingNodeId::ALWAYS_TRUE
            && !(node.if_true == ProjectedNarrowingNodeId::ALWAYS_FALSE
                && node.if_false == ProjectedNarrowingNodeId::ALWAYS_TRUE)
        {
            return self.add_node(ProjectedNarrowingNode {
                atom: node.atom,
                if_true: ProjectedNarrowingNodeId::ALWAYS_FALSE,
                if_uncertain: when_true,
                if_false: ProjectedNarrowingNodeId::ALWAYS_TRUE,
            });
        }

        if let Some(cached) = self.node_cache.get(&node) {
            return *cached;
        }

        let id = ProjectedNarrowingNodeId(self.nodes.len());
        self.nodes.push(node);
        self.node_cache.insert(node, id);
        id
    }

    /// Returns the projected nodes that join multiple incoming paths.
    ///
    /// Projection interns equivalent subgraphs into a DAG. Caching each join lets narrowing
    /// evaluate a shared suffix once and apply each incoming prefix constraint afterward.
    fn joins(&self, root: ProjectedNarrowingNodeId) -> Vec<bool> {
        let mut referenced = vec![false; self.nodes.len()];
        let mut joins = vec![false; self.nodes.len()];
        let mut visited = vec![false; self.nodes.len()];
        let mut pending = vec![root];

        while let Some(id) = pending.pop() {
            if id.is_terminal() || std::mem::replace(&mut visited[id.0], true) {
                continue;
            }

            let node = self.node(id);
            for next in [node.if_true, node.if_uncertain, node.if_false] {
                if !next.is_terminal() {
                    if std::mem::replace(&mut referenced[next.0], true) {
                        joins[next.0] = true;
                    }
                    pending.push(next);
                }
            }
        }

        joins
    }

    /// Combines two paths without copying one path into both outcomes of the other's predicate.
    fn or(
        &mut self,
        left: ProjectedNarrowingNodeId,
        right: ProjectedNarrowingNodeId,
    ) -> ProjectedNarrowingNodeId {
        if left == right || left == ProjectedNarrowingNodeId::ALWAYS_TRUE {
            return left;
        }
        if right == ProjectedNarrowingNodeId::ALWAYS_TRUE {
            return right;
        }
        if left == ProjectedNarrowingNodeId::ALWAYS_FALSE {
            return right;
        }
        if right == ProjectedNarrowingNodeId::ALWAYS_FALSE {
            return left;
        }

        let key = if left.0 <= right.0 {
            (left, right)
        } else {
            (right, left)
        };
        if let Some(cached) = self.or_cache.get(&key) {
            return *cached;
        }

        let left_node = self.node(left);
        let right_node = self.node(right);
        let result = match left_node.atom.cmp(&right_node.atom).reverse() {
            std::cmp::Ordering::Equal => {
                let if_true = self.or(left_node.if_true, right_node.if_true);
                let if_uncertain = self.or(left_node.if_uncertain, right_node.if_uncertain);
                let if_false = self.or(left_node.if_false, right_node.if_false);
                self.add_node(ProjectedNarrowingNode {
                    atom: left_node.atom,
                    if_true,
                    if_uncertain,
                    if_false,
                })
            }
            std::cmp::Ordering::Less => {
                let if_uncertain = self.or(left_node.if_uncertain, right);
                self.add_node(ProjectedNarrowingNode {
                    atom: left_node.atom,
                    if_true: left_node.if_true,
                    if_uncertain,
                    if_false: left_node.if_false,
                })
            }
            std::cmp::Ordering::Greater => {
                let if_uncertain = self.or(left, right_node.if_uncertain);
                self.add_node(ProjectedNarrowingNode {
                    atom: right_node.atom,
                    if_true: right_node.if_true,
                    if_uncertain,
                    if_false: right_node.if_false,
                })
            }
        };

        self.or_cache.insert(key, result);
        result
    }
}

/// Removes predicates that cannot narrow one place from a narrowing constraint.
struct NarrowingProjector<'a, 'db> {
    db: &'db dyn Db,
    constraints: &'a NarrowingConstraints,
    predicates: &'a IndexSlice<ScopedPredicateId, Predicate<'db>>,
    place: ScopedPlaceId,
    project_cache: FxHashMap<ScopedNarrowingConstraint, ProjectedNarrowingNodeId>,
    graph: ProjectedNarrowingGraph<'db>,
}

impl<'a, 'db> NarrowingProjector<'a, 'db> {
    /// Creates a projector for narrowing `place`.
    fn new(
        db: &'db dyn Db,
        constraints: &'a NarrowingConstraints,
        predicates: &'a IndexSlice<ScopedPredicateId, Predicate<'db>>,
        place: ScopedPlaceId,
    ) -> Self {
        Self {
            db,
            constraints,
            predicates,
            place,
            project_cache: FxHashMap::default(),
            graph: ProjectedNarrowingGraph::default(),
        }
    }

    /// Returns the cached positive and negative narrowing constraints for a predicate.
    fn predicate_constraints(
        &mut self,
        predicate_id: ScopedPredicateId,
    ) -> (
        Option<NarrowingConstraint<'db>>,
        Option<NarrowingConstraint<'db>>,
    ) {
        if let Some(cached) = self.graph.predicate_constraints_cache.get(&predicate_id) {
            return cached.clone();
        }

        let constraints =
            infer_narrowing_constraints(self.db, self.predicates[predicate_id], self.place);
        self.graph
            .predicate_constraints_cache
            .insert(predicate_id, constraints.clone());
        constraints
    }

    /// Projects one constraint node into the graph for this place.
    fn project(&mut self, root: ScopedNarrowingConstraint) -> ProjectedNarrowingNodeId {
        type Id = ScopedNarrowingConstraint;
        enum Action {
            Visit(Id),
            AnalyzeNonTerminal(Id),
            FinishNonTerminal { id: Id, branch: Id },
            FinishPredicate(Id),
        }

        let mut actions = SmallVec::<[Action; 8]>::new();
        actions.push(Action::Visit(root));

        while let Some(action) = actions.pop() {
            match action {
                Action::Visit(id) => {
                    if id.is_terminal() || self.project_cache.contains_key(&id) {
                        continue;
                    }

                    let node = self.constraints.get_interior_node(id);
                    let predicate = self.predicates[node.atom];

                    if matches!(predicate.node, PredicateNode::IsNonTerminalCall(_)) {
                        actions.push(Action::AnalyzeNonTerminal(id));
                        actions.push(Action::Visit(node.if_uncertain));
                    } else {
                        actions.push(Action::FinishPredicate(id));
                        actions.push(Action::Visit(node.if_false));
                        actions.push(Action::Visit(node.if_uncertain));
                        actions.push(Action::Visit(node.if_true));
                    }
                }
                Action::AnalyzeNonTerminal(id) => {
                    let node = self.constraints.get_interior_node(id);
                    let predicate = self.predicates[node.atom];
                    let branch = match analyze_single(self.db, &predicate) {
                        Truthiness::AlwaysTrue => node.if_true,
                        Truthiness::AlwaysFalse => node.if_false,
                        Truthiness::Ambiguous => {
                            unreachable!("`IsNonTerminalCall` predicates should never be Ambiguous")
                        }
                    };

                    actions.push(Action::FinishNonTerminal { id, branch });
                    actions.push(Action::Visit(branch));
                }
                Action::FinishNonTerminal { id, branch } => {
                    let node = self.constraints.get_interior_node(id);
                    let branch = self.projected_node(branch);
                    let if_uncertain = self.projected_node(node.if_uncertain);
                    let projected = self.graph.or(branch, if_uncertain);
                    self.project_cache.insert(id, projected);
                }
                Action::FinishPredicate(id) => {
                    let node = self.constraints.get_interior_node(id);
                    let if_true = self.projected_node(node.if_true);
                    let if_uncertain = self.projected_node(node.if_uncertain);
                    let if_false = self.projected_node(node.if_false);
                    let (pos_constraint, neg_constraint) = self.predicate_constraints(node.atom);

                    let projected = if pos_constraint.is_none() && neg_constraint.is_none() {
                        let either = self.graph.or(if_true, if_false);
                        self.graph.or(either, if_uncertain)
                    } else {
                        self.graph.add_node(ProjectedNarrowingNode {
                            atom: node.atom,
                            if_true,
                            if_uncertain,
                            if_false,
                        })
                    };
                    self.project_cache.insert(id, projected);
                }
            }
        }

        self.projected_node(root)
    }

    fn projected_node(&self, id: ScopedNarrowingConstraint) -> ProjectedNarrowingNodeId {
        match id {
            ScopedNarrowingConstraint::ALWAYS_TRUE => ProjectedNarrowingNodeId::ALWAYS_TRUE,
            ScopedNarrowingConstraint::ALWAYS_FALSE => ProjectedNarrowingNodeId::ALWAYS_FALSE,
            _ => self.project_cache[&id],
        }
    }
}

/// Evaluates narrowed types over a projected narrowing graph.
struct ProjectedNarrowingContext<'a, 'db> {
    db: &'db dyn Db,
    base_ty: Type<'db>,
    graph: &'a ProjectedNarrowingGraph<'db>,
    /// Marks join boundaries in the projected DAG.
    joins: Vec<bool>,
    /// Caches each join's narrowed suffix type from its boundary.
    join_cache: FxHashMap<ProjectedNarrowingNodeId, Type<'db>>,
}

impl<'db> ProjectedNarrowingContext<'_, 'db> {
    fn is_join(&self, id: ProjectedNarrowingNodeId) -> bool {
        !id.is_terminal() && self.joins[id.0]
    }

    /// Evaluates one projected join from its boundary and caches its narrowed suffix type.
    fn narrow_join(&mut self, id: ProjectedNarrowingNodeId) -> Type<'db> {
        if let Some(cached) = self.join_cache.get(&id) {
            return *cached;
        }

        let result = self.narrow_uncached(id, None);
        self.join_cache.insert(id, result);
        result
    }

    /// Recursively evaluates a projected path while accumulating narrowing constraints.
    fn narrow(
        &mut self,
        id: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
    ) -> Type<'db> {
        if self.is_join(id) {
            // Preserve replacement narrowing order at a join: evaluate the shared suffix once,
            // then apply the incoming prefix constraint to its narrowed type.
            let suffix_ty = self.narrow_join(id);
            return apply_accumulated_narrowing(self.db, suffix_ty, accumulated);
        }

        self.narrow_uncached(id, accumulated)
    }

    /// Recursively evaluates an unshared projected path while accumulating narrowing constraints.
    fn narrow_uncached(
        &mut self,
        id: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
    ) -> Type<'db> {
        if id == ProjectedNarrowingNodeId::ALWAYS_FALSE {
            return Type::Never;
        }

        if id == ProjectedNarrowingNodeId::ALWAYS_TRUE {
            apply_accumulated_narrowing(self.db, self.base_ty, accumulated)
        } else {
            let node = self.graph.node(id);
            let (pos_constraint, neg_constraint) =
                self.graph.predicate_constraints_cache[&node.atom].clone();

            if node.if_true == ProjectedNarrowingNodeId::ALWAYS_FALSE
                && node.if_uncertain == ProjectedNarrowingNodeId::ALWAYS_FALSE
            {
                let false_accumulated = accumulate_constraint(accumulated, neg_constraint);
                self.narrow(node.if_false, false_accumulated)
            } else if node.if_false == ProjectedNarrowingNodeId::ALWAYS_FALSE
                && node.if_uncertain == ProjectedNarrowingNodeId::ALWAYS_FALSE
            {
                let true_accumulated = accumulate_constraint(accumulated, pos_constraint);
                self.narrow(node.if_true, true_accumulated)
            } else {
                let true_accumulated = accumulate_constraint(accumulated.clone(), pos_constraint);
                let true_ty = self.narrow(node.if_true, true_accumulated);

                let uncertain_ty = self.narrow(node.if_uncertain, accumulated.clone());

                let false_accumulated = accumulate_constraint(accumulated, neg_constraint);
                let false_ty = self.narrow(node.if_false, false_accumulated);

                let true_or_uncertain =
                    UnionType::from_two_elements(self.db, true_ty, uncertain_ty);
                UnionType::from_two_elements(self.db, true_or_uncertain, false_ty)
            }
        }
    }
}

fn analyze_single_pattern_predicate_kind<'db>(
    db: &'db dyn Db,
    predicate_kind: &PatternPredicateKind<'db>,
    subject_ty: Type<'db>,
    precomputed_definite_match_ty: Option<Type<'db>>,
) -> Truthiness {
    match predicate_kind {
        PatternPredicateKind::Value(value) => {
            let value_ty = infer_same_file_expression_type(db, *value, TypeContext::default());

            equality_truthiness(
                db,
                subject_ty,
                value_ty,
                ComparisonSoundnessPolicy::from_analysis_settings(
                    db.analysis_settings(value.file(db)),
                ),
            )
        }
        PatternPredicateKind::Singleton(singleton) => {
            let singleton_ty = singleton_pattern_type(db, *singleton);

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

            let mut remaining_subject_ty = subject_ty;
            let (ControlFlow::Break(truthiness) | ControlFlow::Continue(truthiness)) = predicates
                .iter()
                .map(|p| {
                    let narrowed_subject_ty = remaining_subject_ty;

                    let definitely_matched =
                        definite_match_pattern_type_for_subject(db, p, narrowed_subject_ty);

                    let truthiness = if narrowed_subject_ty.is_subtype_of(db, definitely_matched) {
                        Truthiness::AlwaysTrue
                    } else {
                        analyze_single_pattern_predicate_kind(
                            db,
                            p,
                            narrowed_subject_ty,
                            Some(definitely_matched),
                        )
                    };

                    remaining_subject_ty =
                        pattern_binding_fallthrough_type(db, p, narrowed_subject_ty);
                    truthiness
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
        PatternPredicateKind::Class(kind) => {
            let class_ty =
                match infer_same_file_expression_type(db, kind.class, TypeContext::default()) {
                    Type::ClassLiteral(class) => Type::instance(db, class.top_materialization(db)),
                    Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) => {
                        callable_pattern_type(db)
                    }
                    _ => return Truthiness::Ambiguous,
                };
            let definitely_matched = precomputed_definite_match_ty.unwrap_or_else(|| {
                definite_match_pattern_type_for_subject(db, predicate_kind, subject_ty)
            });

            if subject_ty.is_equivalent_to(db, definitely_matched)
                || subject_ty.is_subtype_of(db, definitely_matched)
            {
                Truthiness::AlwaysTrue
            } else if subject_ty.is_disjoint_from(db, class_ty) {
                Truthiness::AlwaysFalse
            } else {
                Truthiness::Ambiguous
            }
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
        PatternPredicateKind::Sequence(kind) => {
            let sequence_ty = sequence_pattern_type_builder(db).build();
            if subject_ty.is_subtype_of(db, sequence_ty) {
                if kind.is_irrefutable() {
                    Truthiness::AlwaysTrue
                } else {
                    Truthiness::Ambiguous
                }
            } else if subject_ty.is_disjoint_from(db, sequence_ty) {
                Truthiness::AlwaysFalse
            } else {
                Truthiness::Ambiguous
            }
        }
        PatternPredicateKind::As(pattern, _) => pattern
            .as_deref()
            .map(|p| {
                analyze_single_pattern_predicate_kind(
                    db,
                    p,
                    subject_ty,
                    precomputed_definite_match_ty,
                )
            })
            .unwrap_or(Truthiness::AlwaysTrue),
        PatternPredicateKind::Star(_) => Truthiness::AlwaysTrue,
    }
}

/// Determines whether a statement-level call can return.
///
/// Only a call known to return `Never` is treated as terminal. Unsupported or uncertain callable
/// forms are conservatively treated as returning so that subsequent code remains reachable.
///
/// Cycle recovery conservatively treats the call as returning so that a cyclic type inference
/// dependency cannot make subsequent code unreachable.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _, _| Truthiness::AlwaysTrue,
    heap_size = get_size2::GetSize::get_heap_size
)]
fn analyze_non_terminal_call<'db>(
    db: &'db dyn Db,
    callable: Expression<'db>,
    call_expr: Expression<'db>,
    is_await: bool,
) -> Truthiness {
    // We first infer just the type of the callable. In the most likely case that the function is
    // not marked with `NoReturn`, or that it always returns `NoReturn`, doing so allows us to avoid
    // the more expensive work of inferring the entire call expression (which could involve
    // inferring argument types to possibly run the overload selection algorithm). Avoiding this on
    // the happy path is important because these constraints can be very large in number, since we
    // add them on all statement-level function calls.
    let ty = infer_same_file_expression_type(db, callable, TypeContext::default());

    // Short-circuit for well-known types that are known not to return `Never` when called. Without
    // the short-circuit, we've seen that threads keep blocking each other because they all try to
    // acquire Salsa's `CallableType` lock that ensures each type is only interned once. The lock is
    // so heavily congested because there are only very few dynamic types, in which case Salsa's
    // sharding the locks by value doesn't help much. See <https://github.com/astral-sh/ty/issues/968>.
    if matches!(ty, Type::Dynamic(_)) {
        return Truthiness::AlwaysTrue;
    }

    let overloads_iterator = if let Some(callable) = ty
        .try_upcast_to_callable(db)
        .and_then(CallableTypes::exactly_one)
    {
        callable.signatures(db).overloads.iter()
    } else {
        return Truthiness::AlwaysTrue;
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
        let call_expr_ty = infer_same_file_expression_type(db, call_expr, TypeContext::default());
        if call_expr_ty.is_equivalent_to(db, Type::Never) {
            Truthiness::AlwaysFalse
        } else {
            Truthiness::AlwaysTrue
        }
    }
}

fn analyze_non_empty_iterable(db: &dyn Db, iterable: Expression) -> Truthiness {
    match infer_same_file_expression_type(db, iterable, TypeContext::default()) {
        Type::KnownInstance(KnownInstanceType::Range { is_non_empty }) => {
            Truthiness::from(is_non_empty)
        }
        _ => Truthiness::Ambiguous,
    }
}

fn analyze_single(db: &dyn Db, predicate: &Predicate) -> Truthiness {
    let _span = tracing::trace_span!("analyze_single", ?predicate).entered();

    match predicate.node {
        PredicateNode::Expression(test_expr) => {
            infer_same_file_expression_type(db, test_expr, TypeContext::default())
                .bool(db)
                .negate_if(!predicate.is_positive)
        }
        PredicateNode::IsNonTerminalCall(CallableAndCallExpr {
            callable,
            call_expr,
            is_await,
        }) => analyze_non_terminal_call(db, callable, call_expr, is_await)
            .negate_if(!predicate.is_positive),
        PredicateNode::Pattern(inner) => analyze_pattern_predicate(db, inner),
        PredicateNode::SubjectElementPattern(subject_element) => {
            analyze_pattern_predicate(db, subject_element.pattern)
        }
        PredicateNode::IsNonEmptyIterable(iterable) => {
            analyze_non_empty_iterable(db, iterable).negate_if(!predicate.is_positive)
        }
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
            .place_and_qualifiers
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
    evaluate_reachability(db, use_def, binding.reachability_constraint)
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

/// Inference-local cache for static reachability evaluations.
///
/// Place lookup may evaluate the same reachability constraint many times while inferring a single
/// region: once for declarations, again for bindings, and again while recursively looking up
/// related places. Those evaluations can in turn infer predicate truthiness, so reusing the result
/// avoids repeating non-trivial work.
///
/// The common case is evaluating constraints from the inferred region's own use-def map. Those
/// entries are stored in a dense vector indexed by [`ScopedReachabilityConstraintId`]. Constraints
/// from other use-def maps are less common and are stored separately, keyed by the address of their
/// [`ReachabilityConstraints`] graph plus the local constraint id. The graph address is part of the
/// key because scoped constraint ids are only unique within one graph.
pub(crate) struct ReachabilityEvaluationCache<'db> {
    primary_scope: ScopeId<'db>,
    primary_constraints: usize,
    primary_entries: RefCell<Vec<Option<Truthiness>>>,
    other_entries: RefCell<FxHashMap<(usize, ScopedReachabilityConstraintId), Truthiness>>,
}

impl<'db> ReachabilityEvaluationCache<'db> {
    /// Creates a cache optimized for the use-def map of `primary_scope`.
    ///
    /// `primary_constraints` must be the reachability graph for `primary_scope`'s use-def map. The
    /// cache uses this graph's address to decide whether an evaluation can use the dense primary
    /// storage or must fall back to the secondary map for another graph.
    pub(crate) fn new(
        primary_scope: ScopeId<'db>,
        primary_constraints: &ReachabilityConstraints,
    ) -> Self {
        Self {
            primary_scope,
            primary_constraints: std::ptr::from_ref(primary_constraints).addr(),
            primary_entries: RefCell::new(Vec::new()),
            other_entries: RefCell::new(FxHashMap::default()),
        }
    }

    /// Evaluates `id`, reusing a cached result when possible.
    ///
    /// Trivial constraint ids return immediately and are not stored. For interior nodes, the
    /// predicate determines whether the constraint belongs to the primary scope. A primary-scope
    /// constraint from the primary graph is cached by dense index; all other constraints are cached
    /// by graph identity and id.
    pub(crate) fn evaluate(
        &self,
        db: &'db dyn Db,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedReachabilityConstraintId,
    ) -> Truthiness {
        match id {
            ScopedReachabilityConstraintId::ALWAYS_TRUE => return Truthiness::AlwaysTrue,
            ScopedReachabilityConstraintId::ALWAYS_FALSE => return Truthiness::AlwaysFalse,
            ScopedReachabilityConstraintId::AMBIGUOUS => return Truthiness::Ambiguous,
            _ => {}
        }

        let predicate = predicates[constraints.get_interior_node(id).atom()];
        let constraints_key = std::ptr::from_ref(constraints).addr();
        let scope = predicate_scope(db, &predicate);

        if scope != self.primary_scope || constraints_key != self.primary_constraints {
            let key = (constraints_key, id);
            if let Some(result) = self.other_entries.borrow().get(&key).copied() {
                return result;
            }

            let result = evaluate_reachability_constraint(db, scope, id);
            self.other_entries.borrow_mut().insert(key, result);
            return result;
        }

        let index = id.index();
        if let Some(result) = self.primary_entries.borrow().get(index).copied().flatten() {
            return result;
        }

        let result = evaluate_reachability_constraint(db, self.primary_scope, id);
        let mut entries = self.primary_entries.borrow_mut();
        if entries.len() <= index {
            entries.resize(index + 1, None);
        }
        entries[index] = Some(result);
        result
    }
}

/// Evaluates a reachability constraint, optionally using an inference-local cache.
pub(crate) fn evaluate_reachability_with_cache<'db>(
    db: &'db dyn Db,
    cache: Option<&ReachabilityEvaluationCache<'db>>,
    constraints: &ReachabilityConstraints,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    id: ScopedReachabilityConstraintId,
) -> Truthiness {
    if let Some(cache) = cache {
        cache.evaluate(db, constraints, predicates, id)
    } else {
        constraints.evaluate(db, predicates, id)
    }
}

pub(crate) trait DeclarationsIteratorExtension<'db> {
    fn any_reachable(
        self,
        db: &'db dyn Db,
        predicate: impl FnMut(DefinitionState<'db>) -> bool,
    ) -> bool;

    /// Return the first reachable declaration that matches the passed in predicate function.
    fn first_reachable_declaration_order(
        self,
        db: &'db dyn Db,
        predicate: impl FnMut(DefinitionState<'db>) -> bool,
    ) -> Option<ScopedDefinitionId>;
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
                 ..
             }| {
                predicate(declaration)
                    && !reachability_constraints
                        .evaluate(db, predicates, reachability_constraint)
                        .is_always_false()
            },
        )
    }

    fn first_reachable_declaration_order(
        mut self,
        db: &'db dyn Db,
        mut predicate: impl FnMut(DefinitionState<'db>) -> bool,
    ) -> Option<ScopedDefinitionId> {
        let reachability_predicates = self.predicates();
        let reachability_constraints = self.reachability_constraints();

        self.find_map(
            |DeclarationWithConstraint {
                 declaration,
                 declaration_order,
                 reachability_constraint,
             }| {
                (predicate(declaration)
                    && !reachability_constraints
                        .evaluate(db, reachability_predicates, reachability_constraint)
                        .is_always_false())
                .then_some(declaration_order)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ty_python_core::narrowing_constraints::InteriorNode;
    use ty_python_core::predicate::Predicates;
    use ty_python_core::semantic_index;

    #[test]
    fn deep_constraint_projection_does_not_overflow() -> anyhow::Result<()> {
        const DEPTH: usize = 100_000;

        let handle = std::thread::Builder::new()
            .name("deep-narrowing-projection".into())
            .stack_size(ruff_db::STACK_SIZE)
            .spawn(|| -> anyhow::Result<()> {
                let mut db = setup_db();
                db.write_dedented(
                    "/src/test.py",
                    r#"
                    def f(x: int, flag: bool) -> None:
                        if flag:
                            y = x
                    "#,
                )?;

                let file = system_path_to_file(&db, "/src/test.py").unwrap();
                let index = semantic_index(&db, file);
                let function_scope = index.child_scopes(FileScopeId::global()).next().unwrap().0;
                let use_def = index.use_def_map(function_scope);
                let predicate = use_def
                    .predicates()
                    .iter()
                    .find(|predicate| matches!(predicate.node, PredicateNode::Expression(_)))
                    .unwrap();
                let predicates: Predicates = std::iter::repeat_n(*predicate, DEPTH).collect();

                // Build `p99_999 or ... or p0`. Each predicate concerns `flag`, so projecting the
                // graph for `x` removes every interior node and leaves `ALWAYS_TRUE`.
                let nodes = (0..DEPTH)
                    .map(|index| InteriorNode {
                        atom: ScopedPredicateId::new(index),
                        if_true: ScopedNarrowingConstraint::ALWAYS_TRUE,
                        if_uncertain: if index == 0 {
                            ScopedNarrowingConstraint::ALWAYS_FALSE
                        } else {
                            ScopedNarrowingConstraint::new(index - 1)
                        },
                        if_false: ScopedNarrowingConstraint::ALWAYS_FALSE,
                    })
                    .collect();
                let constraints = NarrowingConstraints::from_test_nodes(nodes);
                let x = index.place_table(function_scope).symbol_id("x").unwrap();
                let mut projector = NarrowingProjector::new(
                    &db,
                    &constraints,
                    &predicates,
                    ScopedPlaceId::Symbol(x),
                );

                assert_eq!(
                    projector.project(ScopedNarrowingConstraint::new(DEPTH - 1)),
                    ProjectedNarrowingNodeId::ALWAYS_TRUE
                );
                Ok(())
            })?;

        handle.join().expect("projection thread panicked")
    }
}
