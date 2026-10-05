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

use crate::ProgramEnvironment;
use std::cell::RefCell;

use crate::{
    Db,
    types::{
        CallableType, ComparisonSoundnessPolicy, EnumClassLiteral, KnownInstanceType,
        NarrowingConstraint, SpecialFormType, Type, TypeContext, UnionType,
        definite_match_pattern_type, definite_match_pattern_type_for_subject, equality_truthiness,
        expand_type, infer_expression_types, infer_same_file_expression_type, mapping_pattern_type,
        pattern_binding_fallthrough_type, sequence_pattern_type_builder, singleton_pattern_type,
    },
};
use ruff_db::parsed::parsed_module;
use ruff_index::{Idx, IndexSlice};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::function::IngredientImpl;
use ty_python_core::{
    BindingWithConstraints, DeclarationWithConstraint, DeclarationsIterator, EvaluationMode,
    FileScopeId, NarrowingEvaluator, PredicateNarrowingTargets, ScopedDefinitionId, SemanticIndex,
    Truthiness, UseDefMap,
    definition::DefinitionState,
    expression::Expression,
    narrowing_constraints::{NarrowingConstraints, ScopedNarrowingConstraint},
    place::ScopedPlaceId,
    predicate::{
        CallableAndCallExpr, PatternPredicate, PatternPredicateKind, Predicate, PredicateNode,
        ScopedPredicateId, StarImportPlaceholderPredicate,
    },
    reachability_constraints::{ReachabilityConstraints, ScopedReachabilityConstraintId},
    scope::ScopeId,
    use_def_map,
};

pub(crate) mod narrowing_construction;
pub(crate) mod narrowing_entry;
pub(crate) mod narrowing_evaluation;
pub(crate) mod narrowing_predicate;
pub(crate) mod range;
pub(crate) mod source;
pub(crate) mod star_import;

#[cfg(test)]
mod cache_tests;

use source::{OrdinaryReachabilityEffects, ReachabilityFacts};

/// Narrow `subject_ty` by all preceding unguarded match patterns.
///
/// Caching each prefix lets the next case reuse the already-normalized subject instead of
/// rebuilding it from the union of all preceding patterns, which can repeatedly distribute the
/// same intersections.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, id, _, _| Type::divergent(id),
    cycle_fn = |db: &'db dyn Db, cycle, previous: &Type<'db>, result: Type<'db>, predicate: PatternPredicate<'db>, _| {
        let env = ProgramEnvironment::from_scope(predicate.subject(db).scope(db));
        result.cycle_normalized(db, &env, *previous, cycle)
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
    cycle_fn = |db: &'db dyn Db, cycle, previous: &Type<'db>, result: Type<'db>, predicate: PatternPredicate<'db>, _| {
        let env = ProgramEnvironment::from_scope(predicate.subject(db).scope(db));
        result.cycle_normalized(db, &env, *previous, cycle)
    },
    heap_size = ruff_memory_usage::heap_size
)]
fn type_narrowed_by_pattern<'db>(
    db: &'db dyn Db,
    predicate: PatternPredicate<'db>,
    subject_ty: Type<'db>,
) -> Type<'db> {
    let env = ProgramEnvironment::from_file(predicate.program_file(db));
    pattern_binding_fallthrough_type(db, &env, predicate.kind(db), subject_ty)
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
        Type::TypeAlias(alias) => {
            return enum_literal_subject_names(db, alias.value_type(db));
        }
        _ => return None,
    }

    Some((enum_class?, names))
}

/// Return the canonical enum-member name matched by a single value pattern.
///
/// This recognizes patterns like `case Color.RED:` only when the pattern expression is
/// an enum member belonging to the expected enum class. Enum aliases are resolved to their
/// canonical member names before returning.
fn enum_member_pattern_name<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    enum_class: EnumClassLiteral<'db>,
    kind: &PatternPredicateKind<'db>,
) -> Option<Name> {
    let value_ty = definite_match_pattern_type(db, env, kind);
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
    env: &ProgramEnvironment<'db>,
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
                let alt_coverage = enum_member_pattern_coverage(db, env, enum_class, alt);
                coverage
                    .definitely_matched
                    .extend(alt_coverage.definitely_matched);
                coverage.is_exact &= alt_coverage.is_exact;
            }
        }
        PatternPredicateKind::As(Some(inner), _) => {
            return enum_member_pattern_coverage(db, env, enum_class, inner);
        }
        _ => {
            if let Some(name) = enum_member_pattern_name(db, env, enum_class, kind) {
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
    env: &ProgramEnvironment<'db>,
    predicate: PatternPredicate<'db>,
    subject_ty: Type<'db>,
) -> Option<Truthiness> {
    let (enum_class, mut remaining_names) = enum_literal_subject_names(db, subject_ty)?;
    let current_coverage = enum_member_pattern_coverage(db, env, enum_class, predicate.kind(db));
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
            enum_member_pattern_coverage(db, env, enum_class, previous_predicate.kind(db));
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
    let env = ProgramEnvironment::from_scope(predicate.subject(db).scope(db));
    let subject_ty =
        infer_same_file_expression_type(db, predicate.subject(db), TypeContext::default());

    if let Some(truthiness) =
        analyze_enum_literal_union_pattern_predicate(db, &env, predicate, subject_ty)
    {
        return truthiness;
    }

    let coverage_subject_ty = expand_type(db, &env, subject_ty)
        .map(|types| UnionType::from_elements(db, &env, types))
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

    let truthiness = analyze_single_pattern_predicate_kind(
        db,
        &env,
        predicate.kind(db),
        narrowed_subject_ty,
        None,
    );

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

const NON_TERMINAL_CALL_CHUNK_SIZE: usize = 16;
const REACHABILITY_EVALUATION_CHUNK_SIZE: usize = 256;
const CONTROL_FLOW_REACHABILITY_CHECKPOINT_INTERVAL: usize = 16;
const NARROWING_EVALUATION_CHECKPOINT_INTERVAL: usize = 8;
fn predicate_scope<'db>(db: &'db dyn Db, predicate: &Predicate<'db>) -> ScopeId<'db> {
    match predicate.node {
        PredicateNode::Expression(expression)
        | PredicateNode::Condition(expression)
        | PredicateNode::ChainedComparisonCondition(expression)
        | PredicateNode::ContextManagerSuppresses { expression, .. } => expression.scope(db),
        PredicateNode::IsNonTerminalCall(call) => call.callable(db).scope(db),
        PredicateNode::Pattern(pattern) => pattern.scope(db),
        PredicateNode::FinallyNormalPathImpossible { scope, .. } => scope,
        PredicateNode::OrPatternAlternative(scope) => scope,
        PredicateNode::SubjectElementPattern(subject_element) => subject_element.pattern.scope(db),
        PredicateNode::IsNonEmptyIterable(expression) => expression.scope(db),
        PredicateNode::StarImportPlaceholder(star_import) => star_import.scope(db),
    }
}

/// Infers complete preceding blocks of call predicates in source order.
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
/// Reentrant analysis is handled by Salsa cycle recovery on the cached-range queries. The final
/// incomplete block is left for the reachability walk: it can add at most 15 nested call queries,
/// and analyzing it eagerly would bypass the range query's cycle recovery and could introduce a
/// divergent inference cycle. For large scopes, keeping the complete-block pass unconditional
/// ensures that tracked callers record the same dependencies on every thread. Small scopes do not
/// need prefix warming to bound the Salsa stack, so their calls are evaluated entirely on demand.
fn analyze_non_terminal_call_prefix<'db>(
    db: &'db dyn Db,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    root_predicate: ScopedPredicateId,
) -> bool {
    source::infallible(source::analyze_non_terminal_call_prefix_sync(
        predicates,
        root_predicate,
        ReachabilityFacts,
        &OrdinaryReachabilityEffects::new(db),
    ))
}

fn analyze_large_non_terminal_call_prefix<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    root_predicate: ScopedPredicateId,
) -> bool {
    let call_predicates = non_terminal_call_predicates(db, scope);
    let call_count = call_predicates.partition_point(|predicate| *predicate <= root_predicate);
    let mut start = 0;
    // Leave the incomplete final block demand-driven. Its reverse dependency chain is bounded by
    // the block size, and every eagerly analyzed call remains behind a recoverable range query.
    let mut remaining = call_count / NON_TERMINAL_CALL_CHUNK_SIZE;
    while remaining > 0 {
        let level = remaining.ilog2();
        let length = 1 << level;
        analyze_non_terminal_call_range(db, scope, level, start >> level);
        start += length;
        remaining -= length;
    }

    true
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
    env: &ProgramEnvironment<'db>,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    call_predicates: &[ScopedPredicateId],
) {
    for id in call_predicates {
        analyze_single(db, env, &predicates[*id]);
    }
}

/// Analyzes a power-of-two range of call-predicate blocks in source order.
///
/// Prefixes can be decomposed into these canonical ranges and reused by later expression-inference
/// queries. Splitting ranges in half keeps the Salsa query stack logarithmic even when the first
/// requested prefix contains thousands of calls. Each leaf handles multiple calls iteratively to
/// avoid retaining a Salsa argument and query result for every individual predicate.
///
/// Analyzing a call can re-enter reachability through expression inference and request this same
/// range. Recovery is a no-op because the range only warms call queries; any call still needed for
/// reachability is evaluated directly by the decision-diagram walk.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _, _| (),
    heap_size = get_size2::GetSize::get_heap_size
)]
fn analyze_non_terminal_call_range<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    level: u32,
    index: usize,
) {
    if level == 0 {
        let env = ProgramEnvironment::from_scope(scope);
        let use_def = use_def_map(db, scope);
        let call_predicates = non_terminal_call_predicates(db, scope);
        let start = index * NON_TERMINAL_CALL_CHUNK_SIZE;
        let end = start + NON_TERMINAL_CALL_CHUNK_SIZE;
        analyze_non_terminal_calls(db, &env, use_def.predicates(), &call_predicates[start..end]);
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

/// Evaluates the normal continuation captured by a deferred `finally` predicate.
///
/// Unlike other reachability predicates, a deferred `finally` predicate recursively evaluates
/// another reachability constraint, which may contain earlier deferred `finally` predicates.
/// Caching these continuations prevents a sequence of `finally` suites from repeatedly evaluating
/// all preceding continuations, which would otherwise take exponential time.
///
/// Other expensive predicates already use tracked queries, while ordinary reachability
/// constraints are cached within each inference region and at sparse checkpoints. Tracking
/// [`evaluate_reachability_constraint`] itself would instead retain a Salsa query key and memo for
/// every constraint.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _| Truthiness::Ambiguous,
    heap_size = get_size2::GetSize::get_heap_size
)]
fn evaluate_finally_continuation<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    continuation: ScopedReachabilityConstraintId,
) -> Truthiness {
    evaluate_reachability_constraint(db, scope, continuation)
}

fn terminal_reachability(id: ScopedReachabilityConstraintId) -> Option<Truthiness> {
    match id {
        ScopedReachabilityConstraintId::ALWAYS_TRUE => Some(Truthiness::AlwaysTrue),
        ScopedReachabilityConstraintId::AMBIGUOUS => Some(Truthiness::Ambiguous),
        ScopedReachabilityConstraintId::ALWAYS_FALSE => Some(Truthiness::AlwaysFalse),
        _ => None,
    }
}

/// Selects sparse, stable checkpoints without adding a second scope-wide predicate index.
///
/// Statement calls retain their existing checkpoint spacing. Other control-flow predicates become
/// checkpoints only after a sufficiently long path has demonstrated that reuse is worthwhile.
fn is_reachability_checkpoint(
    call_predicates: Option<&[ScopedPredicateId]>,
    predicate: ScopedPredicateId,
    visited: usize,
) -> bool {
    if let Some(call_index) = call_predicates.and_then(|calls| calls.binary_search(&predicate).ok())
    {
        return (call_index + 1).is_multiple_of(REACHABILITY_EVALUATION_CHUNK_SIZE);
    }

    // Folding the adjacent bucket prevents regularly interleaved predicate kinds from always
    // missing the same checkpoint positions.
    let index = predicate.index();
    let checkpoint_position = index ^ (index / CONTROL_FLOW_REACHABILITY_CHECKPOINT_INTERVAL);
    visited >= CONTROL_FLOW_REACHABILITY_CHECKPOINT_INTERVAL
        && (checkpoint_position + 1).is_multiple_of(CONTROL_FLOW_REACHABILITY_CHECKPOINT_INTERVAL)
}

/// Walks a reachability decision diagram until it reaches a terminal or reusable checkpoint.
///
/// `use_checkpoint` is false only when entering from a checkpoint query. In that case, the first
/// node is evaluated directly to prevent the query from immediately calling itself again.
///
/// General checkpoints are created only after traversing a genuinely long path. Their positions
/// depend on stable predicate IDs, so adjacent roots reuse the same suffix without requiring an
/// additional retained scope-wide index or allocating tracked queries for short, ordinary paths.
fn evaluate_reachability_path<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    constraints: &ReachabilityConstraints,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    call_predicates: Option<&[ScopedPredicateId]>,
    id: ScopedReachabilityConstraintId,
    use_checkpoint: bool,
) -> Truthiness {
    source::infallible(source::evaluate_reachability_path_sync(
        scope,
        constraints,
        predicates,
        call_predicates,
        id,
        use_checkpoint,
        ReachabilityFacts,
        &OrdinaryReachabilityEffects::new(db),
    ))
}

/// Evaluates a canonical suffix of a reachability decision diagram.
///
/// Statement calls retain their existing sparse checkpoints; other predicates become checkpoints
/// only after a long path demonstrates that reuse is worthwhile. This lets later statements reuse
/// constraints accumulated by earlier statements without retaining an additional scope-wide index
/// or a Salsa query key and memo for every constraint.
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
    let predicates = use_def.predicates();
    let has_many_calls = predicates
        .iter()
        .filter(|predicate| matches!(predicate.node, PredicateNode::IsNonTerminalCall(_)))
        .nth(NON_TERMINAL_CALL_CHUNK_SIZE)
        .is_some();
    let call_predicates = has_many_calls.then(|| non_terminal_call_predicates(db, scope));
    evaluate_reachability_path(
        db,
        scope,
        use_def.reachability_constraints(),
        predicates,
        call_predicates,
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
        source::infallible(source::evaluate_reachability_sync(
            self,
            predicates,
            id,
            ReachabilityFacts,
            &OrdinaryReachabilityEffects::new(db),
        ))
    }
}

pub(crate) fn narrow_type_by_constraint<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    evaluator: &NarrowingEvaluator<'_, 'db>,
    base_ty: Type<'db>,
    place: ScopedPlaceId,
) -> Type<'db> {
    source::infallible(narrowing_entry::narrow_type_by_constraint_sync(
        env,
        evaluator,
        base_ty,
        place,
        narrowing_entry::NarrowingEntryFacts,
        &narrowing_entry::OrdinaryNarrowingEntryEffects::new(db),
    ))
}

fn apply_accumulated_narrowing<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    base_ty: Type<'db>,
    accumulated: Option<NarrowingConstraint<'db>>,
) -> Type<'db> {
    match accumulated {
        Some(constraint) => NarrowingConstraint::intersection(base_ty)
            .merge_constraint_and(constraint)
            .evaluate_constraint_type(db, env),
        None => base_ty,
    }
}

/// Identifier for a node in a projected narrowing graph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProjectedNarrowingNodeId(pub(crate) usize);

impl ProjectedNarrowingNodeId {
    /// Terminal node for paths that remain reachable.
    pub(crate) const ALWAYS_TRUE: Self = Self(usize::MAX);
    /// Terminal node for paths that are statically unreachable.
    pub(crate) const ALWAYS_FALSE: Self = Self(usize::MAX - 1);

    pub(crate) fn is_terminal(self) -> bool {
        self == Self::ALWAYS_TRUE || self == Self::ALWAYS_FALSE
    }
}

/// Interior node in a projected narrowing graph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProjectedNarrowingNode {
    pub(crate) atom: ScopedPredicateId,
    pub(crate) if_true: ProjectedNarrowingNodeId,
    pub(crate) if_uncertain: ProjectedNarrowingNodeId,
    pub(crate) if_false: ProjectedNarrowingNodeId,
}

/// A projected predicate or a suffix whose projection can be deferred until it is needed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProjectedNarrowingEntry<'db> {
    Predicate(ProjectedNarrowingNode),
    /// A nonterminal suffix. Constant suffixes use the graph's existing terminal IDs instead.
    Checkpoint {
        constraint: ScopedNarrowingConstraint,
        ty: Type<'db>,
    },
}

/// Narrowing graph containing only predicates that can narrow one place.
#[derive(Default)]
pub(crate) struct ProjectedNarrowingGraph<'db> {
    pub(crate) nodes: Vec<ProjectedNarrowingEntry<'db>>,
    pub(crate) referenced: Vec<bool>,
    pub(crate) joins: Vec<bool>,
    pub(crate) node_cache: FxHashMap<ProjectedNarrowingNode, ProjectedNarrowingNodeId>,
    pub(crate) or_cache:
        FxHashMap<(ProjectedNarrowingNodeId, ProjectedNarrowingNodeId), ProjectedNarrowingNodeId>,
    pub(crate) predicate_constraints_cache: FxHashMap<
        ScopedPredicateId,
        (
            Option<NarrowingConstraint<'db>>,
            Option<NarrowingConstraint<'db>>,
        ),
    >,
}

impl<'db> ProjectedNarrowingGraph<'db> {
    /// Returns an interior projected node by ID.
    pub(crate) fn node(&self, id: ProjectedNarrowingNodeId) -> ProjectedNarrowingEntry<'db> {
        self.nodes[id.0]
    }

    /// Marks a projected node as shared once multiple paths or binding roots reach it.
    pub(crate) fn record_reference(&mut self, id: ProjectedNarrowingNodeId) {
        if !id.is_terminal() && std::mem::replace(&mut self.referenced[id.0], true) {
            self.joins[id.0] = true;
        }
    }
}

/// A cached type together with the terminal shape of its canonical projected graph.
///
/// Joins need to recognize an unconstrained suffix before applying `TypeGuard` replacement.
/// Similarly, an unreachable graph must be eliminated before a later predicate can replace `Never`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ProjectedNarrowingCheckpoint<'db> {
    Unreachable,
    Unconstrained,
    Narrowed(Type<'db>),
}

impl<'db> ProjectedNarrowingCheckpoint<'db> {
    fn ty(self, base_ty: Type<'db>) -> Type<'db> {
        match self {
            Self::Unreachable => Type::Never,
            Self::Unconstrained => base_ty,
            Self::Narrowed(ty) => ty,
        }
    }
}

/// Evaluates a stable suffix with the canonical projected-graph evaluator.
///
/// The root is projected directly to avoid querying its own checkpoint. Descendant checkpoints
/// contribute their cached types and terminal shape. Nonterminal suffixes are expanded locally only
/// when simplifying a join requires their predicates.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, id, _, _, _, _| ProjectedNarrowingCheckpoint::Narrowed(Type::divergent(id)),
    cycle_fn = |db: &'db dyn Db, cycle, previous: &ProjectedNarrowingCheckpoint<'db>, result: ProjectedNarrowingCheckpoint<'db>, scope: ScopeId<'db>, _, _, base_ty| {
        match result {
            ProjectedNarrowingCheckpoint::Narrowed(ty) => ProjectedNarrowingCheckpoint::Narrowed(
                ty.cycle_normalized(db, &ProgramEnvironment::from_scope(scope), previous.ty(base_ty), cycle)
            ),
            _ => result,
        }
    },
    heap_size = get_size2::GetSize::get_heap_size
)]
fn evaluate_projected_narrowing_checkpoint<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    place: ScopedPlaceId,
    constraint: ScopedNarrowingConstraint,
    base_ty: Type<'db>,
) -> ProjectedNarrowingCheckpoint<'db> {
    let env = ProgramEnvironment::from_scope(scope);
    let use_def = use_def_map(db, scope);
    let evaluator = use_def.narrowing_evaluator(constraint);
    let mut projector = NarrowingProjector::new(
        db,
        &env,
        evaluator.narrowing_constraints(),
        use_def.predicates(),
        evaluator.predicate_narrowing_targets(),
        place,
        base_ty,
    );
    let root = projector.project(constraint, false);
    match root {
        ProjectedNarrowingNodeId::ALWAYS_FALSE => ProjectedNarrowingCheckpoint::Unreachable,
        ProjectedNarrowingNodeId::ALWAYS_TRUE => ProjectedNarrowingCheckpoint::Unconstrained,
        _ => ProjectedNarrowingCheckpoint::Narrowed(projector.narrow_projected(root, base_ty)),
    }
}

/// Narrows bindings of one place while reusing their shared constraint suffixes.
pub(crate) struct NarrowingProjector<'a, 'db> {
    db: &'db dyn Db,
    pub(crate) env: &'a ProgramEnvironment<'db>,
    pub(crate) constraints: &'a NarrowingConstraints,
    pub(crate) predicates: &'a IndexSlice<ScopedPredicateId, Predicate<'db>>,
    pub(crate) predicate_narrowing_targets: &'a PredicateNarrowingTargets,
    pub(crate) place: ScopedPlaceId,
    pub(crate) base_ty: Type<'db>,
    /// Checkpoint entries retain narrowed types, so projections are specific to the binding type.
    pub(crate) project_cache:
        FxHashMap<(ScopedNarrowingConstraint, Type<'db>), ProjectedNarrowingNodeId>,
    /// High-water backing bound for controlled inserts and removals.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) source_project_backing: usize,
    /// Largest inline key payload admitted to the projection cache.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) source_project_key_bytes: usize,
    pub(crate) graph: ProjectedNarrowingGraph<'db>,
    pub(crate) narrowed_cache: FxHashMap<(ProjectedNarrowingNodeId, Type<'db>), Type<'db>>,
    /// High-water backing bound for controlled inserts into the narrowed cache.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) source_narrowed_backing: usize,
    /// Largest inline key payload admitted to the narrowed cache.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) source_narrowed_key_bytes: usize,
}

impl<'a, 'db> NarrowingProjector<'a, 'db> {
    /// Creates a projector for narrowing `place`.
    pub(crate) fn new(
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        constraints: &'a NarrowingConstraints,
        predicates: &'a IndexSlice<ScopedPredicateId, Predicate<'db>>,
        predicate_narrowing_targets: &'a PredicateNarrowingTargets,
        place: ScopedPlaceId,
        base_ty: Type<'db>,
    ) -> Self {
        Self {
            db,
            env,
            constraints,
            predicates,
            predicate_narrowing_targets,
            place,
            base_ty,
            project_cache: FxHashMap::default(),
            #[cfg(any(test, feature = "experimental-analysis"))]
            source_project_backing: 0,
            #[cfg(any(test, feature = "experimental-analysis"))]
            source_project_key_bytes: 0,
            graph: ProjectedNarrowingGraph::default(),
            narrowed_cache: FxHashMap::default(),
            #[cfg(any(test, feature = "experimental-analysis"))]
            source_narrowed_backing: 0,
            #[cfg(any(test, feature = "experimental-analysis"))]
            source_narrowed_key_bytes: 0,
        }
    }

    /// Narrows a binding while reusing projections and shared suffixes from earlier bindings.
    pub(crate) fn narrow(
        &mut self,
        constraint: ScopedNarrowingConstraint,
        base_ty: Type<'db>,
    ) -> Type<'db> {
        let effects = narrowing_entry::OrdinaryNarrowingEntryEffects::new(self.db);
        source::infallible(narrowing_entry::narrow_projector_sync(
            self,
            constraint,
            base_ty,
            narrowing_entry::NarrowingEntryFacts,
            &effects,
        ))
    }

    pub(crate) fn set_base_type(&mut self, base_ty: Type<'db>) {
        self.base_ty = base_ty;
    }

    /// Narrows a projected constraint while reusing suffix results for its original binding type.
    ///
    /// Registering each root lets the graph recognize shared joins incrementally.
    fn narrow_projected(
        &mut self,
        root: ProjectedNarrowingNodeId,
        base_ty: Type<'db>,
    ) -> Type<'db> {
        source::infallible(narrowing_evaluation::narrow_projected_sync(
            self,
            root,
            base_ty,
            narrowing_evaluation::NarrowingEvaluationFacts,
            &narrowing_evaluation::OrdinaryNarrowingEvaluationEffects,
        ))
    }

    /// Returns the cached positive and negative narrowing constraints for a predicate.
    fn predicate_constraints(
        &mut self,
        predicate_id: ScopedPredicateId,
    ) -> (
        Option<NarrowingConstraint<'db>>,
        Option<NarrowingConstraint<'db>>,
    ) {
        source::infallible(narrowing_predicate::predicate_constraints_sync(
            self,
            predicate_id,
            &narrowing_predicate::OrdinaryNarrowingPredicateEffects,
        ))
    }

    #[cfg(test)]
    fn add_node(&mut self, node: ProjectedNarrowingNode) -> ProjectedNarrowingNodeId {
        source::infallible(narrowing_construction::build_sync(
            self,
            narrowing_construction::Frame::Add(node),
            narrowing_construction::NarrowingConstructionFacts,
            &narrowing_construction::OrdinaryNarrowingConstructionEffects,
        ))
    }

    #[cfg(test)]
    fn or(
        &mut self,
        left: ProjectedNarrowingNodeId,
        right: ProjectedNarrowingNodeId,
    ) -> ProjectedNarrowingNodeId {
        source::infallible(narrowing_construction::build_sync(
            self,
            narrowing_construction::Frame::Or(left, right),
            narrowing_construction::NarrowingConstructionFacts,
            &narrowing_construction::OrdinaryNarrowingConstructionEffects,
        ))
    }

    /// Projects one constraint node into the graph for this place.
    fn project(
        &mut self,
        root: ScopedNarrowingConstraint,
        use_root_checkpoint: bool,
    ) -> ProjectedNarrowingNodeId {
        source::infallible(narrowing_construction::build_sync(
            self,
            narrowing_construction::Frame::Project {
                root,
                use_root_checkpoint,
            },
            narrowing_construction::NarrowingConstructionFacts,
            &narrowing_construction::OrdinaryNarrowingConstructionEffects,
        ))
    }

    pub(crate) fn projected_node(&self, id: ScopedNarrowingConstraint) -> ProjectedNarrowingNodeId {
        match id {
            ScopedNarrowingConstraint::ALWAYS_TRUE => ProjectedNarrowingNodeId::ALWAYS_TRUE,
            ScopedNarrowingConstraint::ALWAYS_FALSE => ProjectedNarrowingNodeId::ALWAYS_FALSE,
            _ => self.project_cache[&(id, self.base_ty)],
        }
    }
}

/// Evaluates narrowed types over a projected narrowing graph.
pub(crate) struct ProjectedNarrowingContext<'a, 'db> {
    db: &'db dyn Db,
    pub(crate) env: &'a ProgramEnvironment<'db>,
    pub(crate) base_ty: Type<'db>,
    pub(crate) graph: &'a ProjectedNarrowingGraph<'db>,
    /// Caches each shared suffix for the binding type being narrowed.
    pub(crate) join_cache: &'a mut FxHashMap<(ProjectedNarrowingNodeId, Type<'db>), Type<'db>>,
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) source_narrowed_backing: &'a mut usize,
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) source_narrowed_key_bytes: &'a mut usize,
}

impl<'a, 'db> ProjectedNarrowingContext<'a, 'db> {
    pub(crate) fn new(projector: &'a mut NarrowingProjector<'_, 'db>, base_ty: Type<'db>) -> Self {
        Self {
            db: projector.db,
            env: projector.env,
            base_ty,
            graph: &projector.graph,
            join_cache: &mut projector.narrowed_cache,
            #[cfg(any(test, feature = "experimental-analysis"))]
            source_narrowed_backing: &mut projector.source_narrowed_backing,
            #[cfg(any(test, feature = "experimental-analysis"))]
            source_narrowed_key_bytes: &mut projector.source_narrowed_key_bytes,
        }
    }

    /// Evaluates a projected path while accumulating narrowing constraints.
    fn narrow(
        &mut self,
        id: ProjectedNarrowingNodeId,
        accumulated: Option<NarrowingConstraint<'db>>,
    ) -> Type<'db> {
        source::infallible(narrowing_evaluation::evaluate_sync(
            self,
            id,
            accumulated,
            narrowing_evaluation::NarrowingEvaluationFacts,
            &narrowing_evaluation::OrdinaryNarrowingEvaluationEffects,
        ))
    }
}

fn analyze_single_pattern_predicate_kind<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    predicate_kind: &PatternPredicateKind<'db>,
    subject_ty: Type<'db>,
    precomputed_definite_match_ty: Option<Type<'db>>,
) -> Truthiness {
    match predicate_kind {
        PatternPredicateKind::Value(value) => {
            let value_ty = infer_same_file_expression_type(db, *value, TypeContext::default());

            equality_truthiness(
                db,
                env,
                subject_ty,
                value_ty,
                ComparisonSoundnessPolicy::from_analysis_settings(
                    db.analysis_settings(value.file(db)),
                ),
            )
        }
        PatternPredicateKind::Singleton(singleton) => {
            let singleton_ty = singleton_pattern_type(db, env, *singleton);

            if subject_ty.is_equivalent_to(db, env, singleton_ty) {
                Truthiness::AlwaysTrue
            } else if subject_ty.is_disjoint_from(db, env, singleton_ty) {
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
                        definite_match_pattern_type_for_subject(db, env, p, narrowed_subject_ty);

                    let truthiness =
                        if narrowed_subject_ty.is_subtype_of(db, env, definitely_matched) {
                            Truthiness::AlwaysTrue
                        } else {
                            analyze_single_pattern_predicate_kind(
                                db,
                                env,
                                p,
                                narrowed_subject_ty,
                                Some(definitely_matched),
                            )
                        };

                    remaining_subject_ty =
                        pattern_binding_fallthrough_type(db, env, p, narrowed_subject_ty);
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
                    Type::ClassLiteral(class) => {
                        Type::instance(db, env, class.top_materialization(db))
                    }
                    Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) => {
                        Type::Callable(CallableType::top(db))
                    }
                    _ => return Truthiness::Ambiguous,
                };
            let definitely_matched = precomputed_definite_match_ty.unwrap_or_else(|| {
                definite_match_pattern_type_for_subject(db, env, predicate_kind, subject_ty)
            });

            if subject_ty.is_equivalent_to(db, env, definitely_matched)
                || subject_ty.is_subtype_of(db, env, definitely_matched)
            {
                Truthiness::AlwaysTrue
            } else if subject_ty.is_disjoint_from(db, env, class_ty) {
                Truthiness::AlwaysFalse
            } else {
                Truthiness::Ambiguous
            }
        }
        PatternPredicateKind::Mapping(kind) => {
            let mapping_ty = mapping_pattern_type(db, env);
            if subject_ty.is_subtype_of(db, env, mapping_ty) {
                if kind.is_irrefutable() {
                    Truthiness::AlwaysTrue
                } else {
                    Truthiness::Ambiguous
                }
            } else if subject_ty.is_disjoint_from(db, env, mapping_ty) {
                Truthiness::AlwaysFalse
            } else {
                Truthiness::Ambiguous
            }
        }
        PatternPredicateKind::Sequence(kind) => {
            let sequence_ty = sequence_pattern_type_builder(db, env).build();
            if subject_ty.is_subtype_of(db, env, sequence_ty) {
                if kind.is_irrefutable() {
                    Truthiness::AlwaysTrue
                } else {
                    Truthiness::Ambiguous
                }
            } else if subject_ty.is_disjoint_from(db, env, sequence_ty) {
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
                    env,
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
#[salsa::tracked(configuration = (pub(crate) AnalyzeNonTerminalCallConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial = |_, _, _| non_terminal_call_initial(),
    cycle_fn = |_, cycle: &salsa::Cycle, previous: &Truthiness, result: Truthiness, _| {
        non_terminal_call_recover(cycle, *previous, result)
    },
    heap_size = get_size2::GetSize::get_heap_size
)]
fn analyze_non_terminal_call<'db>(db: &'db dyn Db, call: CallableAndCallExpr<'db>) -> Truthiness {
    source::infallible(source::analyze_non_terminal_call_sync(
        call,
        &OrdinaryReachabilityEffects::new(db),
    ))
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(crate) fn non_terminal_call_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<AnalyzeNonTerminalCallConfiguration> {
    analyze_non_terminal_call::fn_ingredient_(db, db.zalsa())
}

pub(crate) fn non_terminal_call_initial() -> Truthiness {
    Truthiness::AlwaysTrue
}

pub(crate) fn non_terminal_call_recover(
    cycle: &salsa::Cycle<'_>,
    previous: Truthiness,
    result: Truthiness,
) -> Truthiness {
    // A call can determine whether its own target is reachable, as with `sys.exit()` before
    // `import sys` in a loop. Expression inference can lose its previous result when it stops
    // being a cycle head, so widen the predicate itself to ensure convergence. Delay widening
    // to allow the optimistic initial value to resolve to a terminal call.
    if cycle.iteration() > crate::TAINTED_CYCLES {
        previous.or(result)
    } else {
        result
    }
}

/// Shares terminal-call classification between scope reachability and defensive-check exemptions.
/// The result type is only needed when overload selection, generics, or awaiting can affect it.
pub(crate) fn is_non_terminal_call<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    is_await: bool,
    call_type: impl FnOnce() -> Type<'db>,
) -> Truthiness {
    source::infallible(source::is_non_terminal_call_sync(
        ty,
        is_await,
        call_type,
        &source::OrdinaryTerminalCallEffects::new(db, env),
    ))
}

fn analyze_non_empty_iterable(db: &dyn Db, iterable: Expression) -> Truthiness {
    match infer_same_file_expression_type(db, iterable, TypeContext::default()) {
        Type::KnownInstance(KnownInstanceType::Range { is_non_empty }) => {
            Truthiness::from(is_non_empty)
        }
        _ => Truthiness::Ambiguous,
    }
}

/// Cache the whole predicate so repeated reachability walks can reuse one query result
/// instead of looking up the expression's type and suppression behavior separately.
/// Separate sync and async queries use the expression directly as their Salsa key.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| false,
    heap_size = get_size2::GetSize::get_heap_size
)]
fn sync_context_manager_suppresses<'db>(db: &'db dyn Db, expression: Expression<'db>) -> bool {
    context_manager_suppresses(db, expression, EvaluationMode::Sync)
}

#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| false,
    heap_size = get_size2::GetSize::get_heap_size
)]
fn async_context_manager_suppresses<'db>(db: &'db dyn Db, expression: Expression<'db>) -> bool {
    context_manager_suppresses(db, expression, EvaluationMode::Async)
}

fn context_manager_suppresses<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
    evaluation_mode: EvaluationMode,
) -> bool {
    let env = ProgramEnvironment::from_scope(expression.scope(db));
    infer_same_file_expression_type(db, expression, TypeContext::default()).can_suppress_exceptions(
        db,
        &env,
        evaluation_mode,
    )
}

/// Evaluate a condition without re-testing intermediate short-circuit results.
///
/// `None` means evaluation cannot produce a result, as for an operand narrowed to `Never`.
/// This differs from ambiguous truthiness: in `flag and raises()`, where `raises()` returns
/// `Never`, only the falsy short-circuit path can complete. For `flag or raises()`, only the
/// truthy path can complete. Callers that cannot represent the absence of a result can
/// conservatively map `None` to [`Truthiness::Ambiguous`].
pub(crate) fn analyze_condition_expression(
    node: &ast::Expr,
    leaf_truthiness: &impl Fn(&ast::Expr) -> Option<Truthiness>,
) -> Option<Truthiness> {
    match node {
        ast::Expr::BoolOp(ast::ExprBoolOp { op, values, .. }) => {
            let short_circuit = Truthiness::from(op.is_or());
            let mut result = short_circuit.negate();
            for value in values {
                let Some(truthiness) = analyze_condition_expression(value, leaf_truthiness) else {
                    return result.is_ambiguous().then_some(short_circuit);
                };
                if truthiness == short_circuit {
                    return Some(short_circuit);
                }
                if truthiness.is_ambiguous() {
                    result = Truthiness::Ambiguous;
                }
            }
            Some(result)
        }
        ast::Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::Not,
            operand,
            ..
        }) => analyze_condition_expression(operand, leaf_truthiness).map(Truthiness::negate),
        ast::Expr::If(ast::ExprIf {
            test, body, orelse, ..
        }) => match analyze_condition_expression(test, leaf_truthiness)? {
            Truthiness::AlwaysTrue => analyze_condition_expression(body, leaf_truthiness),
            Truthiness::AlwaysFalse => analyze_condition_expression(orelse, leaf_truthiness),
            Truthiness::Ambiguous => {
                let body_truthiness = analyze_condition_expression(body, leaf_truthiness);
                let orelse_truthiness = analyze_condition_expression(orelse, leaf_truthiness);
                match (body_truthiness, orelse_truthiness) {
                    (None, truthiness) | (truthiness, None) => truthiness,
                    (Some(body), Some(orelse)) => Some(if body == orelse {
                        body
                    } else {
                        Truthiness::Ambiguous
                    }),
                }
            }
        },
        _ => leaf_truthiness(node),
    }
}

#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| Truthiness::Ambiguous,
    cycle_fn = |_, cycle: &salsa::Cycle, previous: &Truthiness, result: Truthiness, _| {
        // A condition can control whether one of its own inputs is reachable. Expression inference
        // can lose its previous result when it ceases to be a cycle head, so its type widening alone
        // does not ensure that the condition's truthiness converges. Delay widening here to avoid
        // retaining imprecise results from the first few iterations.
        if cycle.iteration() > crate::TAINTED_CYCLES && *previous != result {
            Truthiness::Ambiguous
        } else {
            result
        }
    },
    heap_size = get_size2::GetSize::get_heap_size
)]
fn analyze_condition<'db>(db: &'db dyn Db, expression: Expression<'db>) -> Truthiness {
    let env = ProgramEnvironment::from_scope(expression.scope(db));
    let module = parsed_module(db, expression.python_file(db)).load(db);
    let inference = infer_expression_types(db, expression, TypeContext::default());
    analyze_condition_expression(expression.node_ref(db).node(&module), &|node| {
        inference
            .comparison_truthiness(node)
            .or_else(|| inference.expression_type(node).bool_if_inhabited(db, &env))
    })
    .unwrap_or(Truthiness::Ambiguous)
}

fn analyze_single(db: &dyn Db, env: &ProgramEnvironment<'_>, predicate: &Predicate) -> Truthiness {
    let _span = tracing::trace_span!("analyze_single", ?predicate).entered();

    source::infallible(source::analyze_single_sync(
        env,
        predicate,
        ReachabilityFacts,
        &OrdinaryReachabilityEffects::new(db),
    ))
}

fn analyze_star_import_predicate<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    star_import: StarImportPlaceholderPredicate<'db>,
) -> Truthiness {
    source::infallible(star_import::analyze_star_import_sync(
        env,
        star_import,
        &star_import::OrdinaryStarImportEffects { db },
    ))
}

/// Check whether a diagnostic emitted at `range` is in reachable code, considering both
/// scope reachability and statement-level reachability within the scope.
pub(crate) fn is_range_reachable<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    scope_id: FileScopeId,
    range: TextRange,
) -> bool {
    source::infallible(range::is_range_reachable_sync(
        index,
        scope_id,
        range,
        &range::OrdinaryRangeReachabilityEffects::new(db),
    ))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReachabilityCacheKey {
    Primary(usize),
    Other {
        constraints: usize,
        id: ScopedReachabilityConstraintId,
    },
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
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) fn retained_storage(&self) -> (usize, usize, usize, usize) {
        let primary = self.primary_entries.borrow();
        let other = self.other_entries.borrow();
        (
            primary.len(),
            primary.capacity(),
            other.len(),
            other.capacity(),
        )
    }

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
    fn evaluate(
        &self,
        db: &'db dyn Db,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedReachabilityConstraintId,
    ) -> Truthiness {
        source::infallible(source::evaluate_cached_reachability_sync(
            self,
            constraints,
            predicates,
            id,
            ReachabilityFacts,
            &OrdinaryReachabilityEffects::new(db),
        ))
    }

    pub(crate) fn key(
        &self,
        scope: ScopeId<'db>,
        constraints: &ReachabilityConstraints,
        id: ScopedReachabilityConstraintId,
    ) -> ReachabilityCacheKey {
        let constraints_key = std::ptr::from_ref(constraints).addr();
        if scope == self.primary_scope && constraints_key == self.primary_constraints {
            ReachabilityCacheKey::Primary(id.index())
        } else {
            ReachabilityCacheKey::Other {
                constraints: constraints_key,
                id,
            }
        }
    }

    pub(crate) fn lookup(&self, key: ReachabilityCacheKey) -> Option<Truthiness> {
        match key {
            ReachabilityCacheKey::Primary(index) => {
                self.primary_entries.borrow().get(index).copied().flatten()
            }
            ReachabilityCacheKey::Other { constraints, id } => {
                self.other_entries.borrow().get(&(constraints, id)).copied()
            }
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) fn storage(&self, key: ReachabilityCacheKey) -> (usize, usize) {
        match key {
            ReachabilityCacheKey::Primary(_) => {
                let entries = self.primary_entries.borrow();
                (entries.len(), entries.capacity())
            }
            ReachabilityCacheKey::Other { .. } => {
                let entries = self.other_entries.borrow();
                (entries.len(), entries.capacity())
            }
        }
    }

    pub(crate) fn insert(&self, key: ReachabilityCacheKey, result: Truthiness) {
        match key {
            ReachabilityCacheKey::Primary(index) => {
                let mut entries = self.primary_entries.borrow_mut();
                if entries.len() <= index {
                    entries.resize(index + 1, None);
                }
                entries[index] = Some(result);
            }
            ReachabilityCacheKey::Other { constraints, id } => {
                self.other_entries
                    .borrow_mut()
                    .insert((constraints, id), result);
            }
        }
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
    use ty_python_core::ProgramFile;
    use ty_python_core::narrowing_constraints::InteriorNode;
    use ty_python_core::predicate::Predicates;
    use ty_python_core::semantic_index;
    use ty_python_core::symbol::ScopedSymbolId;

    #[test]
    fn non_terminal_call_range_recovers_cross_file_cycle() -> anyhow::Result<()> {
        let mut db = setup_db();
        let calls = "        other.target.ping()\n".repeat(NON_TERMINAL_CALL_CHUNK_SIZE + 1);
        let a = format!(
            r#"from b import B

class A:
    def setup(self, other: B) -> None:
{calls}        self.target = TargetA()

class TargetA:
    def ping(self) -> None: ...
"#
        );
        let b = format!(
            r#"from a import A

class B:
    def setup(self, other: A) -> None:
{calls}        self.target = TargetB()

class TargetB:
    def ping(self) -> None: ...
"#
        );
        db.write_files([("/src/a.py", a.as_str()), ("/src/b.py", b.as_str())])?;

        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let program_file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let index = semantic_index(&db, program_file);
        let class_scope = index
            .child_scopes(FileScopeId::global())
            .find(|(_, scope)| scope.node().as_class().is_some())
            .unwrap()
            .0;
        let setup_scope = index
            .child_scopes(class_scope)
            .find(|(_, scope)| scope.node().as_function().is_some())
            .unwrap()
            .0
            .to_scope_id(&db, program_file);

        // Enter the range directly so it becomes the cycle head when inferring `other.target`
        // reaches the other module and then re-enters this scope.
        analyze_non_terminal_call_range(&db, setup_scope, 0, 0);
        Ok(())
    }

    #[test]
    fn non_terminal_call_range_invalidates_when_callable_changes() -> anyhow::Result<()> {
        let mut db = setup_db();
        let source = format!(
            "from dependency import callback\n\ndef f() -> None:\n{}",
            "    callback()\n".repeat(NON_TERMINAL_CALL_CHUNK_SIZE + 1)
        );
        db.write_files([
            ("/src/dependency.py", "def callback() -> None: ..."),
            ("/src/test.py", source.as_str()),
        ])?;

        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        let function_scope = {
            let program_file = ProgramFile::new(&db, file, db.program_environment().program(&db));
            let index = semantic_index(&db, program_file);
            index.child_scopes(FileScopeId::global()).next().unwrap().0
        };
        {
            let program_file = ProgramFile::new(&db, file, db.program_environment().program(&db));
            let scope = function_scope.to_scope_id(&db, program_file);
            let use_def = use_def_map(&db, scope);
            assert!(
                evaluate_reachability_constraint(&db, scope, use_def.end_of_scope_reachability(),)
                    .may_be_true()
            );
        }

        db.write_file(
            "/src/dependency.py",
            "from typing import NoReturn\ndef callback() -> NoReturn: ...",
        )?;

        let program_file = ProgramFile::new(&db, file, db.program_environment().program(&db));
        let scope = function_scope.to_scope_id(&db, program_file);
        let use_def = use_def_map(&db, scope);
        assert!(
            evaluate_reachability_constraint(&db, scope, use_def.end_of_scope_reachability(),)
                .is_always_false()
        );
        Ok(())
    }

    #[test]
    fn deep_projected_narrowing_evaluation_does_not_overflow() {
        const DEPTH: usize = 100_000;

        let db = setup_db();
        let env = db.program_environment();
        let ty = Type::bool_literal(true);
        let mut graph = ProjectedNarrowingGraph::default();
        let mut root = ProjectedNarrowingNodeId::ALWAYS_TRUE;
        for index in 0..DEPTH {
            let atom = ScopedPredicateId::new(index);
            graph
                .predicate_constraints_cache
                .insert(atom, (Some(NarrowingConstraint::intersection(ty)), None));
            let node = ProjectedNarrowingNodeId(graph.nodes.len());
            graph
                .nodes
                .push(ProjectedNarrowingEntry::Predicate(ProjectedNarrowingNode {
                    atom,
                    if_true: root,
                    if_uncertain: ProjectedNarrowingNodeId::ALWAYS_FALSE,
                    if_false: ProjectedNarrowingNodeId::ALWAYS_FALSE,
                }));
            graph.referenced.push(true);
            graph.joins.push(false);
            root = node;
        }

        let mut join_cache = FxHashMap::default();
        let mut source_narrowed_backing = 0;
        let mut source_narrowed_key_bytes = 0;
        let mut context = ProjectedNarrowingContext {
            db: &db,
            env: &env,
            base_ty: ty,
            graph: &graph,
            join_cache: &mut join_cache,
            source_narrowed_backing: &mut source_narrowed_backing,
            source_narrowed_key_bytes: &mut source_narrowed_key_bytes,
        };
        assert_eq!(context.narrow(root, None), ty);
    }

    #[test]
    fn deep_projected_narrowing_union_does_not_overflow() {
        const DEPTH: usize = 100_000;

        let db = setup_db();
        let env = db.program_environment();
        let constraints = NarrowingConstraints::from_test_nodes(Vec::new());
        let targets = PredicateNarrowingTargets::default();
        let mut projector = NarrowingProjector::new(
            &db,
            &env,
            &constraints,
            IndexSlice::empty(),
            &targets,
            ScopedPlaceId::Symbol(ScopedSymbolId::new(0)),
            Type::unknown(),
        );

        // Adding the oldest predicate to an existing OR chain visits every uncertain edge.
        // Both input construction and the merge retain only linearly many graph nodes.
        let mut root = ProjectedNarrowingNodeId::ALWAYS_FALSE;
        for index in 1..=DEPTH {
            root = projector.add_node(ProjectedNarrowingNode {
                atom: ScopedPredicateId::new(index),
                if_true: ProjectedNarrowingNodeId::ALWAYS_TRUE,
                if_uncertain: root,
                if_false: ProjectedNarrowingNodeId::ALWAYS_FALSE,
            });
        }
        let oldest = projector.add_node(ProjectedNarrowingNode {
            atom: ScopedPredicateId::new(0),
            if_true: ProjectedNarrowingNodeId::ALWAYS_TRUE,
            if_uncertain: ProjectedNarrowingNodeId::ALWAYS_FALSE,
            if_false: ProjectedNarrowingNodeId::ALWAYS_FALSE,
        });

        let combined = projector.or(root, oldest);
        assert_eq!(projector.graph.nodes.len(), 2 * DEPTH + 1);
        assert_eq!(projector.or(oldest, root), combined);
        assert_eq!(projector.graph.nodes.len(), 2 * DEPTH + 1);

        let mut current = combined;
        for index in (0..=DEPTH).rev() {
            let ProjectedNarrowingEntry::Predicate(node) = projector.graph.node(current) else {
                panic!("OR introduced a checkpoint");
            };
            assert_eq!(node.atom, ScopedPredicateId::new(index));
            assert_eq!(node.if_true, ProjectedNarrowingNodeId::ALWAYS_TRUE);
            assert_eq!(node.if_false, ProjectedNarrowingNodeId::ALWAYS_FALSE);
            current = node.if_uncertain;
        }
        assert_eq!(current, ProjectedNarrowingNodeId::ALWAYS_FALSE);
    }

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
                let program_file =
                    ProgramFile::new(&db, file, db.program_environment().program(&db));
                let index = semantic_index(&db, program_file);
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
                let env = db.program_environment();
                let evaluator = use_def.narrowing_evaluator(ScopedNarrowingConstraint::ALWAYS_TRUE);
                let mut projector = NarrowingProjector::new(
                    &db,
                    &env,
                    &constraints,
                    &predicates,
                    evaluator.predicate_narrowing_targets(),
                    ScopedPlaceId::Symbol(x),
                    Type::unknown(),
                );

                assert_eq!(
                    projector.project(ScopedNarrowingConstraint::new(DEPTH - 1), false),
                    ProjectedNarrowingNodeId::ALWAYS_TRUE
                );
                Ok(())
            })?;

        handle.join().expect("projection thread panicked")
    }
}
