use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use salsa::execution_probe::FinalSourceMemo;

use super::*;

thread_local! {
    static EXPECTED: Cell<Option<usize>> = const { Cell::new(None) };
    static ACCESSES: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::types::infer::source_runtime) fn observe_access(identity: usize) {
    if let Some(expected) = EXPECTED.get() {
        assert_eq!(identity, expected);
        ACCESSES.set(ACCESSES.get() + 1);
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    expression: Expression<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    let key = expression_key(prepared);
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
        let weak = Rc::downgrade(&routes);
        assert!(Rc::strong_count(&routes) > 1);
        EXPECTED.set(Some(Rc::as_ptr(&routes).addr()));
        ACCESSES.set(0);
        let query_routes = Rc::clone(&routes);
        let values = &values;
        let result = catch_unwind(AssertUnwindSafe(|| {
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes: query_routes,
                    values,
                };
                let cloned = access.clone();
                assert!(Rc::ptr_eq(&access.routes, &cloned.routes));
                drop(cloned);
                let inference = access
                    .expression(expression, TypeContext::default())
                    .await?;
                Ok(access
                    .endpoint
                    .local_call(|| {
                        access
                            .endpoint
                            .admit_work(inference.expressions.iter().len().saturating_add(1))?;
                        access.endpoint.check_completion()?;
                        Ok(inference.expression_type(key))
                    })
                    .await)
            })
        }));
        EXPECTED.set(None);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(Rc::strong_count(&routes), 1);
        drop(routes);
        assert!(weak.upgrade().is_none());
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

#[test]
fn providers_and_access_clones_share_one_table_until_the_query_drains() -> anyhow::Result<()> {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        controlled(&prepared, expression(&db), &funded()),
        Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
    );
    assert!(ACCESSES.get() > 0);
    assert_eq!(observations::counts(), (0, 1, 1));
    let events = events_db.take_salsa_events();
    let Some(event) =
        find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &events)
    else {
        anyhow::bail!("cold expression did not execute its canonical query");
    };
    let salsa::EventKind::WillExecute { database_key } = event.kind else {
        anyhow::bail!("expected an expression execution event");
    };
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            database_key.key_index()
        )
        .is_ok()
    );
    assert_no_active_attempt();
    Ok(())
}

#[test]
fn interrupted_queries_release_the_shared_table_before_same_revision_retry() {
    let measured = boolean_fixture(true);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert_eq!(
        controlled(&measured_prepared, expression(&measured), &funded()),
        Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
    );
    let stored_work = funded().semantic_work_limit - observations::stored_remaining().unwrap();

    for cancel in [false, true] {
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::Stored));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: stored_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, expression(&db), &policy)
        }));
        if cancel {
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        } else {
            assert_eq!(
                result.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
        }
        assert!(ACCESSES.get() > 0);
        assert_eq!(observations::counts(), (0, 1, 1));
        assert_no_active_attempt();
        observations::reset(None);
        assert_eq!(
            controlled(&prepared, expression(&db), &funded()),
            Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}
