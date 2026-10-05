use std::panic::AssertUnwindSafe;

use ruff_python_ast::name::Name;
use salsa::execution_probe::FinalSourceMemo;

use super::*;
use crate::FxOrderSet;
use crate::reachability::source::ReachabilityEffects;
use crate::types::infer::builder::{guarded_observations, guarded_type_with};
use crate::types::relation::redundancy_ingredient;
use crate::types::relation::source::{disjointness_observations, redundancy_observations};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::set_theoretic::builder::controlled_union::{
    UnionEffects, exclusion_observations,
};
use crate::types::set_theoretic::builder::intersection_expansion::{Sign, expansion_observations};
use crate::types::set_theoretic::builder::intersection_finalization::{
    constraints_observations, finalization_observations,
};
use crate::types::set_theoretic::builder::intersection_insertion::{
    Sign as InnerSign, insertion_observations,
};
use crate::types::set_theoretic::builder::{
    InnerIntersectionBuilder, IntersectionBuilder, IntersectionPolarity,
    IntersectionSimplification, intersection_simplification_ingredient, simplify_intersection_pair,
};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::typevar::{
    BindingContext, TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarIdentity,
    TypeVarInstance, TypeVarKind, TypeVarNonce,
};
use crate::types::{
    BoundTypeVarInstance, EnumLiteralType, LiteralValueType, NegativeIntersectionElements,
    RelationOperation, TypeVarBoundOrConstraints, TypeVarVariance,
};

enum Action<'a, 'db> {
    Pair(Type<'db>, Type<'db>),
    Guarded(Type<'db>, ast::BoolOp),
    Split(IntersectionType<'db>),
    MergeExclusions(Type<'db>, Type<'db>),
    Truthiness(Type<'db>),
    Outer(&'a [(Type<'db>, Sign)]),
    Inner(&'a mut InnerIntersectionBuilder<'db>),
}

#[derive(Default)]
struct Progress<'db> {
    cancel_guarded: Option<guarded_observations::Stage>,
    cancel_finalization: bool,
    cancel_constraints: bool,
    cancel_truthiness: bool,
    cancel_exclusions: Option<exclusion_observations::Stage>,
    cancel_disjointness: bool,
    truthiness: Cell<Option<Truthiness>>,
    truthiness_started: Cell<bool>,
    truthiness_remaining: Cell<Option<usize>>,
    split_parts: Cell<Option<Option<(Type<'db>, Type<'db>)>>>,
    merged_exclusions: Cell<Option<Option<Type<'db>>>>,
    branches_before_build: Cell<Option<usize>>,
    dropped_branches: Cell<Option<usize>>,
    completed: Cell<bool>,
}

struct OwnedOuter<'a, 'db> {
    builder: IntersectionBuilder<'db>,
    progress: &'a Progress<'db>,
}

impl Drop for OwnedOuter<'_, '_> {
    fn drop(&mut self) {
        self.progress
            .dropped_branches
            .set(Some(self.builder.branches_storage().0.len()));
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    mut action: Action<'_, 'db>,
    policy: &AnalysisPolicy,
    progress: &Progress<'db>,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    guarded_observations::reset(progress.cancel_guarded);
    finalization_observations::reset(progress.cancel_finalization);
    constraints_observations::reset(progress.cancel_constraints);
    exclusion_observations::reset(progress.cancel_exclusions);
    if matches!(
        &action,
        Action::Guarded(..) | Action::Split(..) | Action::MergeExclusions(..)
    ) {
        expansion_observations::reset();
        insertion_observations::reset();
        disjointness_observations::reset(progress.cancel_disjointness);
        redundancy_observations::reset(false);
    }
    with_analysis_session(prepared, policy, |session| {
        let environments = StableStorage::new();
        let builders = StableStorage::new();
        let owners = StableStorage::new();
        let default_arguments = StableStorage::new();
        let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
        let mapping = StableStorage::new();
        let checkers = CheckerStorage::new();
        let resources = SourceResources::new(
            &environments,
            &builders,
            &owners,
            &mapping,
            &checkers,
            &default_arguments,
            &return_callables,
        );
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let (function, overload) = register_function_values(session.db(), &mut registry)?;
        let callable = register_callable_values(session.db(), &mut registry)?;
        let bound_method = register_bound_method_values(session.db(), &mut registry)?;
        let descriptor_get_call_context =
            register_descriptor_get_call_context_values(session.db(), &mut registry)?;
        let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
        let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
        let property = register_property_values(session.db(), &mut registry)?;
        let tuple = register_tuple_values(session.db(), &mut registry)?;
        let string_literal = registry.finite_interned_values_with_memos(
            StringLiteralType::ingredient(session.db().zalsa()),
            (),
        )?;
        let union = register_union_values(session.db(), &mut registry)?;
        let intersection = register_intersection_values(session.db(), &mut registry)?;
        let module = register_module_values(session.db(), &mut registry)?;
        let class = register_class_values(session.db(), &mut registry)?;
        let known_class = register_known_class_values(session.db(), &mut registry)?;
        let member = register_member_lookup_values(session.db(), &mut registry)?;
        let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
        let expression_context = register_expression_context_values(session.db(), &mut registry)?;
        let values = SourceValues {
            type_pair,
            expression_context,
            function,
            overload,
            callable,
            bound_method,
            descriptor_get_call_context,
            descriptor_dispatch,
            descriptor_dispatches,
            property,
            tuple,
            string_literal,
            union,
            intersection,
            module,
            class,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let values = &values;
        let action = &mut action;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let effects = SourceEffects::new(&access, session.program());
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let result = match action {
                Action::Pair(first, second) => {
                    access
                        .intersection_from_two_elements(*first, *second)
                        .await?
                }
                Action::Guarded(ty, op) => {
                    guarded_type_with(session.db(), &env, *ty, *op, &effects).await?
                }
                Action::Split(intersection) => {
                    let builder = PairUnionEffects::new_union(&effects, &env).await?;
                    let parts = UnionEffects::split_truthiness_guarded_intersection(
                        &effects,
                        &builder,
                        *intersection,
                    )
                    .await?;
                    progress.split_parts.set(Some(parts));
                    Type::Intersection(*intersection)
                }
                Action::MergeExclusions(first, second) => {
                    let builder = PairUnionEffects::new_union(&effects, &env).await?;
                    let merged = UnionEffects::merge_disjoint_exclusions(
                        &effects,
                        &builder,
                        *first,
                        *second,
                    )
                    .await?;
                    progress.merged_exclusions.set(Some(merged));
                    *first
                }
                Action::Truthiness(ty) => {
                    progress.truthiness_started.set(true);
                    let truthiness = ReachabilityEffects::truthiness(&effects, &env, *ty).await?;
                    progress.truthiness.set(Some(truthiness));
                    progress.truthiness_remaining.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db()),
                    );
                    if progress.cancel_truthiness {
                        access
                            .endpoint
                            .local_call(|| {
                                session.db().cancellation_token().cancel();
                                access.endpoint.check_completion()
                            })
                            .await;
                    }
                    *ty
                }
                Action::Outer(additions) => {
                    let mut owner = OwnedOuter {
                        builder: effects.new_intersection(&env).await?,
                        progress,
                    };
                    for &(ty, sign) in *additions {
                        match sign {
                            Sign::Positive => {
                                effects
                                    .intersection_add_positive(&mut owner.builder, ty)
                                    .await?;
                            }
                            Sign::Negative => {
                                effects
                                    .intersection_add_negative(&mut owner.builder, ty)
                                    .await?;
                            }
                        }
                    }
                    progress
                        .branches_before_build
                        .set(Some(owner.builder.branches_storage().0.len()));
                    effects.intersection_build(&mut owner.builder).await?
                }
                Action::Inner(builder) => effects.inner_intersection_build(&env, builder).await?,
            };
            progress.completed.set(true);
            Ok(result)
        })
    })
}

fn union<'db>(db: &'db dyn Db, elements: impl IntoIterator<Item = Type<'db>>) -> Type<'db> {
    Type::Union(UnionType::new(
        db,
        elements.into_iter().collect::<Box<[_]>>(),
        RecursivelyDefined::No,
    ))
}

fn scalar_inner<'db>() -> InnerIntersectionBuilder<'db> {
    let mut inner = InnerIntersectionBuilder::default();
    inner.insert_signed(InnerSign::Positive, Type::unknown());
    inner.insert_signed(InnerSign::Positive, Type::AlwaysTruthy);
    inner
}

fn constrained_inner<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
) -> InnerIntersectionBuilder<'db> {
    let constraints =
        TypeVarConstraints::new(db, vec![Type::Never, Type::Never].into_boxed_slice());
    let identity =
        TypeVarIdentity::new(db, Name::new_static("T"), None, TypeVarKind::LegacyTypeVar);
    let typevar = TypeVarInstance::new(
        db,
        identity,
        Some(TypeVarBoundOrConstraintsEvaluation::Eager(
            TypeVarBoundOrConstraints::Constraints(constraints),
        )),
        Some(TypeVarVariance::Invariant),
        None,
    );
    let bound = BoundTypeVarInstance::new(
        db,
        typevar,
        BindingContext::Synthetic(env.program(db)),
        None,
        TypeVarNonce::NONE,
    );
    let mut inner = InnerIntersectionBuilder::default();
    // Raw storage keeps both original constraint slots available to the finalization contract.
    inner.insert_signed(InnerSign::Positive, Type::TypeVar(bound));
    inner.insert_signed(InnerSign::Negative, Type::Never);
    inner
}

#[test]
fn canonical_scalar_pairs_preserve_ordered_identity_and_reuse() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let mut ordered = Vec::new();
    for (first, second) in [
        (Type::Never, Type::unknown()),
        (Type::unknown(), Type::Never),
        (Type::unknown(), Type::unknown()),
        (Type::unknown(), Type::AlwaysTruthy),
        (Type::AlwaysTruthy, Type::unknown()),
    ] {
        events.take_salsa_events();
        let result = controlled(
            &prepared,
            Action::Pair(first, second),
            &funded(),
            &Progress::default(),
        );
        let Ok(AnalysisOutcome::Complete(result)) = result else {
            panic!("scalar pair did not complete: {result:?}");
        };
        let key = TypePair::new(&db, env.program(&db), first, second);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_from_two_elements_ingredient(&db),
                key.as_id()
            )
            .is_ok()
        );
        if let Type::Intersection(intersection) = result {
            assert_eq!(
                intersection
                    .positive(&db)
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
                [first, second]
            );
            assert!(intersection.negative(&db).is_empty());
            ordered.push(result);
        }
        // Ordinary construction runs after the controlled result has been published.
        let expected = IntersectionBuilder::new(&db, &env)
            .positive_elements([first, second])
            .build();
        assert_eq!(result, expected);
        events.take_salsa_events();
        assert_eq!(
            controlled(
                &prepared,
                Action::Pair(first, second),
                &funded(),
                &Progress::default()
            ),
            Ok(AnalysisOutcome::Complete(result)),
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "intersection_from_two_elements",
            None,
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
    assert_eq!(ordered.len(), 2);
    assert_ne!(ordered[0], ordered[1]);
}

fn guarded_remaining() -> [Option<usize>; 3] {
    let [positive, negative] = guarded_observations::remaining();
    [positive, negative, finalization_observations::progress().2]
}

fn assert_guarded_cleanup() {
    let expansion = expansion_observations::progress();
    assert_eq!(expansion.live_expansions, 0);
    assert_eq!(expansion.live_parents, 0);
    assert_eq!(insertion_observations::progress().0, 0);
    assert_eq!(finalization_observations::progress().0, 0);
    assert_eq!(constraints_observations::progress().0, 0);
    assert_eq!(disjointness_observations::progress().0, 0);
    assert_eq!(redundancy_observations::progress().0, 0);
    assert_no_active_attempt();
}

fn assert_guarded_completion<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    ty: Type<'db>,
    op: ast::BoolOp,
) {
    let progress = Progress::default();
    let result = controlled(prepared, Action::Guarded(ty, op), &funded(), &progress);
    assert!(progress.completed.get(), "{op:?}: {result:?}");
    assert!(guarded_remaining().iter().all(Option::is_some));
    assert_eq!(finalization_observations::progress().1, 1);
    assert_guarded_cleanup();

    // This independent signed construction follows controlled inference so it cannot
    // supply cached children or inherit a mistaken guard choice from the shared producer.
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let expected = IntersectionBuilder::new(db, &env)
        .add_positive(ty)
        .add_negative(match op {
            ast::BoolOp::And => Type::AlwaysTruthy,
            ast::BoolOp::Or => Type::AlwaysFalsy,
        })
        .build();
    assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)));
}

#[test]
fn guarded_boolean_contributions_match_signed_ordinary_construction() {
    for op in [ast::BoolOp::And, ast::BoolOp::Or] {
        for ty in [Type::unknown(), Type::any()] {
            let db = fixture();
            let prepared = prepare(&db);
            let revision = salsa::plumbing::current_revision(&db);
            assert_guarded_completion(&db, &prepared, ty, op);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_guarded_cleanup();
        }
    }
}

fn retained_split_input<'db>(
    db: &'db dyn Db,
    positive: &[Type<'db>],
    negative: &[Type<'db>],
) -> IntersectionType<'db> {
    let mut exclusions = NegativeIntersectionElements::Empty;
    for &ty in negative {
        exclusions.insert(ty);
    }
    // Preserve the input fields without simplifying the guard or constructing its core.
    IntersectionType::new(
        db,
        FxOrderSet::from_iter(positive.iter().copied()),
        exclusions,
    )
}

fn assert_split_completion<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    intersection: IntersectionType<'db>,
) -> Option<(Type<'db>, Type<'db>)> {
    let progress = Progress::default();
    assert_eq!(
        controlled(prepared, Action::Split(intersection), &funded(), &progress,),
        Ok(AnalysisOutcome::Complete(Type::Intersection(intersection))),
    );
    assert!(progress.completed.get());
    assert_guarded_cleanup();
    let Some(parts) = progress.split_parts.get() else {
        panic!("guard splitting completed without recording its result");
    };
    parts
}

#[test]
fn truthiness_guard_splitting_requires_exactly_one_marker() {
    for negative in [
        vec![],
        vec![Type::int_literal(1)],
        vec![Type::AlwaysTruthy, Type::AlwaysFalsy],
        vec![Type::AlwaysFalsy, Type::int_literal(1), Type::AlwaysTruthy],
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let intersection = retained_split_input(&db, &[Type::unknown()], &negative);
        assert_eq!(assert_split_completion(&prepared, intersection), None);
    }
}

#[test]
fn truthiness_guard_splitting_preserves_exclusions_and_core_identity() {
    for marker in [Type::AlwaysTruthy, Type::AlwaysFalsy] {
        for exclusions in [vec![], vec![Type::int_literal(1), Type::int_literal(2)]] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let revision = salsa::plumbing::current_revision(&db);
            let mut negative = exclusions.clone();
            negative.insert(negative.len() / 2, marker);
            let intersection = retained_split_input(&db, &[Type::unknown()], &negative);
            let Some((core, guard)) = assert_split_completion(&prepared, intersection) else {
                panic!("single guard did not split: {marker:?}, {exclusions:?}");
            };
            let reconstructed = controlled(
                &prepared,
                Action::Pair(core, guard),
                &funded(),
                &Progress::default(),
            );
            assert_guarded_cleanup();

            // Both controlled operations precede the ordinary oracle so it cannot warm their children.
            let mut expected_core =
                IntersectionBuilder::new(&db, &env).add_positive(Type::unknown());
            for &exclusion in &exclusions {
                expected_core.add_negative_in_place(exclusion);
            }
            let expected_core = expected_core.build();
            let expected_guard = marker.negate(&db, &env);
            assert_eq!((core, guard), (expected_core, expected_guard));
            if !exclusions.is_empty() {
                let Type::Intersection(core) = core else {
                    panic!("the core lost its retained exclusions");
                };
                assert_eq!(
                    core.negative(&db).iter().copied().collect::<Vec<_>>(),
                    exclusions,
                );
            }
            let expected = IntersectionBuilder::new(&db, &env)
                .positive_elements([expected_core, expected_guard])
                .build();
            assert_eq!(
                reconstructed,
                Ok(AnalysisOutcome::Complete(expected)),
                "{marker:?}, {exclusions:?}",
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_guarded_cleanup();
        }
    }
}

#[test]
fn truthiness_guard_splitting_preserves_positive_order_in_the_core() {
    for positive in [
        [Type::unknown(), Type::AlwaysTruthy],
        [Type::AlwaysTruthy, Type::unknown()],
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let intersection = retained_split_input(&db, &positive, &[Type::AlwaysFalsy]);
        let Some((core, guard)) = assert_split_completion(&prepared, intersection) else {
            panic!("single guard did not split");
        };
        let Type::Intersection(core_intersection) = core else {
            panic!("the core lost its retained positive elements");
        };
        assert_eq!(
            core_intersection
                .positive(&db)
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            positive,
        );
        assert!(core_intersection.negative(&db).is_empty());
        let expected = IntersectionBuilder::new(&db, &env)
            .positive_elements(positive)
            .build();
        assert_eq!(core, expected);
        assert_eq!(guard, Type::AlwaysFalsy.negate(&db, &env));
        assert_guarded_cleanup();
    }
}

fn retained_split_retry_input(db: &dyn Db) -> IntersectionType<'_> {
    retained_split_input(
        db,
        &[Type::unknown()],
        &[Type::int_literal(1), Type::AlwaysFalsy],
    )
}

#[test]
fn truthiness_guard_splitting_refusals_and_native_cancellation_allow_retry() {
    let measured = fixture();
    let prepared = prepare(&measured);
    let intersection = retained_split_retry_input(&measured);
    assert!(assert_split_completion(&prepared, intersection).is_some());
    let (_, entered, remaining) = finalization_observations::progress();
    assert_eq!(entered, 1);
    let Some(remaining) = remaining else {
        panic!("guard splitting did not retain the core finalization owner");
    };
    let retained_work = funded().semantic_work_limit - remaining;

    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture();
        let prepared = prepare(&db);
        let intersection = retained_split_retry_input(&db);
        match controlled(
            &prepared,
            Action::Split(intersection),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &Progress::default(),
        ) {
            Ok(AnalysisOutcome::Complete(_)) => upper = middle,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                ..
            }) => lower = middle + 1,
            other => panic!("{other:?}"),
        }
        assert_guarded_cleanup();
    }
    assert!(upper > 0);

    for (policy, refusal) in [
        (
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            },
            Some(AnalysisIncomplete::WorkLimit),
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            Some(AnalysisIncomplete::RequestedAllocationLimit),
        ),
        (funded(), None),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let intersection = retained_split_retry_input(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let progress = Progress {
            cancel_finalization: refusal.is_none(),
            ..Progress::default()
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, Action::Split(intersection), &policy, &progress)
        }));
        match (result, refusal) {
            (Err(salsa::Cancelled::Local), None) => {
                assert_eq!(finalization_observations::progress().1, 1);
            }
            (Ok(outcome), Some(reason)) => {
                assert_eq!(
                    outcome,
                    Ok(AnalysisOutcome::Incomplete {
                        reason,
                        completed: (),
                    }),
                );
                assert_eq!(finalization_observations::progress().1, 1);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(progress.split_parts.get(), None);
        assert!(!progress.completed.get());
        assert_guarded_cleanup();

        let Some((core, guard)) = assert_split_completion(&prepared, intersection) else {
            panic!("single guard did not split after interruption");
        };
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let expected = IntersectionBuilder::new(&db, &env)
            .add_positive(Type::unknown())
            .add_negative(Type::int_literal(1))
            .build();
        assert_eq!(core, expected);
        assert_eq!(guard, Type::AlwaysFalsy.negate(&db, &env));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_guarded_cleanup();
    }
}

fn retained_merge_inputs<'db>(
    db: &'db dyn Db,
    positive: &[Type<'db>],
    left_negative: &[Type<'db>],
    right_negative: &[Type<'db>],
) -> (Type<'db>, Type<'db>) {
    (
        Type::Intersection(retained_split_input(db, positive, left_negative)),
        Type::Intersection(retained_split_input(db, positive, right_negative)),
    )
}

fn assert_exclusion_merge_completion<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    first: Type<'db>,
    second: Type<'db>,
) -> Option<Type<'db>> {
    let progress = Progress::default();
    assert_eq!(
        controlled(
            prepared,
            Action::MergeExclusions(first, second),
            &funded(),
            &progress,
        ),
        Ok(AnalysisOutcome::Complete(first)),
    );
    assert!(progress.completed.get());
    assert_exclusion_merge_cleanup();
    let Some(merged) = progress.merged_exclusions.get() else {
        panic!("exclusion merging completed without recording its result");
    };
    merged
}

fn assert_exclusion_merge_cleanup() {
    let progress = exclusion_observations::progress();
    assert_eq!(progress.live_buffers, 0);
    assert_eq!(progress.entered_buffers, progress.dropped_buffers);
    assert_eq!(progress.spilled_buffers, progress.dropped_spilled_buffers);
    assert_guarded_cleanup();
}

fn exclusion_pair_keys<'db>(pairs: &[(Type<'db>, Type<'db>)]) -> Vec<(u64, u64)> {
    pairs
        .iter()
        .map(|&(first, second)| {
            (
                exclusion_observations::type_key(first),
                exclusion_observations::type_key(second),
            )
        })
        .collect()
}

#[test]
fn exclusion_merging_requires_two_intersections() {
    for reverse in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let intersection = Type::Intersection(retained_split_input(
            &db,
            &[Type::unknown()],
            &[Type::int_literal(1)],
        ));
        let (first, second) = if reverse {
            (Type::unknown(), intersection)
        } else {
            (intersection, Type::unknown())
        };
        assert_eq!(
            assert_exclusion_merge_completion(&prepared, first, second),
            None,
        );
        assert_eq!(exclusion_observations::progress().entered_buffers, 0);
    }
}

#[test]
fn exclusion_merging_compares_positive_sets_without_reordering_them() {
    for positive in [
        [Type::unknown(), Type::AlwaysTruthy],
        [Type::AlwaysTruthy, Type::unknown()],
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let first = Type::Intersection(retained_split_input(
            &db,
            &positive,
            &[Type::int_literal(1)],
        ));
        let second = Type::Intersection(retained_split_input(
            &db,
            &[positive[1], positive[0]],
            &[Type::int_literal(2)],
        ));
        let merged = assert_exclusion_merge_completion(&prepared, first, second);

        // Construct the ordinary result only after the controlled merge has finished.
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let expected = IntersectionBuilder::new(&db, &env)
            .positive_elements(positive)
            .build();
        assert_eq!(merged, Some(expected));
        let Some(Type::Intersection(merged)) = merged else {
            panic!("the merged intersection lost its positive elements");
        };
        assert_eq!(
            merged.positive(&db).iter().copied().collect::<Vec<_>>(),
            positive,
        );
        assert!(merged.negative(&db).is_empty());
    }
}

#[test]
fn exclusion_merging_rejects_different_positive_sets() {
    for right_positive in [
        vec![Type::unknown()],
        vec![Type::unknown(), Type::AlwaysFalsy],
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let first = Type::Intersection(retained_split_input(
            &db,
            &[Type::unknown(), Type::AlwaysTruthy],
            &[Type::int_literal(1)],
        ));
        let second = Type::Intersection(retained_split_input(
            &db,
            &right_positive,
            &[Type::int_literal(2)],
        ));
        assert_eq!(
            assert_exclusion_merge_completion(&prepared, first, second),
            None,
        );
        let observed = exclusion_observations::progress();
        assert_eq!(observed.entered_buffers, 0);
        assert!(observed.pairs.is_empty());
    }
}

#[test]
fn exclusion_merging_leaves_both_exact_containments_to_union_reduction() {
    for reverse in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let (first, second) = retained_merge_inputs(
            &db,
            &[Type::unknown()],
            &[Type::int_literal(1)],
            &[Type::int_literal(1), Type::int_literal(2)],
        );
        let (first, second) = if reverse {
            (second, first)
        } else {
            (first, second)
        };
        assert_eq!(
            assert_exclusion_merge_completion(&prepared, first, second),
            None,
        );
        let observed = exclusion_observations::progress();
        assert_eq!(observed.entered_buffers, 2);
        assert_eq!(observed.pushes, if reverse { 2 } else { 1 });
        assert!(observed.pairs.is_empty());
    }
}

#[test]
fn exclusion_merging_checks_every_disjoint_unique_pair() {
    let db = fixture();
    let prepared = prepare(&db);
    let (first, second) = retained_merge_inputs(
        &db,
        &[Type::unknown()],
        &[Type::int_literal(1), Type::int_literal(2)],
        &[Type::int_literal(3), Type::int_literal(4)],
    );
    let merged = assert_exclusion_merge_completion(&prepared, first, second);
    assert_eq!(
        exclusion_observations::progress().pairs,
        exclusion_pair_keys(&[
            (Type::int_literal(1), Type::int_literal(3)),
            (Type::int_literal(2), Type::int_literal(3)),
            (Type::int_literal(1), Type::int_literal(4)),
            (Type::int_literal(2), Type::int_literal(4)),
        ]),
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let expected = IntersectionBuilder::new(&db, &env)
        .add_positive(Type::unknown())
        .build();
    assert_eq!(merged, Some(expected));
}

#[test]
fn exclusion_merging_retains_partitions_across_canonical_children_and_cancellation() {
    for cancel in [false, true] {
        let db = fixture();
        let mut events = db.clone();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let (first, second) = retained_merge_inputs(
            &db,
            &[Type::unknown()],
            &[Type::AlwaysFalsy],
            &[Type::AlwaysTruthy],
        );
        let progress = Progress {
            cancel_disjointness: cancel,
            ..Progress::default()
        };
        events.take_salsa_events();
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                Action::MergeExclusions(first, second),
                &funded(),
                &progress,
            )
        }));
        if cancel {
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            assert_eq!(progress.merged_exclusions.get(), None);
            assert!(!progress.completed.get());
        } else {
            assert!(matches!(result, Ok(Ok(AnalysisOutcome::Complete(ty))) if ty == first));
            assert_eq!(
                progress.merged_exclusions.get(),
                Some(Some(Type::unknown()))
            );
            assert!(progress.completed.get());
        }
        let executed = events.take_salsa_events();
        let keys: Vec<_> = executed
            .iter()
            .filter_map(|event| {
                if let salsa::EventKind::WillExecute { database_key } = event.kind
                    && db.ingredient_debug_name(database_key.ingredient_index())
                        == "simplify_intersection_pair_impl"
                {
                    Some(database_key.key_index())
                } else {
                    None
                }
            })
            .collect();
        let [key] = keys.as_slice() else {
            panic!("the unique exclusion pair did not run exactly one canonical simplification");
        };
        // Immediate-fallback queries finish before propagating native cancellation to the merge.
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_simplification_ingredient(&db),
                *key,
            )
            .is_ok()
        );
        let (live, entered, remaining) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert!(remaining.is_some());
        assert_eq!(redundancy_observations::progress().1, 2);
        let observed = exclusion_observations::progress();
        assert_eq!(observed.entered_buffers, 2);
        assert_eq!(observed.pushes, 1);
        assert_eq!(
            observed.pairs,
            exclusion_pair_keys(&[(Type::AlwaysFalsy, Type::AlwaysTruthy)]),
        );
        assert_exclusion_merge_cleanup();

        events.take_salsa_events();
        let merged = assert_exclusion_merge_completion(&prepared, first, second);
        assert_eq!(disjointness_observations::progress(), (0, 0, None));
        assert_eq!(redundancy_observations::progress(), (0, 0, None));
        let env = ProgramEnvironment::from_file(prepared.program_file());
        assert_eq!(
            simplify_intersection_pair(
                &db,
                &env,
                Type::AlwaysFalsy,
                Type::AlwaysTruthy,
                IntersectionPolarity::Positive,
            ),
            IntersectionSimplification::Disjoint,
        );
        let expected = IntersectionBuilder::new(&db, &env)
            .add_positive(Type::unknown())
            .build();
        assert_eq!(merged, Some(expected));
        assert_function_query_was_not_run_by_name(
            &db,
            "simplify_intersection_pair_impl",
            None,
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_exclusion_merge_cleanup();
    }
}

#[test]
fn exclusion_merging_stops_at_each_non_disjoint_simplification() {
    let unpromotable = LiteralValueType::unpromotable(true);
    let recursive =
        Type::LiteralValue(unpromotable.with_recursively_defined(RecursivelyDefined::Yes));
    let unpromotable = Type::LiteralValue(unpromotable);
    let promotable = Type::bool_literal(true);
    // These distinct identities overlap, yielding each of the three non-disjoint outcomes.
    for (left, right) in [
        (unpromotable, promotable),
        (promotable, unpromotable),
        (unpromotable, recursive),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let (first, second) = retained_merge_inputs(
            &db,
            &[Type::unknown()],
            &[Type::int_literal(1), left],
            &[right, Type::int_literal(2)],
        );
        assert_eq!(
            assert_exclusion_merge_completion(&prepared, first, second),
            None,
        );
        assert_eq!(
            exclusion_observations::progress().pairs,
            exclusion_pair_keys(&[(Type::int_literal(1), right), (left, right)]),
        );
    }
}

fn retained_spilled_merge_inputs(db: &dyn Db) -> (Type<'_>, Type<'_>) {
    retained_merge_inputs(
        db,
        &[Type::unknown()],
        &[
            Type::int_literal(11),
            Type::int_literal(1),
            Type::int_literal(12),
            Type::int_literal(2),
            Type::int_literal(13),
            Type::int_literal(3),
        ],
        &[
            Type::int_literal(4),
            Type::int_literal(13),
            Type::int_literal(5),
            Type::int_literal(11),
            Type::int_literal(12),
        ],
    )
}

#[test]
fn exclusion_merging_preserves_common_order_with_both_partitions_spilled() {
    let db = fixture();
    let prepared = prepare(&db);
    let (first, second) = retained_spilled_merge_inputs(&db);
    let merged = assert_exclusion_merge_completion(&prepared, first, second);
    let observed = exclusion_observations::progress();
    assert_eq!(observed.spilled_buffers, 2);
    assert_eq!(observed.pushes, 6);
    assert_eq!(observed.peak_len, 3);
    assert!(observed.peak_capacity >= 3);
    assert!(observed.dropped_capacity >= 6);
    assert_eq!(
        observed.pairs,
        exclusion_pair_keys(&[
            (Type::int_literal(1), Type::int_literal(4)),
            (Type::int_literal(2), Type::int_literal(4)),
            (Type::int_literal(3), Type::int_literal(4)),
            (Type::int_literal(1), Type::int_literal(5)),
            (Type::int_literal(2), Type::int_literal(5)),
            (Type::int_literal(3), Type::int_literal(5)),
        ]),
    );
    let common = [
        Type::int_literal(11),
        Type::int_literal(12),
        Type::int_literal(13),
    ];
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut expected = IntersectionBuilder::new(&db, &env).add_positive(Type::unknown());
    for ty in common {
        expected.add_negative_in_place(ty);
    }
    let expected = expected.build();
    assert_eq!(merged, Some(expected));
    let Some(Type::Intersection(merged)) = merged else {
        panic!("the merged intersection lost its common exclusions");
    };
    assert_eq!(
        merged.negative(&db).iter().copied().collect::<Vec<_>>(),
        common,
    );
}

fn assert_spilled_exclusion_merge_retry<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    first: Type<'db>,
    second: Type<'db>,
) {
    let merged = assert_exclusion_merge_completion(prepared, first, second);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut expected = IntersectionBuilder::new(db, &env).add_positive(Type::unknown());
    for ty in [
        Type::int_literal(11),
        Type::int_literal(12),
        Type::int_literal(13),
    ] {
        expected.add_negative_in_place(ty);
    }
    assert_eq!(merged, Some(expected.build()));
    assert_exclusion_merge_cleanup();
}

#[test]
fn exclusion_partition_work_refusal_and_native_cancellation_release_storage_and_retry() {
    let measured = fixture();
    let prepared = prepare(&measured);
    let (first, second) = retained_spilled_merge_inputs(&measured);
    assert!(assert_exclusion_merge_completion(&prepared, first, second).is_some());
    let Some(remaining) = exclusion_observations::progress().partition_remaining else {
        panic!("exclusion merging did not finish partitioning");
    };
    let retained_work = funded().semantic_work_limit - remaining;

    for (cancel_exclusions, pushes, pairs) in [
        (None, 6, 0),
        (Some(exclusion_observations::Stage::AfterPush(5)), 5, 0),
        (
            Some(exclusion_observations::Stage::BeforeReconstruction),
            6,
            6,
        ),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let (first, second) = retained_spilled_merge_inputs(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let progress = Progress {
            cancel_exclusions,
            ..Progress::default()
        };
        let policy = AnalysisPolicy {
            semantic_work_limit: if cancel_exclusions.is_some() {
                funded().semantic_work_limit
            } else {
                retained_work
            },
            ..funded()
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                Action::MergeExclusions(first, second),
                &policy,
                &progress,
            )
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel_exclusions.is_some() => {}
            Ok(outcome) if cancel_exclusions.is_none() => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            other => panic!("{cancel_exclusions:?}: {other:?}"),
        }
        assert_eq!(progress.merged_exclusions.get(), None);
        assert!(!progress.completed.get());
        let observed = exclusion_observations::progress();
        assert_eq!(observed.entered_buffers, 2);
        assert_eq!(observed.pushes, pushes);
        assert_eq!(observed.pairs.len(), pairs);
        assert_eq!(observed.spilled_buffers, if pushes == 5 { 1 } else { 2 });
        assert_exclusion_merge_cleanup();

        assert_spilled_exclusion_merge_retry(&db, &prepared, first, second);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn exclusion_partition_allocation_refusals_precede_each_spill_and_allow_retry() {
    for pushes in [4, 5] {
        let mut lower = 0;
        let mut upper = funded().requested_bytes_limit;
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            // Each quota is measured cold, before the ordinary result can cache children.
            let db = fixture();
            let prepared = prepare(&db);
            let (first, second) = retained_spilled_merge_inputs(&db);
            let outcome = controlled(
                &prepared,
                Action::MergeExclusions(first, second),
                &AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                },
                &Progress::default(),
            );
            assert!(
                matches!(
                    outcome,
                    Ok(AnalysisOutcome::Complete(_))
                        | Ok(AnalysisOutcome::Incomplete {
                            reason: AnalysisIncomplete::RequestedAllocationLimit,
                            ..
                        })
                ),
                "pushes {pushes}, quota {middle}: {outcome:?}",
            );
            if exclusion_observations::progress().pushes >= pushes {
                upper = middle;
            } else {
                lower = middle + 1;
            }
            assert_exclusion_merge_cleanup();
        }

        let db = fixture();
        let prepared = prepare(&db);
        let (first, second) = retained_spilled_merge_inputs(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let progress = Progress::default();
        // The next push grows one full inline partition into heap storage.
        assert_eq!(
            controlled(
                &prepared,
                Action::MergeExclusions(first, second),
                &AnalysisPolicy {
                    requested_bytes_limit: upper,
                    ..funded()
                },
                &progress,
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                completed: (),
            }),
        );
        assert_eq!(progress.merged_exclusions.get(), None);
        assert!(!progress.completed.get());
        let observed = exclusion_observations::progress();
        assert_eq!(observed.entered_buffers, 2);
        assert_eq!(observed.pushes, pushes);
        assert_eq!(observed.spilled_buffers, usize::from(pushes == 5));
        assert!(observed.pairs.is_empty());
        assert_eq!(finalization_observations::progress().1, 0);
        assert_exclusion_merge_cleanup();

        assert_spilled_exclusion_merge_retry(&db, &prepared, first, second);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn assert_ambiguous_intersection<'db>(prepared: &PreparedAnalysisFile<'db>, ty: Type<'db>) {
    assert!(matches!(ty, Type::Intersection(_)));
    let progress = Progress::default();
    assert_eq!(
        controlled(prepared, Action::Truthiness(ty), &funded(), &progress),
        Ok(AnalysisOutcome::Complete(ty)),
    );
    assert_eq!(progress.truthiness.get(), Some(Truthiness::Ambiguous));
    assert!(progress.completed.get());
    assert_guarded_cleanup();
}

#[test]
fn non_enum_intersection_truthiness_preserves_ambiguous_results() {
    for ty in [Type::unknown(), Type::any()] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        for action in [
            Action::Guarded(ty, ast::BoolOp::And),
            Action::Guarded(ty, ast::BoolOp::Or),
            Action::Pair(ty, Type::AlwaysTruthy),
        ] {
            let result = controlled(&prepared, action, &funded(), &Progress::default());
            let Ok(AnalysisOutcome::Complete(intersection)) = result else {
                panic!("intersection construction did not complete: {result:?}");
            };
            assert_ambiguous_intersection(&prepared, intersection);
            assert_ambiguous_intersection(&prepared, intersection);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);

            let env = ProgramEnvironment::from_file(prepared.program_file());
            assert_eq!(intersection.bool(&db, &env), Truthiness::Ambiguous);
        }
    }
}

#[test]
fn enum_intersection_truthiness_reaches_the_real_complement_boundary() {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "from enum import Enum\nclass Choice(Enum):\n    FIRST = 1\n    SECOND = 2\nleft = right = Choice\n",
    )
    .unwrap();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let expr = expression(&db);
    let inference = infer_expression_types(&db, expr, TypeContext::default());
    let Type::ClassLiteral(class) = inference.expression_type(expr.node_ref(&db)) else {
        panic!("fixture expression names an enum class");
    };
    let Some(enum_class) = class.into_enum_class(&db) else {
        panic!("fixture class has canonical enum members");
    };
    let instance = Type::instance(&db, &env, ClassType::NonGeneric(class));
    let literal = Type::enum_literal(EnumLiteralType::new(
        &db,
        enum_class,
        Name::new_static("FIRST"),
    ));
    // Retain the signed input: ordinary finalization can replace it with its remaining member.
    let intersection = IntersectionType::new(
        &db,
        FxOrderSet::from_iter([instance]),
        NegativeIntersectionElements::Single(literal),
    );
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        let progress = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                Action::Truthiness(Type::Intersection(intersection)),
                &funded(),
                &progress,
            ),
            Ok(unavailable(OperationId::EnumComplementIntern)),
        );
        assert!(progress.truthiness_started.get());
        assert_eq!(progress.truthiness.get(), None);
        assert!(!progress.completed.get());
        assert_guarded_cleanup();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    assert!(intersection.enum_complement(&db, &env).is_some());
}

fn retained_scalar_intersection(db: &dyn Db) -> Type<'_> {
    Type::Intersection(IntersectionType::new(
        db,
        FxOrderSet::from_iter([Type::unknown(), Type::AlwaysTruthy]),
        NegativeIntersectionElements::Empty,
    ))
}

#[test]
fn intersection_truthiness_refusal_and_native_cancellation_allow_retry() {
    let measured = fixture();
    let prepared = prepare(&measured);
    let ty = retained_scalar_intersection(&measured);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, Action::Truthiness(ty), &funded(), &progress),
        Ok(AnalysisOutcome::Complete(ty)),
    );
    let Some(remaining) = progress.truthiness_remaining.get() else {
        panic!("truthiness did not complete");
    };
    let work = funded().semantic_work_limit - remaining;
    assert!(work > 0);
    assert_guarded_cleanup();

    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture();
        let prepared = prepare(&db);
        let ty = retained_scalar_intersection(&db);
        match controlled(
            &prepared,
            Action::Truthiness(ty),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &Progress::default(),
        ) {
            Ok(AnalysisOutcome::Complete(_)) => upper = middle,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                ..
            }) => lower = middle + 1,
            other => panic!("{other:?}"),
        }
        assert_guarded_cleanup();
    }
    assert!(upper > 0);

    for (policy, refusal) in [
        (
            AnalysisPolicy {
                semantic_work_limit: work - 1,
                ..funded()
            },
            Some(AnalysisIncomplete::WorkLimit),
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            Some(AnalysisIncomplete::RequestedAllocationLimit),
        ),
        (funded(), None),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let ty = retained_scalar_intersection(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let progress = Progress {
            cancel_truthiness: refusal.is_none(),
            ..Progress::default()
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, Action::Truthiness(ty), &policy, &progress)
        }));
        match (result, refusal) {
            (Err(salsa::Cancelled::Local), None) => {
                assert_eq!(progress.truthiness.get(), Some(Truthiness::Ambiguous));
            }
            (Ok(outcome), Some(reason)) => {
                assert_eq!(
                    outcome,
                    Ok(AnalysisOutcome::Incomplete {
                        reason,
                        completed: (),
                    }),
                );
                assert_eq!(progress.truthiness.get(), None);
            }
            other => panic!("{other:?}"),
        }
        assert!(progress.truthiness_started.get());
        assert!(!progress.completed.get());
        assert_guarded_cleanup();

        assert_ambiguous_intersection(&prepared, ty);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn guarded_boolean_owners_retire_on_work_refusal_and_native_cancellation() {
    for op in [ast::BoolOp::And, ast::BoolOp::Or] {
        let measured = fixture();
        let prepared = prepare(&measured);
        assert_guarded_completion(&measured, &prepared, Type::unknown(), op);
        let remaining = guarded_remaining();

        for (boundary, cancel_guarded) in [
            Some(guarded_observations::Stage::AfterPositive),
            Some(guarded_observations::Stage::AfterNegative),
            None,
        ]
        .into_iter()
        .enumerate()
        {
            let Some(remaining) = remaining[boundary] else {
                panic!("guarded construction did not reach boundary {boundary}");
            };
            for cancel in [false, true] {
                let db = fixture();
                let prepared = prepare(&db);
                let revision = salsa::plumbing::current_revision(&db);
                let progress = Progress {
                    cancel_guarded: if cancel { cancel_guarded } else { None },
                    cancel_finalization: cancel && cancel_guarded.is_none(),
                    ..Progress::default()
                };
                let policy = AnalysisPolicy {
                    semantic_work_limit: if cancel {
                        funded().semantic_work_limit
                    } else {
                        funded().semantic_work_limit - remaining
                    },
                    ..funded()
                };
                let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                    controlled(
                        &prepared,
                        Action::Guarded(Type::unknown(), op),
                        &policy,
                        &progress,
                    )
                }));
                match result {
                    Err(salsa::Cancelled::Local) if cancel => {}
                    Ok(outcome) if !cancel => assert_eq!(
                        outcome,
                        Ok(AnalysisOutcome::Incomplete {
                            reason: AnalysisIncomplete::WorkLimit,
                            completed: (),
                        }),
                    ),
                    other => panic!("{op:?}, boundary {boundary}, cancel {cancel}: {other:?}"),
                }
                assert!(!progress.completed.get());
                let reached = guarded_remaining();
                assert!(reached[..=boundary].iter().all(Option::is_some));
                assert!(reached[boundary + 1..].iter().all(Option::is_none));
                assert_guarded_cleanup();

                assert_guarded_completion(&db, &prepared, Type::unknown(), op);
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
                assert_guarded_cleanup();
            }
        }
    }
}

#[test]
fn guarded_boolean_allocation_refusals_drain_retained_owners_and_retry() {
    for op in [ast::BoolOp::And, ast::BoolOp::Or] {
        for boundary in 0..3 {
            let mut lower = 0;
            let mut upper = funded().requested_bytes_limit;
            while lower < upper {
                let middle = lower + (upper - lower) / 2;
                // Each candidate starts cold so completed children cannot reduce its cost.
                let db = fixture();
                let prepared = prepare(&db);
                let result = controlled(
                    &prepared,
                    Action::Guarded(Type::unknown(), op),
                    &AnalysisPolicy {
                        requested_bytes_limit: middle,
                        ..funded()
                    },
                    &Progress::default(),
                );
                assert!(
                    matches!(
                        result,
                        Ok(AnalysisOutcome::Complete(_))
                            | Ok(AnalysisOutcome::Incomplete {
                                reason: AnalysisIncomplete::RequestedAllocationLimit,
                                ..
                            })
                    ),
                    "{op:?}, boundary {boundary}: {result:?}"
                );
                if guarded_remaining()[boundary].is_some() {
                    upper = middle;
                } else {
                    lower = middle + 1;
                }
                assert_guarded_cleanup();
            }

            let db = fixture();
            let prepared = prepare(&db);
            let revision = salsa::plumbing::current_revision(&db);
            let progress = Progress::default();
            // The minimum quota reaching this stage leaves the next real allocation unpaid.
            assert_eq!(
                controlled(
                    &prepared,
                    Action::Guarded(Type::unknown(), op),
                    &AnalysisPolicy {
                        requested_bytes_limit: upper,
                        ..funded()
                    },
                    &progress,
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    completed: (),
                }),
                "{op:?}, boundary {boundary}",
            );
            assert!(!progress.completed.get());
            let reached = guarded_remaining();
            assert!(reached[..=boundary].iter().all(Option::is_some));
            assert!(reached[boundary + 1..].iter().all(Option::is_none));
            assert_guarded_cleanup();

            assert_guarded_completion(&db, &prepared, Type::unknown(), op);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_guarded_cleanup();
        }
    }
}

#[test]
fn empty_and_singleton_outer_assembly_complete_without_union_normalization() {
    let db = fixture();
    let prepared = prepare(&db);
    let empty_union = union(&db, [Type::Never, Type::Never]);
    for (additions, branches, expected) in [
        (Vec::new(), 1, Type::object()),
        (vec![(empty_union, Sign::Positive)], 0, Type::Never),
        (vec![(Type::unknown(), Sign::Positive)], 1, Type::unknown()),
    ] {
        let progress = Progress::default();
        assert_eq!(
            controlled(&prepared, Action::Outer(&additions), &funded(), &progress),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(progress.branches_before_build.get(), Some(branches));
        assert_eq!(progress.dropped_branches.get(), Some(0));
        assert!(progress.completed.get());
        assert_no_active_attempt();
    }
}

#[test]
fn eager_upper_bound_expansion_runs_both_signed_loops() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let identity =
        TypeVarIdentity::new(&db, Name::new_static("T"), None, TypeVarKind::LegacyTypeVar);
    let typevar = TypeVarInstance::new(
        &db,
        identity,
        Some(TypeVarBoundOrConstraintsEvaluation::Eager(
            TypeVarBoundOrConstraints::UpperBound(Type::Never),
        )),
        Some(TypeVarVariance::Invariant),
        None,
    );
    let bound = BoundTypeVarInstance::new(
        &db,
        typevar,
        BindingContext::Synthetic(env.program(&db)),
        None,
        TypeVarNonce::NONE,
    );
    let mut inner = InnerIntersectionBuilder::default();
    inner.insert_signed(InnerSign::Positive, Type::TypeVar(bound));
    inner.insert_signed(InnerSign::Negative, Type::AlwaysTruthy);
    expansion_observations::reset();

    // The upper bound becomes Never, and the original negative still passes through expansion.
    assert_eq!(
        controlled(
            &prepared,
            Action::Inner(&mut inner),
            &funded(),
            &Progress::default()
        ),
        Ok(AnalysisOutcome::Complete(Type::Never)),
    );
    let (live, entered, _) = finalization_observations::progress();
    assert_eq!((live, entered), (0, 2));
    assert_eq!(constraints_observations::progress(), (0, 0, None));
    let expansions = expansion_observations::progress();
    assert_eq!(expansions.entered_expansions, 2);
    assert_eq!(expansions.live_expansions, 0);
    assert_eq!(expansions.live_parents, 0);
    assert_no_active_attempt();
}

#[test]
fn multiple_branches_refuse_at_the_real_union_child() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let alternatives = union(
        &db,
        [Type::AlwaysTruthy, Type::AlwaysFalsy, Type::unknown()],
    );
    for attempt in 0..2 {
        events.take_salsa_events();
        let progress = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                Action::Pair(Type::unknown(), alternatives),
                &funded(),
                &progress
            ),
            Ok(unavailable(OperationId::Union)),
        );
        // Combining the first two finalized inputs reaches an unsupported union operation before
        // requesting the third.
        assert_eq!(finalization_observations::progress().0, 0);
        assert_eq!(finalization_observations::progress().1, 2);
        assert!(!progress.completed.get());
        let events = events.take_salsa_events();
        if attempt == 1 {
            assert_function_query_was_not_run_by_name(
                &db,
                "simplify_intersection_pair_impl",
                None,
                &events,
            );
        }
        let program = prepared.program_file().program(&db);
        let parent = TypePair::new(&db, program, Type::unknown(), alternatives);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_from_two_elements_ingredient(&db),
                parent.as_id()
            )
            .is_err()
        );
        for second in [Type::AlwaysTruthy, Type::AlwaysFalsy] {
            for (first, second) in [(Type::unknown(), second), (second, Type::unknown())] {
                let child = TypePair::new(&db, program, first, second);
                if attempt == 1 {
                    assert_function_query_was_not_run_by_name(
                        &db,
                        "is_redundant_with_impl",
                        Some(child.as_id()),
                        &events,
                    );
                }
                assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        redundancy_ingredient(&db),
                        child.as_id()
                    )
                    .is_ok()
                );
            }
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn retained_inner_and_constraint_owners_retire_on_refusal_and_cancellation() {
    for constraints in [false, true] {
        let measured = fixture();
        let prepared = prepare(&measured);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let mut inner = if constraints {
            constrained_inner(&measured, &env)
        } else {
            scalar_inner()
        };
        let result = controlled(
            &prepared,
            Action::Inner(&mut inner),
            &funded(),
            &Progress::default(),
        );
        assert!(
            matches!(result, Ok(AnalysisOutcome::Complete(_))),
            "{result:?}"
        );
        let (live, entered, remaining) = if constraints {
            constraints_observations::progress()
        } else {
            finalization_observations::progress()
        };
        assert_eq!((live, entered), (0, 1));
        let Some(remaining) = remaining else {
            panic!("finalization did not retain its owner");
        };
        let retained_work = funded().semantic_work_limit - remaining;

        for cancel in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let revision = salsa::plumbing::current_revision(&db);
            let mut inner = if constraints {
                constrained_inner(&db, &env)
            } else {
                scalar_inner()
            };
            let progress = Progress {
                cancel_finalization: cancel && !constraints,
                cancel_constraints: cancel && constraints,
                ..Progress::default()
            };
            let policy = if cancel {
                funded()
            } else {
                AnalysisPolicy {
                    semantic_work_limit: retained_work,
                    ..funded()
                }
            };
            let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled(&prepared, Action::Inner(&mut inner), &policy, &progress)
            }));
            match outcome {
                Err(salsa::Cancelled::Local) if cancel => {}
                Ok(result) if !cancel => assert_eq!(
                    result,
                    Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: ()
                    })
                ),
                other => panic!("{other:?}"),
            }
            assert!(!progress.completed.get());
            assert_eq!(inner.signed_storage(InnerSign::Positive).len, 0);
            assert_eq!(inner.signed_storage(InnerSign::Negative).len, 0);
            assert_eq!(finalization_observations::progress().0, 0);
            assert_eq!(finalization_observations::progress().1, 1);
            assert_eq!(constraints_observations::progress().0, 0);
            assert_eq!(
                constraints_observations::progress().1,
                usize::from(constraints)
            );
            assert_no_active_attempt();

            let mut retry = if constraints {
                constrained_inner(&db, &env)
            } else {
                scalar_inner()
            };
            let result = controlled(
                &prepared,
                Action::Inner(&mut retry),
                &funded(),
                &Progress::default(),
            );
            let expected = if constraints {
                Type::Never
            } else {
                IntersectionBuilder::new(&db, &env)
                    .positive_elements([Type::unknown(), Type::AlwaysTruthy])
                    .build()
            };
            assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)));
            assert_eq!(finalization_observations::progress().0, 0);
            assert_eq!(constraints_observations::progress().0, 0);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn allocation_refusal_retires_finalization_storage_and_retries() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture();
        let prepared = prepare(&db);
        let mut inner = scalar_inner();
        let outcome = controlled(
            &prepared,
            Action::Inner(&mut inner),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &Progress::default(),
        );
        match outcome {
            Ok(AnalysisOutcome::Complete(_)) => upper = middle,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                ..
            }) => lower = middle + 1,
            other => panic!("{other:?}"),
        }
        assert_eq!(finalization_observations::progress().0, 0);
        assert_no_active_attempt();
    }
    assert!(upper > 0);
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut inner = scalar_inner();
    assert_eq!(
        controlled(
            &prepared,
            Action::Inner(&mut inner),
            &AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            &Progress::default()
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }),
    );
    assert_eq!(finalization_observations::progress().0, 0);
    assert_eq!(finalization_observations::progress().1, 1);
    assert_eq!(inner.signed_storage(InnerSign::Positive).len, 0);
    assert_no_active_attempt();
    let mut retry = scalar_inner();
    assert!(matches!(
        controlled(
            &prepared,
            Action::Inner(&mut retry),
            &funded(),
            &Progress::default()
        ),
        Ok(AnalysisOutcome::Complete(Type::Intersection(_))),
    ));
    assert_eq!(finalization_observations::progress().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
