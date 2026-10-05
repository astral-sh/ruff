use salsa::execution_probe::FinalSourceMemo;

use super::*;

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer::source_runtime) enum Stage {
    BeforeIntern,
    AfterIntern,
    ExpressionFieldRead,
    ContextFieldRead,
}

thread_local! {
    static REMAINING: Cell<[Option<usize>; 4]> = const { Cell::new([None; 4]) };
}

pub(in crate::types::infer::source_runtime) fn observe(stage: Stage, db: &dyn Db) {
    let mut remaining = REMAINING.get();
    if remaining[stage as usize].is_none() {
        remaining[stage as usize] = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
    }
    REMAINING.set(remaining);
}

fn reset(cancel: Option<observations::Event>) {
    REMAINING.set([None; 4]);
    observations::reset(cancel);
}

#[derive(Clone, Copy)]
enum Action {
    Infer(Option<salsa::Id>),
    Initial,
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    expression: Expression<'db>,
    context: TypeContext<'db>,
    action: Action,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<&'db ExpressionInference<'db>>, AnalysisFailure> {
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
            if let Action::Initial = action {
                let contextual = endpoint
                    .intern_value(&values.expression_context, (expression, context))
                    .await;
                let input = InferExpression::WithContext(contextual);
                let provider = ExpressionProvider {
                    session,
                    routes,
                    values,
                };
                provider
                    .initial(endpoint, session.db(), input.as_id(), input)
                    .await?;
                return Err(RunError::Contract(
                    "expression cycle initial unexpectedly completed",
                ));
            }
            if let Action::Infer(Some(key)) = action {
                assert_eq!(
                    routes.expression.database_key(key),
                    expression_inference_ingredient(session.db()).database_key_index(key),
                );
            }
            SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            }
            .expression(expression, context)
            .await
        })
    })
}

fn contextual_keys(db: &TestDb, events: &[salsa::Event]) -> Vec<salsa::Id> {
    events
        .iter()
        .filter_map(|event| {
            if let salsa::EventKind::DidInternValue { key, .. } = event.kind
                && db.ingredient_debug_name(key.ingredient_index()) == "ExpressionWithContext"
            {
                Some(key.key_index())
            } else {
                None
            }
        })
        .collect()
}

fn executed_key(db: &TestDb, events: &[salsa::Event]) -> salsa::Id {
    let Some(event) =
        find_will_execute_event_by_name(db, "infer_expression_types_impl", None, events)
    else {
        panic!("cold expression did not execute its canonical query");
    };
    let salsa::EventKind::WillExecute { database_key } = event.kind else {
        panic!("expected an expression execution event");
    };
    database_key.key_index()
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn cold_contexts_preserve_exact_canonical_keys_and_full_results() {
    for source in ["True", "1", "1 < 2", "callee"] {
        let mut db = setup_db();
        db.write_file(
            "src/main.py",
            if source == "callee" {
                "def choose():\n    pass\nchoose()\n".to_owned()
            } else {
                format!("left = right = {source}\n")
            },
        )
        .unwrap();
        let prepared = prepare(&db);
        let expression = if source == "callee" {
            call_expressions(&db).0
        } else {
            expression(&db)
        };
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        let mut completed = Vec::new();
        for annotation in [None, Some(Type::unknown()), Some(Type::any())] {
            let context = TypeContext::new(annotation);
            events_db.take_salsa_events();
            reset(None);
            let result = controlled(
                &prepared,
                expression,
                context,
                Action::Infer(None),
                &funded(),
            );
            let Ok(AnalysisOutcome::Complete(inference)) = result else {
                panic!("{source}, {context:?}: {result:?}");
            };
            let events = events_db.take_salsa_events();
            let key = executed_key(&db, &events);
            if annotation.is_some() {
                assert_eq!(contextual_keys(&db, &events), [key]);
                assert_ne!(key, expression.as_id());
                assert!(REMAINING.get().iter().all(Option::is_some));
            } else {
                assert!(contextual_keys(&db, &events).is_empty());
                assert_eq!(key, expression.as_id());
                assert_eq!(REMAINING.get(), [None; 4]);
            }
            assert!(completed.iter().all(|(_, previous, _)| *previous != key));
            let repeated = controlled(
                &prepared,
                expression,
                context,
                Action::Infer(Some(key)),
                &funded(),
            );
            let Ok(AnalysisOutcome::Complete(repeated)) = repeated else {
                panic!("canonical reuse failed: {repeated:?}");
            };
            assert!(std::ptr::eq(inference, repeated));
            let reused = events_db.take_salsa_events();
            assert!(contextual_keys(&db, &reused).is_empty());
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                None,
                &reused,
            );
            completed.push((context, key, inference));
            assert_cleanup();
        }

        // Every context completes under controlled inference before ordinary parity checks.
        let env = ProgramEnvironment::from_file(prepared.program_file());
        for (context, key, inference) in completed {
            assert_eq!(InferExpression::new(&db, expression, context).as_id(), key);
            assert!(std::ptr::eq(
                inference,
                infer_expression_types(&db, expression, context),
            ));
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                None,
                &events_db.take_salsa_events(),
            );
            let ordinary = TypeInferenceBuilder::new(
                &db,
                &env,
                InferenceRegion::Expression(expression, context),
                prepared.program_file().file(&db),
                prepared.program_file(),
                prepared.semantic_index(),
                prepared.parsed_module(),
            )
            .finish_expression();
            assert_eq!(inference, &ordinary);
            assert_eq!(
                inference.expressions.iter().len(),
                if source == "1 < 2" { 3 } else { 1 },
            );
            assert!(inference.extra.is_none());
            events_db.take_salsa_events();
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn contextual_descendant_refusals_preserve_their_cause_and_retry() {
    for (source, annotation, operation) in [
        ("[1]", Type::unknown(), OperationId::ExpressionKind),
        (
            "1",
            Type::int_literal(1),
            OperationId::ContextualLiteralAssignability,
        ),
    ] {
        let mut db = setup_db();
        db.write_file("src/main.py", format!("left = right = {source}\n"))
            .unwrap();
        let prepared = prepare(&db);
        let expression = expression(&db);
        let context = TypeContext::new(Some(annotation));
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        let mut key = None;
        for attempt in 0..2 {
            events_db.take_salsa_events();
            reset(None);
            assert_eq!(
                controlled(
                    &prepared,
                    expression,
                    context,
                    Action::Infer(key),
                    &funded()
                ),
                Ok(unavailable(operation)),
            );
            let events = events_db.take_salsa_events();
            let executed = executed_key(&db, &events);
            if attempt == 0 {
                assert_eq!(contextual_keys(&db, &events), [executed]);
                key = Some(executed);
            } else {
                assert_eq!(Some(executed), key);
                assert!(contextual_keys(&db, &events).is_empty());
            }
            assert!(REMAINING.get()[Stage::ContextFieldRead as usize].is_some());
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    expression_inference_ingredient(&db),
                    executed,
                )
                .is_err()
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_cleanup();
        }
    }
}

#[test]
fn contextual_work_refusal_drains_each_admission_boundary_before_retry() {
    let measured = boolean_fixture(true);
    let prepared = prepare(&measured);
    let context = TypeContext::new(Some(Type::unknown()));
    reset(None);
    assert!(matches!(
        controlled(
            &prepared,
            expression(&measured),
            context,
            Action::Infer(None),
            &funded(),
        ),
        Ok(AnalysisOutcome::Complete(_)),
    ));
    let checkpoints = REMAINING.get();
    let stored = observations::stored_remaining();
    for (index, remaining) in checkpoints.into_iter().chain([stored]).enumerate() {
        let Some(remaining) = remaining else {
            panic!("contextual inference missed work checkpoint {index}");
        };
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let expression = expression(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset(None);
        assert_eq!(
            controlled(
                &prepared,
                expression,
                context,
                Action::Infer(None),
                &AnalysisPolicy {
                    semantic_work_limit: funded().semantic_work_limit - remaining,
                    ..funded()
                },
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
            "checkpoint {index}",
        );
        let events = events_db.take_salsa_events();
        if index == Stage::BeforeIntern as usize {
            assert!(contextual_keys(&db, &events).is_empty());
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                None,
                &events,
            );
        } else {
            assert_eq!(contextual_keys(&db, &events).len(), 1);
        }
        if index < checkpoints.len() {
            assert!(REMAINING.get()[index].is_some());
            assert_eq!(observations::counts().1, 0);
        } else {
            assert_eq!(observations::counts(), (0, 1, 1));
        }
        assert_cleanup();
        reset(None);
        let result = controlled(
            &prepared,
            expression,
            context,
            Action::Infer(None),
            &funded(),
        );
        let Ok(AnalysisOutcome::Complete(inference)) = result else {
            panic!("funded retry failed at checkpoint {index}: {result:?}");
        };
        assert_eq!(
            inference.expression_type(expression.node_ref(&db)),
            Type::bool_literal(true),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn contextual_allocation_refusal_before_interning_and_publication_retries() {
    for publication in [false, true] {
        let mut lower = 0;
        let mut upper = funded().requested_bytes_limit;
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            let db = boolean_fixture(true);
            let prepared = prepare(&db);
            reset(None);
            let result = controlled(
                &prepared,
                expression(&db),
                TypeContext::new(Some(Type::unknown())),
                Action::Infer(None),
                &AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                },
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
                "{result:?}",
            );
            let reached = if publication {
                matches!(result, Ok(AnalysisOutcome::Complete(_)))
            } else {
                REMAINING.get()[Stage::AfterIntern as usize].is_some()
            };
            if reached {
                upper = middle;
            } else {
                lower = middle + 1;
            }
            assert_cleanup();
        }
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let expression = expression(&db);
        let context = TypeContext::new(Some(Type::unknown()));
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset(None);
        assert_eq!(
            controlled(
                &prepared,
                expression,
                context,
                Action::Infer(None),
                &AnalysisPolicy {
                    requested_bytes_limit: upper - 1,
                    ..funded()
                },
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                completed: (),
            }),
        );
        let events = events_db.take_salsa_events();
        if publication {
            assert_eq!(contextual_keys(&db, &events).len(), 1);
            assert_eq!(observations::counts(), (0, 1, 1));
        } else {
            assert!(contextual_keys(&db, &events).is_empty());
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                None,
                &events,
            );
            assert_eq!(observations::counts(), (0, 0, 0));
        }
        assert_cleanup();
        reset(None);
        assert!(matches!(
            controlled(
                &prepared,
                expression,
                context,
                Action::Infer(None),
                &funded(),
            ),
            Ok(AnalysisOutcome::Complete(_)),
        ));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn contextual_native_cancellation_preserves_payload_and_reuses_completed_children() {
    for event in [observations::Event::Created, observations::Event::Stored] {
        let db = boolean_fixture(false);
        let prepared = prepare(&db);
        let expression = expression(&db);
        let context = TypeContext::new(Some(Type::unknown()));
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        reset(Some(event));
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            controlled(
                &prepared,
                expression,
                context,
                Action::Infer(None),
                &funded(),
            )
        }));
        assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        assert_cleanup();
        let mut previous = None;
        for retry in 0..2 {
            events_db.take_salsa_events();
            reset(None);
            let result = controlled(
                &prepared,
                expression,
                context,
                Action::Infer(None),
                &funded(),
            );
            let Ok(AnalysisOutcome::Complete(inference)) = result else {
                panic!("retry after cancellation failed: {result:?}");
            };
            assert_eq!(
                inference.expression_type(expression.node_ref(&db)),
                Type::bool_literal(false),
            );
            if let Some(previous) = previous {
                assert_eq!(retry, 1);
                assert!(std::ptr::eq(previous, inference));
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_expression_types_impl",
                    None,
                    &events_db.take_salsa_events(),
                );
            }
            previous = Some(inference);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_cleanup();
        }
    }
}

#[test]
fn contextual_cycle_initial_refuses_without_creating_a_seed() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    reset(None);
    assert_eq!(
        controlled(
            &prepared,
            expression(&db),
            TypeContext::new(Some(Type::unknown())),
            Action::Initial,
            &funded(),
        ),
        Ok(unavailable(OperationId::ExpressionCycleInitial)),
    );
    assert_eq!(observations::counts(), (0, 0, 0));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}
