use super::*;
use crate::reachability::NarrowingProjector;
use crate::reachability::narrowing_construction::NarrowingConstructionEffects;
use crate::types::NarrowingConstraint;
use crate::types::narrow::expression_narrowing_constraints_ingredient;
use crate::types::narrow::infer_narrowing_constraints;
use salsa::execution_probe::{FinalSourceMemo, VerifyResult};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::ScopedPredicateId;

type Constraints<'db> = (
    Option<NarrowingConstraint<'db>>,
    Option<NarrowingConstraint<'db>>,
);

#[derive(Default)]
struct Progress {
    remaining: Cell<Option<usize>>,
    cancel: bool,
    previous_revision: Option<salsa::Revision>,
    verified: Cell<Option<VerifyResult>>,
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    projector: &mut NarrowingProjector<'_, 'db>,
    predicate: ScopedPredicateId,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Constraints<'db>>, AnalysisFailure> {
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
        let projector = &mut *projector;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let program = session.program();
            let effects = SourceEffects::new(&access, program);
            let constraints = effects.predicate_constraints(projector, predicate).await?;
            if let Some(previous) = progress.previous_revision {
                let expression = predicate_expression(projector.predicates[predicate]);
                let verified = access
                    .endpoint
                    .child_call(|| async {
                        access
                            .endpoint
                            .validate_callable(
                                &access.routes.expression_narrowing,
                                expression.as_id(),
                                previous,
                            )?
                            .await
                    })
                    .await;
                progress.verified.set(Some(verified));
            }
            progress
                .remaining
                .set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    session.db(),
                ));
            if progress.cancel {
                access
                    .endpoint
                    .local_call(|| {
                        session.db().cancellation_token().cancel();
                        access.endpoint.check_completion()
                    })
                    .await;
            }
            Ok(constraints)
        })
    })
}

fn projector<'map, 'db>(
    db: &'db dyn Db,
    prepared: &PreparedAnalysisFile<'db>,
    env: &'map ProgramEnvironment<'db>,
) -> (
    NarrowingProjector<'map, 'db>,
    ScopedPredicateId,
    Predicate<'db>,
) {
    projector_for_name(db, prepared, env, "value")
}

fn projector_for_name<'map, 'db>(
    db: &'db dyn Db,
    prepared: &PreparedAnalysisFile<'db>,
    env: &'map ProgramEnvironment<'db>,
    name: &str,
) -> (
    NarrowingProjector<'map, 'db>,
    ScopedPredicateId,
    Predicate<'db>,
) {
    let Some(Stmt::FunctionDef(function)) = prepared.parsed_module().syntax().body.first() else {
        panic!("fixture function");
    };
    let Some(Stmt::Return(statement)) = function.body.first() else {
        panic!("fixture return");
    };
    let expression = statement.value.as_deref().unwrap();
    let index = prepared.semantic_index();
    let scope = index.expression_scope_id(expression);
    let place = ScopedPlaceId::Symbol(index.place_table(scope).symbol_id(name).unwrap());
    let evaluator = index
        .use_def_map(scope)
        .narrowing_evaluator(ScopedNarrowingConstraint::ALWAYS_TRUE);
    let (predicate_id, predicate) = evaluator
        .predicates()
        .iter_enumerated()
        .find(|(id, _)| evaluator.predicate_narrowing_targets().contains(*id, place))
        .unwrap();
    let projector = NarrowingProjector::new(
        db,
        env,
        evaluator.narrowing_constraints(),
        evaluator.predicates(),
        evaluator.predicate_narrowing_targets(),
        place,
        Type::unknown(),
    );
    (projector, predicate_id, *predicate)
}

fn predicate_expression(predicate: Predicate<'_>) -> Expression<'_> {
    match predicate.node {
        PredicateNode::Expression(expression)
        | PredicateNode::Condition(expression)
        | PredicateNode::ChainedComparisonCondition(expression) => expression,
        _ => panic!("fixture expression predicate"),
    }
}

#[test]
fn cold_predicate_constraints_publish_both_polarities_and_reuse_the_canonical_query() {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "def choose(value):\n    return value and value and True\n",
    )
    .unwrap();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (mut projector, predicate_id, predicate) = projector(&db, &prepared, &env);
    let place = projector.place;
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let outcome = controlled(
        &prepared,
        &mut projector,
        predicate_id,
        &funded(),
        &Progress::default(),
    );
    let Ok(AnalysisOutcome::Complete(constraints)) = outcome else {
        panic!("cold predicate production did not complete: {outcome:?}");
    };
    assert!(constraints.0.is_some());
    assert!(constraints.1.is_some());
    assert_ne!(constraints.0, constraints.1);
    let expected = (
        Some(NarrowingConstraint::intersection(
            Type::AlwaysFalsy.negate(&db, &env),
        )),
        Some(NarrowingConstraint::intersection(
            Type::AlwaysTruthy.negate(&db, &env),
        )),
    );
    assert_eq!(
        constraints,
        if predicate.is_positive {
            expected
        } else {
            (expected.1, expected.0)
        }
    );
    events_db.take_salsa_events();
    assert_eq!(
        infer_narrowing_constraints(&db, predicate, place),
        constraints
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "all_narrowing_constraints_for_expression",
        None,
        &events_db.take_salsa_events(),
    );
    assert_eq!(
        controlled(
            &prepared,
            &mut projector,
            predicate_id,
            &funded(),
            &Progress::default()
        ),
        Ok(AnalysisOutcome::Complete(constraints)),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn predicate_constraint_interruptions_retry_in_the_same_revision() {
    let source = "def choose(value):\n    return value and value and True\n";
    let mut measured = setup_db();
    measured.write_file("src/main.py", source).unwrap();
    let measured_prepared = prepare(&measured);
    let env = ProgramEnvironment::from_file(measured_prepared.program_file());
    let (mut measured_projector, predicate_id, _) = projector(&measured, &measured_prepared, &env);
    let progress = Progress::default();
    assert!(matches!(
        controlled(
            &measured_prepared,
            &mut measured_projector,
            predicate_id,
            &funded(),
            &progress
        ),
        Ok(AnalysisOutcome::Complete(_))
    ));
    let work = funded().semantic_work_limit - progress.remaining.get().unwrap();
    assert!(work > 4);

    for (limit, bytes, cancel) in [
        (work / 3, funded().requested_bytes_limit, false),
        (2 * work / 3, funded().requested_bytes_limit, false),
        (work - 1, funded().requested_bytes_limit, false),
        (funded().semantic_work_limit, 0, false),
        (
            funded().semantic_work_limit,
            funded().requested_bytes_limit,
            true,
        ),
    ] {
        let mut db = setup_db();
        db.write_file("src/main.py", source).unwrap();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (mut projector, predicate_id, predicate) = projector(&db, &prepared, &env);
        let revision = salsa::plumbing::current_revision(&db);
        let progress = Progress {
            cancel,
            ..Progress::default()
        };
        let policy = AnalysisPolicy {
            semantic_work_limit: limit,
            requested_bytes_limit: bytes,
        };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            controlled(&prepared, &mut projector, predicate_id, &policy, &progress)
        }));
        if cancel {
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            let expression = predicate_expression(predicate);
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    expression_narrowing_constraints_ingredient(&db),
                    expression.as_id()
                )
                .is_ok()
            );
        } else {
            assert_eq!(
                result.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: if bytes == 0 {
                        AnalysisIncomplete::RequestedAllocationLimit
                    } else {
                        AnalysisIncomplete::WorkLimit
                    },
                    completed: (),
                })
            );
        }
        assert_no_active_attempt();
        let outcome = controlled(
            &prepared,
            &mut projector,
            predicate_id,
            &funded(),
            &Progress::default(),
        );
        let Ok(AnalysisOutcome::Complete(constraints)) = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            constraints,
            infer_narrowing_constraints(&db, predicate, projector.place)
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn predicate_constraint_queries_preserve_equal_and_changed_results_across_revisions() {
    for ordinary_first in [false, true] {
        for changed in [false, true] {
            let mut db = setup_db();
            db.write_file(
                "src/main.py",
                "def choose(value, other):\n    return value and value and True\n",
            )
            .unwrap();
            let previous_revision = salsa::plumbing::current_revision(&db);
            let original_id = {
                let prepared = prepare(&db);
                let env = ProgramEnvironment::from_file(prepared.program_file());
                let (mut projector, predicate_id, predicate) = projector(&db, &prepared, &env);
                if ordinary_first {
                    infer_narrowing_constraints(&db, predicate, projector.place);
                } else {
                    assert!(matches!(
                        controlled(
                            &prepared,
                            &mut projector,
                            predicate_id,
                            &funded(),
                            &Progress::default()
                        ),
                        Ok(AnalysisOutcome::Complete(_))
                    ));
                }
                predicate_expression(predicate).as_id()
            };
            db.write_file(
                "src/main.py",
                if changed {
                    "def choose(value, other):\n    return other and value and True\n"
                } else {
                    "def choose(value, other):\n    return value and value and None\n"
                },
            )
            .unwrap();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let (mut projector, predicate_id, predicate) = projector_for_name(
                &db,
                &prepared,
                &env,
                if changed { "other" } else { "value" },
            );
            assert_eq!(predicate_expression(predicate).as_id(), original_id);
            let progress = Progress {
                previous_revision: Some(previous_revision),
                ..Progress::default()
            };
            let outcome = controlled(
                &prepared,
                &mut projector,
                predicate_id,
                &funded(),
                &progress,
            );
            let Ok(AnalysisOutcome::Complete(constraints)) = outcome else {
                panic!("{outcome:?}");
            };
            assert_eq!(
                matches!(progress.verified.get(), Some(VerifyResult::Changed)),
                changed
            );
            assert!(progress.verified.get().is_some());
            assert_eq!(
                infer_narrowing_constraints(&db, predicate, projector.place),
                constraints
            );
            assert_no_active_attempt();
        }
    }
}
