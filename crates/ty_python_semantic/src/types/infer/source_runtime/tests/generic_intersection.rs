use std::panic::AssertUnwindSafe;

use super::*;
use crate::types::set_theoretic::generic_gradual_intersections::generic_gradual_intersection;

#[derive(Default)]
struct Progress {
    remaining: Cell<Option<usize>>,
    cancel_after_completion: bool,
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    first: Type<'db>,
    second: Type<'db>,
    policy: &AnalysisPolicy,
    progress: &Progress,
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
            let result = effects
                .generic_intersection_source(&env, first, second)
                .await?;
            progress
                .remaining
                .set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    session.db(),
                ));
            if progress.cancel_after_completion {
                access
                    .endpoint
                    .local_call(|| {
                        session.db().cancellation_token().cancel();
                        access.endpoint.check_completion()
                    })
                    .await;
            }
            Ok(result.is_none())
        })
    })
}

#[test]
fn atomic_generic_reductions_use_shared_search_and_class_selection() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    for (first, second) in [
        (Type::unknown(), Type::AlwaysTruthy),
        (Type::AlwaysTruthy, Type::unknown()),
        (Type::unknown(), Type::AlwaysFalsy),
        (Type::AlwaysFalsy, Type::unknown()),
    ] {
        assert_eq!(
            controlled(&prepared, first, second, &funded(), &Progress::default()),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert!(generic_gradual_intersection(&db, &env, first, second).is_none());
        assert_no_active_attempt();
    }
}

#[test]
fn both_class_selectors_run_before_a_missing_specialization_short_circuits() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    // Unknown has no class specialization. The second operand still needs its literal fallback
    // before the helper can conclude that this pair has no generic reduction.
    for _ in 0..2 {
        assert_eq!(
            controlled(
                &prepared,
                Type::unknown(),
                Type::bool_literal(true),
                &funded(),
                &Progress::default(),
            ),
            Ok(unavailable(OperationId::ClassSelection)),
        );
        assert_no_active_attempt();
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn generic_reduction_refusal_and_cancellation_allow_same_revision_retry() {
    let measured = fixture();
    let prepared = prepare(&measured);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            Type::unknown(),
            Type::AlwaysTruthy,
            &funded(),
            &progress
        ),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let completed_work = funded().semantic_work_limit - progress.remaining.get().unwrap();
    assert!(completed_work > 0);

    for cancel in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let progress = Progress {
            cancel_after_completion: cancel,
            ..Progress::default()
        };
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: completed_work - 1,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                Type::unknown(),
                Type::AlwaysTruthy,
                &policy,
                &progress,
            )
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => assert!(progress.remaining.get().is_some()),
            Ok(outcome) if !cancel => {
                assert!(progress.remaining.get().is_none());
                assert_eq!(
                    outcome,
                    Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    })
                );
            }
            other => panic!("{other:?}"),
        }
        assert_no_active_attempt();
        assert_eq!(
            controlled(
                &prepared,
                Type::unknown(),
                Type::AlwaysTruthy,
                &funded(),
                &Progress::default()
            ),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
