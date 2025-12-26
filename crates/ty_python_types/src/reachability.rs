//! Reachability constraint evaluation with type information.
//!
//! This module provides the type-dependent evaluation logic for reachability constraints.
//! The data structures (`ReachabilityConstraints`, `ReachabilityConstraintsBuilder`) stay
//! in `ty_python_semantic`, but the evaluation methods that need type inference are here.

use ty_python_semantic::Db;
use ty_python_semantic::Truthiness;
use ty_python_semantic::semantic_index::place::ScopedPlaceId;
use ty_python_semantic::semantic_index::place_table;
use ty_python_semantic::semantic_index::predicate::{
    CallableAndCallExpr, PatternPredicate, PatternPredicateKind, Predicate, PredicateNode,
    Predicates,
};
use ty_python_semantic::semantic_index::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_semantic::semantic_index::use_def::ConstraintsIterator;

use crate::dunder_all::dunder_all_names;
use crate::place::{Definedness, Place, RequiresExplicitReExport, imported_symbol};
use crate::types::{
    CallableTypes, IntersectionBuilder, Type, TypeContext, UnionBuilder, UnionType,
    infer_expression_type, static_expression_truthiness,
};

// Rebind some constants locally so that we don't need as many qualifiers below.
const ALWAYS_TRUE: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::ALWAYS_TRUE;
const AMBIGUOUS: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::AMBIGUOUS;
const ALWAYS_FALSE: ScopedReachabilityConstraintId = ScopedReachabilityConstraintId::ALWAYS_FALSE;

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
pub fn analyze_pattern_predicate<'db>(
    db: &'db dyn Db,
    predicate: PatternPredicate<'db>,
) -> Truthiness {
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

fn analyze_pattern_predicate_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _predicate: PatternPredicate<'db>,
) -> Truthiness {
    Truthiness::Ambiguous
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
            .map(|p| analyze_single_pattern_predicate_kind(db, p, subject_ty))
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

            let (no_overloads_return_never, all_overloads_return_never) =
                overloads_iterator.fold((true, true), |(none, all), overload| {
                    let overload_returns_never = overload
                        .return_ty
                        .is_some_and(|return_type| return_type.is_equivalent_to(db, Type::Never));

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
                Place::Defined(_, _, Definedness::AlwaysDefined, _) => Truthiness::AlwaysTrue,
                Place::Defined(_, _, Definedness::PossiblyUndefined, _) => Truthiness::Ambiguous,
                Place::Undefined => Truthiness::AlwaysFalse,
            }
        }
    }
}

/// Extension trait that provides the `evaluate` method for `ReachabilityConstraints`.
///
/// This trait is implemented in `ty_python_types` because the evaluation logic requires
/// type inference, which is not available in `ty_python_semantic`.
pub trait ReachabilityConstraintsExt {
    /// Analyze the statically known reachability for a given constraint.
    fn evaluate<'db>(
        &self,
        db: &'db dyn Db,
        predicates: &Predicates<'db>,
        id: ScopedReachabilityConstraintId,
    ) -> Truthiness;
}

impl ReachabilityConstraintsExt for ReachabilityConstraints {
    fn evaluate<'db>(
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
                        self.used_indices().get_bit(raw_index).unwrap_or(false),
                        "all used reachability constraints should have been marked as used",
                    );
                    let index = self.used_indices().rank(raw_index) as usize;
                    self.used_interiors()[index]
                }
            };
            let predicate = &predicates[node.atom];
            match analyze_single(db, predicate) {
                Truthiness::AlwaysTrue => id = node.if_true,
                Truthiness::Ambiguous => id = node.if_ambiguous,
                Truthiness::AlwaysFalse => id = node.if_false,
            }
        }
    }
}

/// Extension trait that provides the `narrow` method for `ConstraintsIterator`.
///
/// This trait is implemented in `ty_python_types` because the narrowing logic requires
/// type inference, which is not available in `ty_python_semantic`.
pub trait ConstraintsIteratorExt<'db> {
    /// Narrow a base type according to the constraints in this iterator.
    fn narrow(self, db: &'db dyn Db, base_ty: Type<'db>, place: ScopedPlaceId) -> Type<'db>;
}

impl<'db> ConstraintsIteratorExt<'db> for ConstraintsIterator<'_, 'db> {
    fn narrow(self, db: &'db dyn Db, base_ty: Type<'db>, place: ScopedPlaceId) -> Type<'db> {
        use crate::types::infer_narrowing_constraint;

        let constraint_tys: Vec<_> = self
            .filter_map(|constraint| infer_narrowing_constraint(db, constraint, place))
            .collect();

        if constraint_tys.is_empty() {
            base_ty
        } else {
            constraint_tys
                .into_iter()
                .rev()
                .fold(
                    IntersectionBuilder::new(db).add_positive(base_ty),
                    IntersectionBuilder::add_positive,
                )
                .build()
        }
    }
}

/// Extension trait that provides reachability methods for `UseDefMap`.
///
/// These methods are implemented in `ty_python_types` because they require
/// type inference through the `evaluate` method.
pub trait UseDefMapExt<'db> {
    /// Check if a node is reachable from the start of the scope.
    fn is_node_reachable(
        &self,
        db: &dyn Db,
        node_key: ty_python_semantic::node_key::NodeKey,
    ) -> bool;

    /// Check whether the function can implicitly return None (i.e., if control flow can reach
    /// the end of the function without an explicit return).
    fn can_implicitly_return_none(&self, db: &dyn Db) -> bool;

    /// Get the reachability truthiness for a binding.
    fn binding_reachability(
        &self,
        db: &dyn Db,
        binding: &ty_python_semantic::semantic_index::use_def::BindingWithConstraints<'_, 'db>,
    ) -> crate::types::Truthiness;

    /// Check if a scope-level reachability constraint is satisfied.
    fn is_reachable(&self, db: &dyn Db, reachability: ScopedReachabilityConstraintId) -> bool;
}

impl<'db> UseDefMapExt<'db> for ty_python_semantic::semantic_index::use_def::UseDefMap<'db> {
    fn is_node_reachable(
        &self,
        db: &dyn Db,
        node_key: ty_python_semantic::node_key::NodeKey,
    ) -> bool {
        self.reachability_constraints()
            .evaluate(db, self.predicates(), self.node_reachability(node_key))
            .may_be_true()
    }

    fn can_implicitly_return_none(&self, db: &dyn Db) -> bool {
        !self
            .reachability_constraints()
            .evaluate(db, self.predicates(), self.end_of_scope_reachability())
            .is_always_false()
    }

    fn binding_reachability(
        &self,
        db: &dyn Db,
        binding: &ty_python_semantic::semantic_index::use_def::BindingWithConstraints<'_, 'db>,
    ) -> crate::types::Truthiness {
        self.reachability_constraints().evaluate(
            db,
            self.predicates(),
            binding.reachability_constraint,
        )
    }

    fn is_reachable(&self, db: &dyn Db, reachability: ScopedReachabilityConstraintId) -> bool {
        self.reachability_constraints()
            .evaluate(db, self.predicates(), reachability)
            .may_be_true()
    }
}

/// Extension trait that provides reachability methods for `SemanticIndex`.
///
/// These methods are implemented in `ty_python_types` because they require
/// type inference through the `evaluate` method.
pub trait SemanticIndexExt<'db> {
    /// Check if a scope is reachable.
    fn is_scope_reachable(
        &self,
        db: &'db dyn Db,
        scope_id: ty_python_semantic::semantic_index::FileScopeId,
    ) -> bool;

    /// Check if a node is reachable within its scope.
    fn is_node_reachable(
        &self,
        db: &'db dyn Db,
        scope_id: ty_python_semantic::semantic_index::FileScopeId,
        node_key: ty_python_semantic::node_key::NodeKey,
    ) -> bool;
}

impl<'db> SemanticIndexExt<'db> for ty_python_semantic::semantic_index::SemanticIndex<'db> {
    fn is_scope_reachable(
        &self,
        db: &'db dyn Db,
        scope_id: ty_python_semantic::semantic_index::FileScopeId,
    ) -> bool {
        self.parent_scope_id(scope_id)
            .is_none_or(|parent_scope_id| {
                if !self.is_scope_reachable(db, parent_scope_id) {
                    return false;
                }

                let parent_use_def = self.use_def_map(parent_scope_id);
                let reachability = self.scope(scope_id).reachability();

                parent_use_def.is_reachable(db, reachability)
            })
    }

    fn is_node_reachable(
        &self,
        db: &'db dyn Db,
        scope_id: ty_python_semantic::semantic_index::FileScopeId,
        node_key: ty_python_semantic::node_key::NodeKey,
    ) -> bool {
        self.is_scope_reachable(db, scope_id)
            && self.use_def_map(scope_id).is_node_reachable(db, node_key)
    }
}
