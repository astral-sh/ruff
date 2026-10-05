use std::panic::AssertUnwindSafe;

use super::*;
use crate::FxOrderSet;
use crate::analysis::TruthinessOperation;
use crate::types::relation::source::retained::observations as retained_observations;
use crate::types::relation::source::{
    RelationSourceOperation, disjointness_condition, disjointness_condition_with_mode,
    disjointness_observations,
};
use crate::types::{GenericAlias, KnownClass, NegativeIntersectionElements};

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    first: Type<'db>,
    second: Type<'db>,
    perform_expensive_checks: Option<bool>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<bool>, AnalysisFailure> {
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
            match perform_expensive_checks {
                Some(perform_expensive_checks) => {
                    disjointness_condition_with_mode(
                        session.db(),
                        &env,
                        first,
                        second,
                        perform_expensive_checks,
                        &effects,
                    )
                    .await
                }
                None => disjointness_condition(session.db(), &env, first, second, &effects).await,
            }
        })
    })
}

#[test]
fn terminal_disjointness_matches_ordinary_with_fresh_owners() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let first_string = Type::string_literal(&db, "first");
    let second_string = Type::string_literal(&db, "second");
    for (first, second, expected) in [
        (Type::Never, Type::unknown(), true),
        (Type::unknown(), Type::Never, true),
        (Type::Never, Type::Never, true),
        (Type::unknown(), Type::AlwaysTruthy, false),
        (Type::AlwaysTruthy, Type::unknown(), false),
        (Type::unknown(), Type::AlwaysFalsy, false),
        (Type::AlwaysFalsy, Type::unknown(), false),
        (Type::bool_literal(false), Type::AlwaysTruthy, true),
        (Type::AlwaysTruthy, Type::bool_literal(false), true),
        (Type::bool_literal(false), Type::AlwaysFalsy, false),
        (Type::AlwaysFalsy, Type::bool_literal(false), false),
        (Type::bool_literal(true), Type::AlwaysFalsy, true),
        (Type::AlwaysFalsy, Type::bool_literal(true), true),
        (Type::bool_literal(true), Type::AlwaysTruthy, false),
        (Type::AlwaysTruthy, Type::bool_literal(true), false),
        (Type::literal_string(), Type::AlwaysTruthy, false),
        (Type::AlwaysTruthy, Type::literal_string(), false),
        (Type::literal_string(), Type::AlwaysFalsy, false),
        (Type::AlwaysFalsy, Type::literal_string(), false),
        (Type::bool_literal(true), Type::bool_literal(false), true),
        (Type::bool_literal(true), Type::bool_literal(true), false),
        (first_string, second_string, true),
        (first_string, first_string, false),
        (Type::literal_string(), first_string, false),
        (first_string, Type::literal_string(), false),
        (Type::literal_string(), Type::literal_string(), false),
    ] {
        for _ in 0..2 {
            disjointness_observations::reset(false);
            assert_eq!(
                controlled(&prepared, first, second, None, &funded()),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            let (live, entered, _) = disjointness_observations::progress();
            assert_eq!((live, entered), (0, 1));
            assert_eq!(first.is_disjoint_from(&db, &env, second), expected);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

fn intersection<'db>(db: &'db dyn Db, positive: &[Type<'db>], negative: &[Type<'db>]) -> Type<'db> {
    Type::Intersection(IntersectionType::new(
        db,
        FxOrderSet::from_iter(positive.iter().copied()),
        match negative {
            [] => NegativeIntersectionElements::Empty,
            [ty] => NegativeIntersectionElements::Single(*ty),
            _ => NegativeIntersectionElements::Multiple(FxOrderSet::from_iter(
                negative.iter().copied(),
            )),
        },
    ))
}

#[test]
fn intersection_disjointness_matches_ordinary_with_fresh_owners() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let yes = Type::bool_literal(true);
    let no = Type::bool_literal(false);
    let positive_yes = intersection(&db, &[yes], &[]);
    let positive_no = intersection(&db, &[no], &[]);
    let negative_yes = intersection(&db, &[Type::AlwaysTruthy], &[yes]);
    let negative_no = intersection(&db, &[Type::AlwaysTruthy], &[no]);
    let only_negative = intersection(&db, &[], &[yes]);
    for (first, second, expected) in [
        (positive_yes, no, true),
        (no, positive_yes, true),
        (positive_yes, positive_no, true),
        (negative_yes, yes, true),
        (yes, negative_yes, true),
        (only_negative, positive_yes, true),
        (negative_no, yes, false),
        (yes, negative_no, false),
        (positive_yes, positive_yes, false),
        (only_negative, only_negative, false),
    ] {
        for _ in 0..2 {
            disjointness_observations::reset(false);
            retained_observations::reset(None);
            assert_eq!(
                controlled(&prepared, first, second, None, &funded()),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            let (live, entered, _) = disjointness_observations::progress();
            assert_eq!((live, entered), (0, 1));
            let (live, entered, poll_depth) = retained_observations::progress();
            assert_eq!(live, 0);
            assert!(entered >= 1);
            assert_eq!(poll_depth, 1);
            assert_eq!(first.is_disjoint_from(&db, &env, second), expected);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn recursive_intersection_disjointness_keeps_flat_polling_and_one_root() {
    for depth in [1, 8, 32] {
        for expected in [false, true] {
            for intersection_on_left in [false, true] {
                let db = fixture();
                let prepared = prepare(&db);
                let env = ProgramEnvironment::from_file(prepared.program_file());
                let revision = salsa::plumbing::current_revision(&db);
                let mut nested = Type::bool_literal(true);
                for _ in 0..depth {
                    nested = intersection(&db, &[nested], &[]);
                }
                let other = Type::bool_literal(!expected);
                let (first, second) = if intersection_on_left {
                    (nested, other)
                } else {
                    (other, nested)
                };
                disjointness_observations::reset(false);
                retained_observations::reset(None);
                assert_eq!(
                    controlled(&prepared, first, second, None, &funded()),
                    Ok(AnalysisOutcome::Complete(expected)),
                    "depth={depth}, intersection_on_left={intersection_on_left}",
                );
                let (live, entered, _) = disjointness_observations::progress();
                assert_eq!((live, entered), (0, 1));
                assert_eq!(retained_observations::progress(), (0, depth + 1, 1));
                assert_eq!(first.is_disjoint_from(&db, &env, second), expected);
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
                assert_no_active_attempt();
            }
        }
    }
}

#[test]
fn interrupted_intersection_disjointness_retires_owners_and_retries_in_the_same_revision() {
    let measured = fixture();
    let measured_prepared = prepare(&measured);
    let first = intersection(&measured, &[Type::bool_literal(true)], &[]);
    let second = Type::bool_literal(false);
    disjointness_observations::reset(false);
    retained_observations::reset(None);
    assert_eq!(
        controlled(&measured_prepared, first, second, None, &funded()),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (live, entered, remaining) = disjointness_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let Some(remaining) = remaining else {
        panic!("disjointness owners were not observed");
    };
    let retained_work = funded().semantic_work_limit - remaining;
    assert_eq!(retained_observations::progress(), (0, 2, 1));
    assert_no_active_attempt();

    for cancel in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let first = intersection(&db, &[Type::bool_literal(true)], &[]);
        disjointness_observations::reset(cancel);
        retained_observations::reset(None);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, first, second, None, &policy)
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
        let (live, entered, remaining) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert!(remaining.is_some());
        assert_eq!(retained_observations::progress().0, 0);
        assert_no_active_attempt();

        disjointness_observations::reset(false);
        retained_observations::reset(None);
        assert_eq!(
            controlled(&prepared, first, second, None, &funded()),
            Ok(AnalysisOutcome::Complete(true)),
        );
        let (live, entered, _) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert_eq!(retained_observations::progress(), (0, 2, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn unsupported_disjointness_children_refuse_again_after_ordinary_evaluation() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let union = UnionType::from_elements(
        &db,
        &env,
        [Type::bool_literal(true), Type::string_literal(&db, "value")],
    );
    assert!(matches!(union, Type::Union(_)));
    let class = KnownClass::Int.to_class_literal(&db, &env);
    for (first, second, operation, expected) in [
        (
            class,
            Type::AlwaysTruthy,
            OperationId::Truthiness(TruthinessOperation::MetaclassInstance),
            false,
        ),
        (
            Type::AlwaysTruthy,
            class,
            OperationId::Truthiness(TruthinessOperation::MetaclassInstance),
            false,
        ),
        (
            class,
            Type::AlwaysFalsy,
            OperationId::Truthiness(TruthinessOperation::MetaclassInstance),
            false,
        ),
        (
            Type::AlwaysFalsy,
            class,
            OperationId::Truthiness(TruthinessOperation::MetaclassInstance),
            false,
        ),
        (
            union,
            Type::bool_literal(false),
            OperationId::Relation(RelationSourceOperation::DisjointUnion),
            true,
        ),
        (
            Type::bool_literal(false),
            union,
            OperationId::Relation(RelationSourceOperation::DisjointUnion),
            true,
        ),
    ] {
        for _ in 0..2 {
            disjointness_observations::reset(false);
            assert_eq!(
                controlled(&prepared, first, second, None, &funded()),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(operation),
                    completed: (),
                }),
            );
            let (live, entered, _) = disjointness_observations::progress();
            assert_eq!((live, entered), (0, 1));
            assert_eq!(first.is_disjoint_from(&db, &env, second), expected);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn disabling_expensive_disjointness_checks_skips_unsupported_children() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let union = UnionType::from_elements(
        &db,
        &env,
        [Type::bool_literal(true), Type::string_literal(&db, "value")],
    );
    assert!(matches!(union, Type::Union(_)));
    for (first, second, operation, expected) in [
        (
            union,
            Type::bool_literal(false),
            OperationId::Relation(RelationSourceOperation::DisjointUnion),
            true,
        ),
        (
            KnownClass::Int.to_class_literal(&db, &env),
            Type::AlwaysFalsy,
            OperationId::Truthiness(TruthinessOperation::MetaclassInstance),
            false,
        ),
    ] {
        assert_eq!(first.is_disjoint_from(&db, &env, second), expected);
        disjointness_observations::reset(false);
        assert_eq!(
            controlled(&prepared, first, second, Some(false), &funded()),
            Ok(AnalysisOutcome::Complete(false)),
        );
        let (live, entered, _) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert_no_active_attempt();
        disjointness_observations::reset(false);
        assert_eq!(
            controlled(&prepared, first, second, Some(true), &funded()),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(operation),
                completed: (),
            }),
        );
        let (live, entered, _) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert_no_active_attempt();
    }

    disjointness_observations::reset(false);
    assert_eq!(
        controlled(&prepared, Type::Never, union, Some(false), &funded()),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (live, entered, _) = disjointness_observations::progress();
    assert_eq!((live, entered), (0, 1));
    assert_no_active_attempt();
}

#[test]
fn generic_alias_origins_short_circuit_specialization_disjointness() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let alias = |known_class: KnownClass, argument| {
        let Type::ClassLiteral(ClassLiteral::Static(origin)) =
            known_class.to_class_literal(&db, &env)
        else {
            panic!("fixture class is not static");
        };
        let Some(context) = origin.generic_context(&db) else {
            panic!("fixture class is not generic");
        };
        Type::GenericAlias(GenericAlias::new(
            &db,
            origin,
            context.specialize(&db, [argument].as_slice()),
        ))
    };
    let list_int = alias(KnownClass::List, Type::int_literal(1));
    let list_bool = alias(KnownClass::List, Type::bool_literal(true));
    let set_int = alias(KnownClass::Set, Type::int_literal(1));
    let revision = salsa::plumbing::current_revision(&db);

    for (first, second, same_origin) in [
        (list_int, set_int, false),
        (set_int, list_int, false),
        (list_int, list_bool, true),
        (list_bool, list_int, true),
    ] {
        for perform_expensive_checks in [true, false] {
            for _ in 0..2 {
                disjointness_observations::reset(false);
                let expected = if same_origin && perform_expensive_checks {
                    unavailable(OperationId::Relation(
                        RelationSourceOperation::ClassSpecialization,
                    ))
                } else {
                    AnalysisOutcome::Complete(!same_origin)
                };
                assert_eq!(
                    controlled(
                        &prepared,
                        first,
                        second,
                        Some(perform_expensive_checks),
                        &funded(),
                    ),
                    Ok(expected),
                );
                let (live, entered, _) = disjointness_observations::progress();
                assert_eq!((live, entered), (0, 1));
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
                assert_no_active_attempt();
            }
        }
    }
}

#[test]
fn interrupted_disjointness_retires_owners_and_retries_in_the_same_revision() {
    let first = Type::literal_string();
    let second = Type::AlwaysTruthy;
    let measured = fixture();
    let measured_prepared = prepare(&measured);
    disjointness_observations::reset(false);
    assert_eq!(
        controlled(&measured_prepared, first, second, None, &funded()),
        Ok(AnalysisOutcome::Complete(false)),
    );
    let (live, entered, remaining) = disjointness_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let retained_work = funded().semantic_work_limit - remaining.unwrap();
    assert!(retained_work > 0);

    for cancel in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        disjointness_observations::reset(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, first, second, None, &policy)
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
        let (live, entered, remaining) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert!(remaining.is_some());
        assert_no_active_attempt();

        disjointness_observations::reset(false);
        assert_eq!(
            controlled(&prepared, first, second, None, &funded()),
            Ok(AnalysisOutcome::Complete(false)),
        );
        let (live, entered, _) = disjointness_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}
