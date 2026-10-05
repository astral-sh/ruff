use std::panic::AssertUnwindSafe;

use salsa::execution_probe::FinalSourceMemo;

use super::*;
use crate::analysis::TruthinessOperation;
use crate::types::relation::redundancy_ingredient;
use crate::types::relation::source::{disjointness_observations, redundancy_observations};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::set_theoretic::builder::{
    IntersectionPolarity, IntersectionSimplification, intersection_simplification_ingredient,
    simplify_intersection_pair,
};
use crate::types::{KnownClass, LiteralValueType};

#[derive(Clone, Copy)]
enum Entry {
    Comparison,
    Canonical,
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    first: Type<'db>,
    second: Type<'db>,
    polarity: IntersectionPolarity,
    entry: Entry,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<IntersectionSimplification>, AnalysisFailure> {
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
            match entry {
                Entry::Comparison => {
                    access
                        .simplify_intersection_pair(first, second, polarity)
                        .await
                }
                Entry::Canonical => {
                    access
                        .canonical_intersection_simplification(first, second, polarity)
                        .await
                }
            }
        })
    })
}

fn executed_key(db: &TestDb, events: &[salsa::Event]) -> salsa::Id {
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
        panic!("intersection simplification query did not execute");
    };
    key
}

#[test]
fn canonical_simplification_preserves_polarity_order_and_ordinary_reuse() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let mut keys = Vec::new();
    for (first, second, expected) in [
        (
            Type::Never,
            Type::unknown(),
            [
                IntersectionSimplification::SecondRedundant,
                IntersectionSimplification::FirstRedundant,
                IntersectionSimplification::Disjoint,
            ],
        ),
        (
            Type::unknown(),
            Type::Never,
            [
                IntersectionSimplification::Disjoint,
                IntersectionSimplification::SecondRedundant,
                IntersectionSimplification::SecondRedundant,
            ],
        ),
        (
            Type::AlwaysFalsy,
            Type::AlwaysTruthy,
            [
                IntersectionSimplification::Disjoint,
                IntersectionSimplification::Unchanged,
                IntersectionSimplification::SecondRedundant,
            ],
        ),
    ] {
        for (polarity, expected) in [
            IntersectionPolarity::Positive,
            IntersectionPolarity::Negative,
            IntersectionPolarity::Mixed,
        ]
        .into_iter()
        .zip(expected)
        {
            disjointness_observations::reset(false);
            events.take_salsa_events();
            assert_eq!(
                controlled(
                    &prepared,
                    first,
                    second,
                    polarity,
                    Entry::Canonical,
                    &funded()
                ),
                Ok(AnalysisOutcome::Complete(expected)),
                "{first:?}, {second:?}, {polarity:?}",
            );
            let key = executed_key(&db, &events.take_salsa_events());
            assert!(!keys.contains(&key));
            keys.push(key);
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    intersection_simplification_ingredient(&db),
                    key,
                )
                .is_ok()
            );
            if first == Type::unknown() && polarity == IntersectionPolarity::Positive {
                // Reverse redundancy is true, but disjointness still decides the result.
                let (live, entered, _) = disjointness_observations::progress();
                assert_eq!((live, entered), (0, 1));
            }

            events.take_salsa_events();
            assert_eq!(
                simplify_intersection_pair(&db, &env, first, second, polarity),
                expected,
            );
            for entry in [Entry::Comparison, Entry::Canonical] {
                assert_eq!(
                    controlled(&prepared, first, second, polarity, entry, &funded()),
                    Ok(AnalysisOutcome::Complete(expected)),
                );
            }
            assert_function_query_was_not_run_by_name(
                &db,
                "simplify_intersection_pair_impl",
                None,
                &events.take_salsa_events(),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
    assert_eq!(keys.len(), 9);
}

#[test]
fn literal_shortcuts_preserve_flags_without_pair_interning_or_queries() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let promotable = Type::bool_literal(true);
    let unpromotable = LiteralValueType::unpromotable(true);
    let recursive =
        Type::LiteralValue(unpromotable.with_recursively_defined(RecursivelyDefined::Yes));
    let unpromotable = Type::LiteralValue(unpromotable);
    let string = Type::string_literal(&db, "value");
    let other_string = Type::string_literal(&db, "other");
    let bytes = Type::bytes_literal(&db, b"value");
    for (first, second, polarity, expected) in [
        (
            unpromotable,
            promotable,
            IntersectionPolarity::Positive,
            IntersectionSimplification::FirstRedundant,
        ),
        (
            promotable,
            unpromotable,
            IntersectionPolarity::Positive,
            IntersectionSimplification::SecondRedundant,
        ),
        (
            unpromotable,
            unpromotable,
            IntersectionPolarity::Positive,
            IntersectionSimplification::SecondRedundant,
        ),
        (
            unpromotable,
            recursive,
            IntersectionPolarity::Positive,
            IntersectionSimplification::Unchanged,
        ),
        (
            unpromotable,
            recursive,
            IntersectionPolarity::Negative,
            IntersectionSimplification::SecondRedundant,
        ),
        (
            Type::int_literal(1),
            promotable,
            IntersectionPolarity::Positive,
            IntersectionSimplification::Disjoint,
        ),
        (
            string,
            string,
            IntersectionPolarity::Mixed,
            IntersectionSimplification::Disjoint,
        ),
        (
            string,
            other_string,
            IntersectionPolarity::Mixed,
            IntersectionSimplification::SecondRedundant,
        ),
        (
            bytes,
            string,
            IntersectionPolarity::Negative,
            IntersectionSimplification::Unchanged,
        ),
    ] {
        redundancy_observations::reset(false);
        disjointness_observations::reset(false);
        events.take_salsa_events();
        assert_eq!(
            controlled(
                &prepared,
                first,
                second,
                polarity,
                Entry::Comparison,
                &funded()
            ),
            Ok(AnalysisOutcome::Complete(expected)),
            "{first:?}, {second:?}, {polarity:?}",
        );
        assert_eq!(
            simplify_intersection_pair(&db, &env, first, second, polarity),
            expected,
        );
        assert_eq!(redundancy_observations::progress(), (0, 0, None));
        assert_eq!(disjointness_observations::progress(), (0, 0, None));
        let events = events.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "simplify_intersection_pair_impl",
            None,
            &events,
        );
        for event in &events {
            if let salsa::EventKind::DidInternValue { key, .. } = event.kind {
                assert_ne!(db.ingredient_debug_name(key.ingredient_index()), "TypePair");
            }
        }
        assert_no_active_attempt();
    }
}

#[test]
fn unavailable_relation_child_leaves_simplification_unpublished() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let first = KnownClass::Int.to_class_literal(&db, &env);
    let second = Type::AlwaysTruthy;
    let polarity = IntersectionPolarity::Positive;
    for _ in 0..2 {
        redundancy_observations::reset(false);
        events.take_salsa_events();
        assert_eq!(
            controlled(
                &prepared,
                first,
                second,
                polarity,
                Entry::Comparison,
                &funded()
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(OperationId::Truthiness(
                    TruthinessOperation::MetaclassInstance,
                )),
                completed: (),
            }),
        );
        let key = executed_key(&db, &events.take_salsa_events());
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_simplification_ingredient(&db),
                key,
            )
            .is_err()
        );
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert_no_active_attempt();
    }

    assert_eq!(
        simplify_intersection_pair(&db, &env, first, second, polarity),
        IntersectionSimplification::Unchanged,
    );
    events.take_salsa_events();
    assert_eq!(
        controlled(
            &prepared,
            first,
            second,
            polarity,
            Entry::Comparison,
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(
            IntersectionSimplification::Unchanged
        )),
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "simplify_intersection_pair_impl",
        None,
        &events.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn interrupted_simplification_retires_owners_and_retries_in_the_same_revision() {
    let first = Type::AlwaysFalsy;
    let second = Type::AlwaysTruthy;
    let polarity = IntersectionPolarity::Positive;
    let measured = fixture();
    let measured_prepared = prepare(&measured);
    disjointness_observations::reset(false);
    assert_eq!(
        controlled(
            &measured_prepared,
            first,
            second,
            polarity,
            Entry::Comparison,
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(
            IntersectionSimplification::Disjoint
        )),
    );
    let (live, entered, remaining) = disjointness_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let Some(remaining) = remaining else {
        panic!("disjointness owners did not record the remaining work");
    };
    let retained_work = funded().semantic_work_limit - remaining;
    assert!(retained_work > 0);

    for cancel in [false, true] {
        let db = fixture();
        let mut events = db.clone();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        redundancy_observations::reset(false);
        disjointness_observations::reset(cancel);
        events.take_salsa_events();
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                first,
                second,
                polarity,
                Entry::Comparison,
                &policy,
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
            other => panic!("{other:?}"),
        }
        let key = executed_key(&db, &events.take_salsa_events());
        // Immediate-fallback queries defer local cancellation through completion. The completed
        // result remains reusable when cancellation reaches the caller; work refusal publishes nothing.
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_simplification_ingredient(&db),
                key,
            )
            .is_ok(),
            cancel,
            "cancel={cancel}",
        );
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 2));
        let (live, entered, remaining) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert!(remaining.is_some());
        assert_no_active_attempt();

        // Both redundancy queries finished before the interrupted disjointness check.
        for (first, second) in [(first, second), (second, first)] {
            let pair = TypePair::new(&db, prepared.program_file().program(&db), first, second);
            assert!(
                FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                    .is_ok()
            );
        }
        redundancy_observations::reset(false);
        disjointness_observations::reset(false);
        events.take_salsa_events();
        assert_eq!(
            controlled(
                &prepared,
                first,
                second,
                polarity,
                Entry::Comparison,
                &funded()
            ),
            Ok(AnalysisOutcome::Complete(
                IntersectionSimplification::Disjoint
            )),
        );
        let retry_events = events.take_salsa_events();
        if cancel {
            assert_function_query_was_not_run_by_name(
                &db,
                "simplify_intersection_pair_impl",
                None,
                &retry_events,
            );
        } else {
            assert_eq!(executed_key(&db, &retry_events), key);
        }
        assert_eq!(redundancy_observations::progress(), (0, 0, None));
        let (live, entered, _) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, usize::from(!cancel)));
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
