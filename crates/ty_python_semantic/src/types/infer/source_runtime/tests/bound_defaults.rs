use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::typevar::bound_typevar_default_ingredient;

thread_local! {
    static ENTRIES: Cell<usize> = const { Cell::new(0) };
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static BOUND: Cell<Option<salsa::Id>> = const { Cell::new(None) };
    static CONTEXT_CLASS: Cell<Option<salsa::Id>> = const { Cell::new(None) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
}

pub(in crate::types::infer) fn observe_default_query(db: &dyn Db, bound: BoundTypeVarInstance<'_>) {
    let previous = ENTRIES.get();
    ENTRIES.set(previous + 1);
    if previous == 0 {
        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        BOUND.set(Some(bound.as_id()));
    }
    if CANCEL.replace(false) {
        db.cancellation_token().cancel();
    }
}

fn reset(cancel: bool) {
    ENTRIES.set(0);
    REMAINING.set(None);
    BOUND.set(None);
    CONTEXT_CLASS.set(None);
    CANCEL.set(cancel);
    observations::reset(None);
}

fn fixture(with_default: bool) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.pyi",
        if with_default {
            "from typing import Generic, TypeVar\n\
             class Leaf: ...\n\
             T = TypeVar(\"T\", default=\"Leaf\")\n\
             class Product(Generic[T]): ...\n"
        } else {
            "from typing import Generic, TypeVar\n\
             T = TypeVar(\"T\")\n\
             class Product(Generic[T]): ...\n"
        },
    )
    .unwrap();
    db
}

pub(super) fn prepare_fixture(db: &TestDb) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, "src/main.pyi").unwrap();
    prepare_file(db, file).unwrap()
}

pub(super) fn selected_definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> Definition<'db> {
    let Some(Stmt::ClassDef(class)) = prepared.parsed_module().syntax().body.last() else {
        panic!("fixture Product class");
    };
    prepared.semantic_index().expect_single_definition(class)
}

pub(super) type DefaultResult<'db> = (
    StaticClassLiteral<'db>,
    GenericContext<'db>,
    BoundTypeVarInstance<'db>,
    Option<Type<'db>>,
);

pub(super) fn controlled_default<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<DefaultResult<'db>>, AnalysisFailure> {
    let definition = selected_definition(prepared);
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
            let inference = access.definition(definition).await?;
            let class = access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(2)?;
                    access.endpoint.check_completion()?;
                    let Some(ClassLiteral::Static(class)) =
                        inference.original_class_type(definition)
                    else {
                        return Err(RunError::Contract(
                            "fixture definition is not a static class",
                        ));
                    };
                    Ok(class)
                })
                .await;
            let Some(context) = access.class_generic_context(class).await? else {
                return Err(RunError::Contract("fixture class has no generic context"));
            };
            let variables = access
                .endpoint
                .read_field(
                    context.variables_request(access.endpoint.field_request_context()),
                    &BorrowOrCopy,
                )
                .await;
            let variable = access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(3)?;
                    access.endpoint.check_completion()?;
                    if variables.len() != 1 {
                        return Err(RunError::Contract("fixture context must have one variable"));
                    }
                    CONTEXT_CLASS.set(Some(class.as_id()));
                    GenericContext::variable_at_in(variables, 0)
                        .ok_or(RunError::Contract("fixture context has no variable"))
                })
                .await;
            let default = access.bound_typevar_default(variable).await?;
            Ok((class, context, variable, default))
        })
    })
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

pub(super) fn completed_attempt<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> (
    Result<AnalysisOutcome<DefaultResult<'db>>, AnalysisFailure>,
    usize,
) {
    let revision = salsa::plumbing::current_revision(db);
    for preceding_work_limit_attempts in 0..4 {
        reset(false);
        let result = controlled_default(prepared, &funded());
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(db), revision);
        if !matches!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            })
        ) {
            return (result, preceding_work_limit_attempts);
        }
    }
    panic!("bound default did not reach a terminal result within four funded caller attempts");
}

pub(super) fn query_ran(db: &TestDb, variable: salsa::Id, events: &[salsa::Event]) -> bool {
    let key = bound_typevar_default_ingredient(db).database_key_index(variable);
    events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    })
}

#[test]
fn cold_absent_default_publishes_and_reuses_the_canonical_none() {
    let db = fixture(false);
    let prepared = prepare_fixture(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let (result, _) = completed_attempt(&db, &prepared);
    let Ok(AnalysisOutcome::Complete((class, context, variable, default))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(default, None);
    assert_eq!(ENTRIES.get(), 1);
    assert_eq!(BOUND.get(), Some(variable.as_id()));
    let events = events_db.take_salsa_events();
    assert!(query_ran(&db, variable.as_id(), &events));
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            bound_typevar_default_ingredient(&db),
            variable.as_id(),
        )
        .is_ok()
    );
    assert_eq!(variable.default_type(&db), None);
    assert_eq!(
        variable.binding_context(&db).definition(),
        Some(selected_definition(&prepared))
    );
    assert_eq!(class.generic_context(&db), Some(context));
    reset(false);
    assert_eq!(controlled_default(&prepared, &funded()), result);
    assert_eq!(ENTRIES.get(), 0);
    let events = events_db.take_salsa_events();
    assert!(!query_ran(&db, variable.as_id(), &events));
    for query in [
        "infer_definition_types",
        "static_class_generic_context",
        "infer_deferred_types",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();

    let ordinary_db = fixture(false);
    let ordinary_prepared = prepare_fixture(&ordinary_db);
    let definition = selected_definition(&ordinary_prepared);
    let Some(ClassLiteral::Static(ordinary_class)) =
        infer_definition_types(&ordinary_db, definition).original_class_type(definition)
    else {
        panic!("ordinary fixture class");
    };
    let context = ordinary_class.generic_context(&ordinary_db).unwrap();
    let variable = context.variables(&ordinary_db).next().unwrap();
    assert_eq!(variable.name(&ordinary_db).as_str(), "T");
    assert_eq!(variable.default_type(&ordinary_db), None);
}

#[test]
fn controlled_default_reuses_an_ordinary_present_default_memo() {
    let db = fixture(true);
    let prepared = prepare_fixture(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let definition = selected_definition(&prepared);
    let Some(ClassLiteral::Static(class)) =
        infer_definition_types(&db, definition).original_class_type(definition)
    else {
        panic!("ordinary fixture class");
    };
    let context = class.generic_context(&db).unwrap();
    let variable = context.variables(&db).next().unwrap();
    let Some(default) = variable.default_type(&db) else {
        panic!("ordinary fixture default");
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(default.display(&db, &env).to_string(), "Leaf");
    let ingredient = bound_typevar_default_ingredient(&db);
    let certificate =
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, variable.as_id()).unwrap();
    let key = ingredient.database_key_index(variable.as_id());
    assert_eq!(certificate.database_key(), key);
    let canonical = salsa::attach(&db, || {
        ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            variable.as_id(),
        )
    });
    assert_eq!(*canonical, Some(default));

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    assert_eq!(
        controlled_default(&prepared, &funded()),
        Ok(AnalysisOutcome::Complete((
            class,
            context,
            variable,
            Some(default)
        ))),
    );
    assert_eq!(ENTRIES.get(), 0);
    let after = FinalSourceMemo::certify(&db as &dyn Db, ingredient, variable.as_id()).unwrap();
    assert_eq!(after.database_key(), key);
    let reused = salsa::attach(&db, || {
        ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            variable.as_id(),
        )
    });
    assert!(std::ptr::eq(canonical, reused));
    let events = events_db.take_salsa_events();
    assert!(!query_ran(&db, variable.as_id(), &events));
    for query in [
        "infer_definition_types",
        "static_class_generic_context",
        "infer_deferred_types",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn interrupted_default_query_reuses_completed_context_at_the_same_revision() {
    let measured = fixture(false);
    let measured_prepared = prepare_fixture(&measured);
    let (result, preceding_work_limit_attempts) = completed_attempt(&measured, &measured_prepared);
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete((_, _, _, None)))),
        "{result:?}"
    );
    assert_eq!(ENTRIES.get(), 1);
    let work = funded().semantic_work_limit - REMAINING.get().unwrap();

    for cancel in [false, true] {
        let db = fixture(false);
        let prepared = prepare_fixture(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        for _ in 0..preceding_work_limit_attempts {
            reset(false);
            assert_eq!(
                controlled_default(&prepared, &funded()),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            );
            assert_cleanup();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        events_db.take_salsa_events();
        reset(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            }
        };
        let result =
            salsa::Cancelled::catch(AssertUnwindSafe(|| controlled_default(&prepared, &policy)));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            other => panic!("cancel={cancel}: {other:?}"),
        }
        assert_eq!(ENTRIES.get(), 1);
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        let variable_id = BOUND.get().unwrap();
        let class_id = CONTEXT_CLASS.get().unwrap();
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                static_class_generic_context_ingredient(&db),
                class_id,
            )
            .is_ok()
        );
        let events = events_db.take_salsa_events();
        assert!(query_ran(&db, variable_id, &events));
        if !cancel {
            assert_eq!(REMAINING.get(), Some(0));
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    bound_typevar_default_ingredient(&db),
                    variable_id,
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
        }
        let (result, _) = completed_attempt(&db, &prepared);
        let Ok(AnalysisOutcome::Complete((class, _, variable, None))) = result else {
            panic!("{result:?}");
        };
        assert_eq!(class.as_id(), class_id);
        assert_eq!(variable.as_id(), variable_id);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                bound_typevar_default_ingredient(&db),
                variable_id,
            )
            .is_ok()
        );
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "static_class_generic_context",
            Some(class_id),
            &events,
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(selected_definition(&prepared).as_id()),
            &events,
        );
        if !cancel {
            assert!(query_ran(&db, variable_id, &events));
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// Evaluating a present lazy default publishes its bound value and reuses it in the same revision.
#[test]
fn present_lazy_default_publishes_and_reuses_its_canonical_value() {
    let db = fixture(true);
    let prepared = prepare_fixture(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let (result, _) = completed_attempt(&db, &prepared);
    let Ok(AnalysisOutcome::Complete((_, _, variable, Some(default)))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(ENTRIES.get(), 1);
    assert!(query_ran(&db, variable.as_id(), &events_db.take_salsa_events()));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(default.display(&db, &env).to_string(), "Leaf");
    let ingredient = bound_typevar_default_ingredient(&db);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, variable.as_id()).is_ok());
    assert_eq!(variable.default_type(&db), Some(default));
    assert_cleanup();

    events_db.take_salsa_events();
    reset(false);
    assert_eq!(controlled_default(&prepared, &funded()), result);
    assert_eq!(ENTRIES.get(), 0);
    let events = events_db.take_salsa_events();
    assert!(!query_ran(&db, variable.as_id(), &events));
    assert_function_query_was_not_run_by_name(&db, "infer_deferred_types", None, &events);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}
