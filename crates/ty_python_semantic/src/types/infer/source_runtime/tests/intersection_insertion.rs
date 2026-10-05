use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::FxOrderSet;
use crate::types::relation::redundancy_ingredient;
use crate::types::relation::source::retained::observations as retained_observations;
use crate::types::relation::source::{
    disjointness_observations, guard_observations, redundancy_observations,
};
use crate::types::set_theoretic::builder::intersection_insertion::{
    InsertionFacts, OrdinaryInsertionEffects, Sign, add_sync, insertion_observations,
};
use crate::types::set_theoretic::builder::{
    InnerIntersectionBuilder, intersection_simplification_ingredient,
};
use crate::types::{NegativeIntersectionElements, TypeFormType, TypeGuardType, todo_type};

#[derive(Default)]
struct Progress {
    completed_insertions: Cell<usize>,
    remaining_after_first: Cell<Option<usize>>,
    remaining_after_last: Cell<Option<usize>>,
    dropped_entries: Cell<Option<[usize; 2]>>,
    retired: Cell<bool>,
}

struct OwnedBuilder<'a, 'db> {
    builder: Option<InnerIntersectionBuilder<'db>>,
    progress: &'a Progress,
}

impl Drop for OwnedBuilder<'_, '_> {
    fn drop(&mut self) {
        if let Some(builder) = self.builder.take() {
            let entries = [
                builder.signed_storage(Sign::Positive).len,
                builder.signed_storage(Sign::Negative).len,
            ];
            drop(builder);
            self.progress.dropped_entries.set(Some(entries));
        }
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    additions: &[(Type<'db>, Sign)],
    retire: bool,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Option<InnerIntersectionBuilder<'db>>>, AnalysisFailure> {
    insertion_observations::reset();
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let effects = SourceEffects::new(&access, session.program());
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let mut owner = OwnedBuilder {
                builder: Some(effects.new_inner_intersection().await?),
                progress,
            };
            for &(ty, sign) in additions {
                let Some(builder) = &mut owner.builder else {
                    return Err(RunError::Contract("insertion fixture lost its builder"));
                };
                effects
                    .inner_intersection_add(&env, builder, ty, sign)
                    .await?;
                progress
                    .completed_insertions
                    .set(progress.completed_insertions.get() + 1);
                let remaining =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db());
                if progress.completed_insertions.get() == 1 {
                    progress.remaining_after_first.set(remaining);
                }
                progress.remaining_after_last.set(remaining);
            }
            let Some(builder) = owner.builder.take() else {
                return Err(RunError::Contract("insertion fixture lost its builder"));
            };
            if retire {
                effects.retire_inner_intersection(builder).await?;
                progress.retired.set(true);
                Ok(None)
            } else {
                Ok(Some(builder))
            }
        })
    })
}

fn ordinary<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    additions: &[(Type<'db>, Sign)],
) -> InnerIntersectionBuilder<'db> {
    let mut builder = InnerIntersectionBuilder::default();
    let effects = OrdinaryInsertionEffects::new(db, env);
    for &(ty, sign) in additions {
        match add_sync(&mut builder, ty, sign, InsertionFacts, &effects) {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }
    builder
}

fn simplification_key(db: &TestDb, events: &[salsa::Event]) -> salsa::Id {
    let Some(key) = events.iter().find_map(|event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind
            && db.ingredient_debug_name(database_key.ingredient_index())
                == "simplify_intersection_pair_impl"
        {
            Some(database_key.key_index())
        } else {
            None
        }
    }) else {
        panic!("insertion did not execute its simplification child");
    };
    key
}

fn pair() -> [(Type<'static>, Sign); 2] {
    [
        (Type::unknown(), Sign::Positive),
        (Type::AlwaysTruthy, Sign::Positive),
    ]
}

fn type_guard_pair(db: &TestDb, nesting: usize) -> [(Type<'_>, Sign); 2] {
    let mut first = Type::Never;
    let mut second = Type::bool_literal(true);
    for _ in 0..nesting {
        first = Type::TypeGuard(TypeGuardType::new(db, first, None));
        second = Type::TypeGuard(TypeGuardType::new(db, second, None));
    }
    [(first, Sign::Positive), (second, Sign::Positive)]
}

fn type_guard_literal_fallback_pair(db: &TestDb) -> [(Type<'_>, Sign); 2] {
    [
        (
            Type::TypeGuard(TypeGuardType::new(db, Type::bool_literal(true), None)),
            Sign::Positive,
        ),
        (
            Type::TypeGuard(TypeGuardType::new(
                db,
                Type::TypeForm(TypeFormType::new(db, Type::bool_literal(false))),
                None,
            )),
            Sign::Positive,
        ),
    ]
}

fn type_guard_type_form_pair(db: &TestDb) -> [(Type<'_>, Sign); 2] {
    [true, false].map(|value| {
        (
            Type::TypeGuard(TypeGuardType::new(
                db,
                Type::TypeForm(TypeFormType::new(db, Type::bool_literal(value))),
                None,
            )),
            Sign::Positive,
        )
    })
}

#[test]
fn signed_atomic_insertions_match_the_ordinary_builder() {
    for truthiness in [Type::AlwaysTruthy, Type::AlwaysFalsy] {
        for first_sign in [Sign::Positive, Sign::Negative] {
            for second_sign in [Sign::Positive, Sign::Negative] {
                for reversed in [false, true] {
                    let db = fixture();
                    let prepared = prepare(&db);
                    let env = ProgramEnvironment::from_file(prepared.program_file());
                    let mut additions = [(Type::unknown(), first_sign), (truthiness, second_sign)];
                    if reversed {
                        additions.reverse();
                    }
                    let progress = Progress::default();
                    let result = controlled(&prepared, &additions, false, &funded(), &progress);
                    assert_eq!(progress.completed_insertions.get(), additions.len());
                    assert_eq!(insertion_observations::progress(), (0, 2));
                    // Ordinary evaluation follows the controlled run so it cannot supply cached children.
                    assert_eq!(
                        result,
                        Ok(AnalysisOutcome::Complete(Some(ordinary(
                            &db, &env, &additions
                        )))),
                    );
                    assert_no_active_attempt();
                }
            }
        }
    }
}

#[test]
fn nested_negative_intersections_traverse_both_fields_and_spill_continuations() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut intersection = IntersectionType::new(
        &db,
        FxOrderSet::from_iter([Type::unknown()]),
        NegativeIntersectionElements::Single(Type::AlwaysTruthy),
    );
    // Each nested intersection keeps its enclosing Sequence pending. Three nested
    // sequences exceed the insertion owner's two inline frame slots before a leaf runs.
    for _ in 0..2 {
        intersection = IntersectionType::new(
            &db,
            FxOrderSet::from_iter([Type::Intersection(intersection)]),
            NegativeIntersectionElements::Empty,
        );
    }
    let additions = [(Type::Intersection(intersection), Sign::Negative)];
    let progress = Progress::default();
    let result = controlled(&prepared, &additions, false, &funded(), &progress);
    assert_eq!(progress.completed_insertions.get(), 1);
    assert_eq!(insertion_observations::progress(), (0, 1));
    let expected = ordinary(&db, &env, &additions);
    assert_eq!(expected.signed_storage(Sign::Positive).len, 2);
    assert_eq!(expected.signed_storage(Sign::Negative).len, 0);
    assert!(expected.contains_signed(Sign::Positive, Type::unknown()));
    assert!(expected.contains_signed(Sign::Positive, Type::AlwaysTruthy));
    assert_eq!(result, Ok(AnalysisOutcome::Complete(Some(expected))));
    assert_no_active_attempt();
}

#[test]
fn incoming_and_retained_inline_payloads_contribute_to_insertion_work() {
    let mut work = Vec::new();
    for dynamic in [
        todo_type!("short"),
        todo_type!(
            "retained intersection key with a long inline diagnostic payload that must contribute to both its initial hash and subsequent resident comparisons against smaller incoming dynamic keys"
        ),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let additions = [(dynamic, Sign::Positive), (Type::unknown(), Sign::Positive)];
        let progress = Progress::default();
        let result = controlled(&prepared, &additions, false, &funded(), &progress);
        assert_eq!(
            result,
            Ok(AnalysisOutcome::Complete(Some(ordinary(
                &db, &env, &additions
            )))),
        );
        let (Some(first), Some(last)) = (
            progress.remaining_after_first.get(),
            progress.remaining_after_last.get(),
        ) else {
            panic!("inline payload insertions did not complete");
        };
        work.push((funded().semantic_work_limit - first, first - last));
        assert_no_active_attempt();
    }
    if cfg!(debug_assertions) {
        assert!(
            work[1].0 > work[0].0,
            "incoming inline payload was not quoted"
        );
        assert!(
            work[1].1 > work[0].1,
            "retained inline payload was not quoted"
        );
    }
}

#[test]
fn insertion_publishes_and_reuses_canonical_simplification_children() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let additions = pair();
    redundancy_observations::reset(false);
    disjointness_observations::reset(false);
    events.take_salsa_events();
    let result = controlled(
        &prepared,
        &additions,
        false,
        &funded(),
        &Progress::default(),
    );
    assert!(matches!(result, Ok(AnalysisOutcome::Complete(Some(_)))));
    assert_eq!(insertion_observations::progress(), (0, 2));
    let key = simplification_key(&db, &events.take_salsa_events());
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            intersection_simplification_ingredient(&db),
            key
        )
        .is_ok()
    );
    assert_eq!(redundancy_observations::progress().0, 0);
    assert_eq!(redundancy_observations::progress().1, 2);
    assert_eq!(disjointness_observations::progress().0, 0);
    assert_eq!(disjointness_observations::progress().1, 1);
    for (first, second) in [
        (Type::unknown(), Type::AlwaysTruthy),
        (Type::AlwaysTruthy, Type::unknown()),
    ] {
        let key = TypePair::new(&db, env.program(&db), first, second);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), key.as_id())
                .is_ok()
        );
    }

    events.take_salsa_events();
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Complete(Some(ordinary(
            &db, &env, &additions
        )))),
    );
    redundancy_observations::reset(false);
    disjointness_observations::reset(false);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, &additions, true, &funded(), &progress),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert!(progress.retired.get());
    assert_eq!(insertion_observations::progress(), (0, 2));
    let events = events.take_salsa_events();
    for query in ["simplify_intersection_pair_impl", "is_redundant_with_impl"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(redundancy_observations::progress(), (0, 0, None));
    assert_eq!(disjointness_observations::progress(), (0, 0, None));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn type_guard_fields_publish_and_reuse_canonical_simplification_children() {
    for nesting in [1, 2] {
        let oracle = fixture();
        let oracle_env = oracle.program_environment();
        let oracle_additions = type_guard_pair(&oracle, nesting);
        let expected = ordinary(&oracle, &oracle_env, &oracle_additions);
        assert_eq!(expected.signed_storage(Sign::Positive).len, 1);
        assert!(expected.contains_signed(Sign::Positive, oracle_additions[0].0));
        assert_eq!(expected.signed_storage(Sign::Negative).len, 0);

        let db = fixture();
        let mut events = db.clone();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        let additions = type_guard_pair(&db, nesting);
        let pair = TypePair::new(&db, env.program(&db), additions[0].0, additions[1].0);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_err()
        );
        redundancy_observations::reset(false);
        events.take_salsa_events();
        let progress = Progress::default();
        let cold = capture(&db, || {
            controlled(&prepared, &additions, false, &funded(), &progress)
        })
        .unwrap();
        assert_eq!(cold.check_root_reads(), Ok(()));
        let Ok(AnalysisOutcome::Complete(Some(builder))) = &cold.value else {
            panic!("TypeGuard insertion did not complete: {:?}", cold.value);
        };
        // Covariance retains the first TypeGuard because Never is narrower than True.
        assert_eq!(builder.signed_storage(Sign::Positive).len, 1);
        assert!(builder.contains_signed(Sign::Positive, additions[0].0));
        assert_eq!(builder.signed_storage(Sign::Negative).len, 0);
        assert_eq!(progress.completed_insertions.get(), 2);
        assert_eq!(insertion_observations::progress(), (0, 2));
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 1));
        let key = simplification_key(&db, &events.take_salsa_events());
        let ingredient = intersection_simplification_ingredient(&db);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).is_ok());
        let memo_key = ingredient.database_key_index(key);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_ok()
        );
        let cold_address = cold
            .reads
            .iter()
            .find(|read| read.key == memo_key && read.parent.is_none())
            .map(|read| read.memo_address);
        assert!(cold_address.is_some());

        events.take_salsa_events();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Complete(Some(ordinary(
                &db, &env, &additions
            )))),
        );
        redundancy_observations::reset(false);
        let warm = capture(&db, || {
            controlled(
                &prepared,
                &additions,
                false,
                &funded(),
                &Progress::default(),
            )
        })
        .unwrap();
        assert_eq!(warm.value, cold.value);
        assert_eq!(warm.check_root_reads(), Ok(()));
        assert_eq!(
            warm.reads
                .iter()
                .find(|read| read.key == memo_key && read.parent.is_none())
                .map(|read| read.memo_address),
            cold_address,
        );
        let events = events.take_salsa_events();
        for query in ["simplify_intersection_pair_impl", "is_redundant_with_impl"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(redundancy_observations::progress(), (0, 0, None));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn type_guard_literal_fallback_publishes_and_reuses_canonical_children() {
    let oracle = fixture();
    let expected = ordinary(
        &oracle,
        &oracle.program_environment(),
        &type_guard_literal_fallback_pair(&oracle),
    );
    assert_eq!(expected.signed_storage(Sign::Positive).len, 1);
    assert!(expected.contains_signed(Sign::Positive, Type::Never));
    assert_eq!(expected.signed_storage(Sign::Negative).len, 0);

    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let additions = type_guard_literal_fallback_pair(&db);
    events.take_salsa_events();
    let cold = capture(&db, || {
        controlled(
            &prepared,
            &additions,
            false,
            &funded(),
            &Progress::default(),
        )
    })
    .unwrap();
    assert_eq!(cold.check_root_reads(), Ok(()));
    let Ok(AnalysisOutcome::Complete(Some(builder))) = &cold.value else {
        panic!(
            "TypeGuard literal fallback did not complete: {:?}",
            cold.value
        );
    };
    assert_eq!(builder.signed_storage(Sign::Positive).len, 1);
    assert!(builder.contains_signed(Sign::Positive, Type::Never));
    assert_eq!(builder.signed_storage(Sign::Negative).len, 0);
    let cold_events = events.take_salsa_events();
    let key = simplification_key(&db, &cold_events);
    let ingredient = intersection_simplification_ingredient(&db);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).is_ok());
    let memo_key = ingredient.database_key_index(key);
    let root = cold
        .reads
        .iter()
        .find(|read| read.key == memo_key && read.parent.is_none())
        .expect("TypeGuard insertion reads its canonical simplification");
    let fallback = cold
        .reads
        .iter()
        .find(|read| {
            db.ingredient_debug_name(read.key.ingredient_index()) == "known_class_to_instance"
        })
        .expect("the literal fallback reads its canonical known-class instance");
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            known_class_to_instance_ingredient(&db),
            fallback.key.key_index(),
        )
        .is_ok()
    );
    for (first, second) in [
        (additions[0].0, additions[1].0),
        (additions[1].0, additions[0].0),
    ] {
        let pair = TypePair::new(&db, env.program(&db), first, second);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_ok()
        );
    }

    events.take_salsa_events();
    assert_eq!(
        cold.value,
        Ok(AnalysisOutcome::Complete(Some(ordinary(
            &db, &env, &additions
        )))),
    );
    let warm = capture(&db, || {
        controlled(
            &prepared,
            &additions,
            false,
            &funded(),
            &Progress::default(),
        )
    })
    .unwrap();
    assert_eq!(warm.value, cold.value);
    assert_eq!(warm.check_root_reads(), Ok(()));
    assert!(
        warm.reads
            .iter()
            .any(|read| { read.key == memo_key && read.memo_address == root.memo_address })
    );
    let events = events.take_salsa_events();
    for query in [
        "simplify_intersection_pair_impl",
        "is_redundant_with_impl",
        "known_class_to_instance",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn type_guard_relation_guard_publishes_and_reuses_canonical_children() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let additions = type_guard_type_form_pair(&db);
    let pairs = [
        TypePair::new(&db, env.program(&db), additions[0].0, additions[1].0),
        TypePair::new(&db, env.program(&db), additions[1].0, additions[0].0),
    ];
    for pair in pairs {
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
    }
    redundancy_observations::reset(false);
    disjointness_observations::reset(false);
    guard_observations::reset(false);
    retained_observations::reset(None);
    events.take_salsa_events();
    let progress = Progress::default();
    let cold = capture(&db, || {
        controlled(&prepared, &additions, false, &funded(), &progress)
    })
    .unwrap();
    assert_eq!(cold.check_root_reads(), Ok(()));
    let Ok(AnalysisOutcome::Complete(Some(builder))) = &cold.value else {
        panic!(
            "guarded TypeForm insertion did not complete: {:?}",
            cold.value
        );
    };
    assert_eq!(builder.signed_storage(Sign::Positive).len, 1);
    assert!(builder.contains_signed(Sign::Positive, Type::Never));
    assert_eq!(builder.signed_storage(Sign::Negative).len, 0);
    assert_eq!(progress.completed_insertions.get(), 2);
    assert_eq!(progress.dropped_entries.get(), None);
    assert!(!progress.retired.get());
    assert_eq!(insertion_observations::progress(), (0, 2));
    let (live, entered, _) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 2));
    let (live, entered, _) = disjointness_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let (entered, remaining, active) = guard_observations::progress();
    assert_eq!((entered, active), (2, 1));
    assert!(remaining.is_some());
    let (live, entered, polling) = retained_observations::progress();
    assert_eq!((live, polling), (0, 1));
    assert!(entered > 0);
    let cold_events = events.take_salsa_events();
    let key = simplification_key(&db, &cold_events);
    let ingredient = intersection_simplification_ingredient(&db);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).is_ok());
    let memo_key = ingredient.database_key_index(key);
    let root_address = cold
        .reads
        .iter()
        .find(|read| read.key == memo_key && read.parent.is_none())
        .map(|read| read.memo_address);
    assert!(root_address.is_some());
    for pair in pairs {
        let ingredient = redundancy_ingredient(&db);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, pair.as_id()).is_ok());
        let key = ingredient.database_key_index(pair.as_id());
        assert!(cold.reads.iter().any(|read| read.key == key));
        assert!(cold_events.iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
        }));
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    assert_eq!(
        cold.value,
        Ok(AnalysisOutcome::Complete(Some(ordinary(
            &db, &env, &additions
        )))),
    );

    for retire in [false, true] {
        redundancy_observations::reset(false);
        disjointness_observations::reset(false);
        guard_observations::reset(false);
        retained_observations::reset(None);
        events.take_salsa_events();
        let progress = Progress::default();
        let warm = capture(&db, || {
            controlled(&prepared, &additions, retire, &funded(), &progress)
        })
        .unwrap();
        if retire {
            assert_eq!(warm.value, Ok(AnalysisOutcome::Complete(None)));
        } else {
            assert_eq!(warm.value, cold.value);
        }
        assert_eq!(warm.check_root_reads(), Ok(()));
        assert_eq!(
            warm.reads
                .iter()
                .find(|read| read.key == memo_key && read.parent.is_none())
                .map(|read| read.memo_address),
            root_address,
        );
        assert_eq!(progress.completed_insertions.get(), 2);
        assert_eq!(progress.retired.get(), retire);
        assert_eq!(progress.dropped_entries.get(), None);
        assert_eq!(insertion_observations::progress(), (0, 2));
        let warm_events = events.take_salsa_events();
        for query in ["simplify_intersection_pair_impl", "is_redundant_with_impl"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &warm_events);
        }
        assert_eq!(redundancy_observations::progress(), (0, 0, None));
        assert_eq!(disjointness_observations::progress(), (0, 0, None));
        assert_eq!(guard_observations::progress(), (0, None, 0));
        assert_eq!(retained_observations::progress(), (0, 0, 0));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn type_guard_guard_finish_refusal_and_cancellation_allow_same_revision_retry() {
    let measured = fixture();
    let prepared = prepare(&measured);
    guard_observations::reset(false);
    redundancy_observations::reset(false);
    disjointness_observations::reset(false);
    retained_observations::reset(None);
    assert!(matches!(
        controlled(
            &prepared,
            &type_guard_type_form_pair(&measured),
            false,
            &funded(),
            &Progress::default(),
        ),
        Ok(AnalysisOutcome::Complete(Some(_))),
    ));
    let (entered, remaining, active) = guard_observations::progress();
    assert_eq!((entered, active), (2, 1));
    let Some(remaining) = remaining else {
        panic!("guarded TypeForm insertion did not reach guard finish");
    };
    let completed_child_work = funded().semantic_work_limit - remaining;

    for cancel in [false, true] {
        let db = fixture();
        let mut events = db.clone();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        let additions = type_guard_type_form_pair(&db);
        guard_observations::reset(cancel);
        redundancy_observations::reset(false);
        disjointness_observations::reset(false);
        retained_observations::reset(None);
        events.take_salsa_events();
        let progress = Progress::default();
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: completed_child_work,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, &additions, false, &policy, &progress)
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            other => panic!("cancel={cancel}: {other:?}"),
        }
        assert_eq!(progress.completed_insertions.get(), 1);
        assert_eq!(progress.dropped_entries.get(), Some([1, 0]));
        assert!(!progress.retired.get());
        assert_eq!(insertion_observations::progress(), (0, 2));
        let (entered, remaining, active) = guard_observations::progress();
        assert!(entered > 0);
        assert_eq!(active, 1);
        assert!(remaining.is_some());
        if !cancel {
            assert_eq!(entered, 1);
            assert_eq!(remaining, Some(0));
        }
        assert_eq!(redundancy_observations::progress().0, 0);
        assert_eq!(disjointness_observations::progress().0, 0);
        let (live, entered, polling) = retained_observations::progress();
        assert_eq!((live, polling), (0, 1));
        assert!(entered > 0);
        let key = simplification_key(&db, &events.take_salsa_events());
        if !cancel {
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    intersection_simplification_ingredient(&db),
                    key,
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
            for (first, second) in [
                (additions[0].0, additions[1].0),
                (additions[1].0, additions[0].0),
            ] {
                let pair = TypePair::new(&db, env.program(&db), first, second);
                assert_eq!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        redundancy_ingredient(&db),
                        pair.as_id(),
                    )
                    .map(|_| ()),
                    Err(FinalSourceError::MissingMemo),
                );
            }
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();

        guard_observations::reset(false);
        redundancy_observations::reset(false);
        disjointness_observations::reset(false);
        retained_observations::reset(None);
        let retry = Progress::default();
        let result = controlled(&prepared, &additions, false, &funded(), &retry);
        assert_eq!(retry.completed_insertions.get(), 2);
        assert_eq!(retry.dropped_entries.get(), None);
        assert_eq!(insertion_observations::progress(), (0, 2));
        assert_eq!(redundancy_observations::progress().0, 0);
        assert_eq!(disjointness_observations::progress().0, 0);
        assert_eq!(retained_observations::progress().0, 0);
        assert_eq!(
            result,
            Ok(AnalysisOutcome::Complete(Some(ordinary(
                &db, &env, &additions
            )))),
        );
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_simplification_ingredient(&db),
                key,
            )
            .is_ok()
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn type_guard_work_refusal_retires_the_retained_insertion_before_retry() {
    let measured = fixture();
    let prepared = prepare(&measured);
    redundancy_observations::reset(false);
    assert_eq!(
        controlled(
            &prepared,
            &type_guard_pair(&measured, 2),
            true,
            &funded(),
            &Progress::default(),
        ),
        Ok(AnalysisOutcome::Complete(None)),
    );
    let (live, entered, remaining) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let Some(remaining) = remaining else {
        panic!("TypeGuard insertion did not retain its relation owners");
    };

    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let additions = type_guard_pair(&db, 2);
    let pair = TypePair::new(
        &db,
        prepared.program_file().program(&db),
        additions[0].0,
        additions[1].0,
    );
    redundancy_observations::reset(false);
    events.take_salsa_events();
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            &additions,
            true,
            &AnalysisPolicy {
                semantic_work_limit: funded().semantic_work_limit - remaining,
                ..funded()
            },
            &progress,
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    assert_eq!(progress.completed_insertions.get(), 1);
    assert_eq!(progress.dropped_entries.get(), Some([1, 0]));
    assert!(!progress.retired.get());
    assert_eq!(insertion_observations::progress(), (0, 2));
    let (live, entered, _) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let key = simplification_key(&db, &events.take_salsa_events());
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            intersection_simplification_ingredient(&db),
            key,
        )
        .is_err()
    );
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id()).is_err()
    );
    assert_no_active_attempt();

    redundancy_observations::reset(false);
    let retry = Progress::default();
    assert_eq!(
        controlled(&prepared, &additions, true, &funded(), &retry),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert_eq!(retry.completed_insertions.get(), 2);
    assert!(retry.retired.get());
    assert_eq!(insertion_observations::progress(), (0, 2));
    let (live, entered, _) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id()).is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn work_refusal_and_native_cancellation_retire_retained_insertions() {
    let measured = fixture();
    let prepared = prepare(&measured);
    disjointness_observations::reset(false);
    assert_eq!(
        controlled(&prepared, &pair(), true, &funded(), &Progress::default()),
        Ok(AnalysisOutcome::Complete(None)),
    );
    let (live, entered, remaining) = disjointness_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let Some(remaining) = remaining else {
        panic!("insertion did not retain disjointness owners");
    };
    let retained_work = funded().semantic_work_limit - remaining;

    for cancel in [false, true] {
        let db = fixture();
        let mut events = db.clone();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        disjointness_observations::reset(cancel);
        events.take_salsa_events();
        let progress = Progress::default();
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, &pair(), true, &policy, &progress)
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            other => panic!("{other:?}"),
        }
        assert_eq!(progress.completed_insertions.get(), 1);
        assert_eq!(progress.dropped_entries.get(), Some([1, 0]));
        assert!(!progress.retired.get());
        assert_eq!(insertion_observations::progress(), (0, 2));
        let key = simplification_key(&db, &events.take_salsa_events());
        // Local cancellation reaches this caller after its immediate-fallback child completes.
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_simplification_ingredient(&db),
                key
            )
            .is_ok(),
            cancel,
        );
        let (live, entered, remaining) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert!(remaining.is_some());
        assert_no_active_attempt();

        disjointness_observations::reset(false);
        let retry = Progress::default();
        assert_eq!(
            controlled(&prepared, &pair(), true, &funded(), &retry),
            Ok(AnalysisOutcome::Complete(None)),
        );
        assert_eq!(retry.completed_insertions.get(), 2);
        assert!(retry.retired.get());
        assert_eq!(insertion_observations::progress(), (0, 2));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn allocation_refusal_after_retention_allows_a_fresh_invocation_quota() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        // Each probe starts cold; completed children from an earlier limit cannot reduce its cost.
        let db = fixture();
        let prepared = prepare(&db);
        let outcome = controlled(
            &prepared,
            &pair(),
            true,
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &Progress::default(),
        );
        match outcome {
            Ok(AnalysisOutcome::Complete(None)) => upper = middle,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                completed: (),
            }) => lower = middle + 1,
            other => panic!("{other:?}"),
        }
        assert_no_active_attempt();
    }
    assert!(upper > 0);
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            &pair(),
            true,
            &AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            &progress,
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: (),
        }),
    );
    assert!(progress.completed_insertions.get() >= 1);
    let Some([positive, negative]) = progress.dropped_entries.get() else {
        panic!("allocation refusal did not retire its retained builder");
    };
    assert!(positive >= 1);
    assert_eq!(negative, 0);
    assert!(!progress.retired.get());
    assert_eq!(insertion_observations::progress(), (0, 2));
    assert_no_active_attempt();

    let retry = Progress::default();
    assert_eq!(
        controlled(&prepared, &pair(), true, &funded(), &retry),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert_eq!(retry.completed_insertions.get(), 2);
    assert!(retry.retired.get());
    assert_eq!(insertion_observations::progress(), (0, 2));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn unavailable_semantic_children_do_not_complete_insertion() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    for (second, operation) in [
        (Type::bool_literal(true), OperationId::ClassSelection),
        (
            Type::TypeForm(TypeFormType::new(&db, Type::unknown())),
            OperationId::Narrowing,
        ),
    ] {
        let additions = [(Type::unknown(), Sign::Positive), (second, Sign::Positive)];
        for _ in 0..2 {
            let progress = Progress::default();
            assert_eq!(
                controlled(&prepared, &additions, true, &funded(), &progress),
                Ok(unavailable(operation)),
            );
            assert_eq!(progress.completed_insertions.get(), 1);
            assert_eq!(progress.dropped_entries.get(), Some([1, 0]));
            assert!(!progress.retired.get());
            assert_eq!(insertion_observations::progress(), (0, 2));
            assert_no_active_attempt();
        }
    }
    assert_eq!(
        controlled(&prepared, &pair(), true, &funded(), &Progress::default()),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
