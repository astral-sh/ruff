use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::KnownInstanceType;
use crate::types::typevar::{
    TypeVarDefaultVisitor, TypeVarInstance, lazy_typevar_default_ingredient,
};

thread_local! {
    static CHILDREN: Cell<usize> = const { Cell::new(0) };
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
}

pub(in crate::types::infer) fn observe_type_expression(db: &dyn Db, _ty: Type<'_>) {
    CHILDREN.set(CHILDREN.get() + 1);
    if REMAINING.get().is_none() {
        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        ACTIVE.set(observations::counts().0);
    }
    if CANCEL.replace(false) {
        db.cancellation_token().cancel();
    }
}

fn reset(cancel: bool) {
    CHILDREN.set(0);
    REMAINING.set(None);
    ACTIVE.set(0);
    CANCEL.set(cancel);
    observations::reset(None);
}

#[derive(Clone, Copy)]
enum Action<'visitor, 'db> {
    Deferred,
    Raw,
    Checked(&'visitor TypeVarDefaultVisitor<'db>),
    Initial,
}

#[derive(Debug, Eq, PartialEq)]
enum Value<'db> {
    Deferred(&'db DefinitionInference<'db>),
    Default(TypeVarInstance<'db>, Option<Type<'db>>),
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    definition: Definition<'db>,
    action: Action<'_, 'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Value<'db>>, AnalysisFailure> {
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
                DeferredDefinitionProvider {
                    session,
                    routes,
                    values,
                }
                .initial(endpoint, session.db(), definition.as_id(), definition)
                .await?;
                return Err(RunError::Contract(
                    "deferred definition cycle initial unexpectedly completed",
                ));
            }
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            if let Action::Deferred = action {
                return Ok(Value::Deferred(
                    access.deferred_definition(definition).await?,
                ));
            }
            let inference = access.definition(definition).await?;
            let variable = access
                .endpoint()
                .local_call(|| {
                    access.endpoint().admit_work(4)?;
                    access.endpoint().check_completion()?;
                    match inference.completed_binding(definition) {
                        Some(Type::KnownInstance(KnownInstanceType::TypeVar(variable))) => {
                            Ok(variable)
                        }
                        _ => Err(RunError::Contract(
                            "fixture definition is not a legacy TypeVar",
                        )),
                    }
                })
                .await;
            let default = match action {
                Action::Checked(visitor) => {
                    SourceEffects::new(&access, session.program())
                        .typevar_default_with_visitor(
                            variable,
                            &ProgramEnvironment::from_file(prepared.program_file()),
                            visitor,
                        )
                        .await?
                }
                Action::Raw => access.lazy_typevar_default(variable).await?,
                Action::Deferred | Action::Initial => {
                    return Err(RunError::Contract("fixture action was already handled"));
                }
            };
            Ok(Value::Default(variable, default))
        })
    })
}

fn fixture() -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.py",
        "from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n",
    )
    .unwrap();
    db
}

fn definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> Definition<'db> {
    let Some(Stmt::Assign(assignment)) = prepared.parsed_module().syntax().body.last() else {
        panic!("fixture ends with a TypeVar assignment");
    };
    assignment_definition(prepared, assignment)
}

fn default_expression<'a>(prepared: &'a PreparedAnalysisFile<'_>) -> &'a ast::Expr {
    let Some(Stmt::Assign(assignment)) = prepared.parsed_module().syntax().body.last() else {
        panic!("fixture ends with a TypeVar assignment");
    };
    let ast::Expr::Call(call) = &*assignment.value else {
        panic!("fixture assignment calls TypeVar");
    };
    &call.arguments.find_keyword("default").unwrap().value
}

fn ordinary_variable<'db>(db: &'db TestDb, definition: Definition<'db>) -> TypeVarInstance<'db> {
    let Some(Type::KnownInstance(KnownInstanceType::TypeVar(variable))) =
        infer_definition_types(db, definition).completed_binding(definition)
    else {
        panic!("fixture definition is a legacy TypeVar");
    };
    variable
}

fn raw_ordinary<'db>(db: &'db TestDb, variable: TypeVarInstance<'db>) -> Option<Type<'db>> {
    salsa::attach(db, || {
        *lazy_typevar_default_ingredient(db).fetch(
            db as &dyn Db,
            (db as &dyn Db).zalsa(),
            (db as &dyn Db).zalsa_local(),
            variable.as_id(),
        )
    })
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn type_name<'db>(db: &'db TestDb, prepared: &PreparedAnalysisFile<'db>, ty: Type<'db>) -> String {
    ty.display(db, &ProgramEnvironment::from_file(prepared.program_file()))
        .to_string()
}

fn assert_deferred_missing(db: &TestDb, definition: Definition<'_>) {
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            deferred_definition_inference_ingredient(db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
}

#[test]
fn cold_deferred_typevar_default_matches_independent_ordinary_inference() {
    let db = fixture();
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    assert_deferred_missing(&db, definition);
    reset(false);
    let result = controlled(&prepared, definition, Action::Deferred, &funded());
    let Ok(AnalysisOutcome::Complete(Value::Deferred(inference))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(CHILDREN.get(), 1);
    assert!(ACTIVE.get() > 0);
    let actual = inference.expression_type(default_expression(&prepared));
    assert_eq!(type_name(&db, &prepared, actual), "int");
    assert!(inference.extra.is_none());
    let canonical = crate::types::infer::infer_deferred_types(&db, definition);
    assert!(std::ptr::eq(canonical, inference));
    assert_cleanup();

    let ordinary_db = fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_definition = self::definition(&ordinary_prepared);
    let ordinary = crate::types::infer::infer_deferred_types(&ordinary_db, ordinary_definition);
    assert_eq!(
        type_name(&db, &prepared, actual),
        type_name(
            &ordinary_db,
            &ordinary_prepared,
            ordinary.expression_type(default_expression(&ordinary_prepared))
        ),
    );
    assert_eq!(
        inference.expressions.iter().len(),
        ordinary.expressions.iter().len()
    );
    assert!(ordinary.extra.is_none());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn cold_raw_and_checked_defaults_complete_from_canonical_deferred_inference() {
    for checked in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let definition = definition(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let visitor = TypeVarDefaultVisitor::new(None);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset(false);
        let result = controlled(
            &prepared,
            definition,
            if checked {
                Action::Checked(&visitor)
            } else {
                Action::Raw
            },
            &funded(),
        );
        let Ok(AnalysisOutcome::Complete(Value::Default(_, Some(default)))) = result else {
            panic!("{result:?}");
        };
        assert_eq!(visitor.ownership_probe_counts(), (0, usize::from(checked)));
        let events = events_db.take_salsa_events();
        for query in ["infer_definition_types", "infer_deferred_types"] {
            assert!(
                find_will_execute_event_by_name(&db, query, None, &events).is_some(),
                "{query}"
            );
        }
        assert_eq!(CHILDREN.get(), 1);
        let variable = ordinary_variable(&db, definition);
        let raw_key = lazy_typevar_default_ingredient(&db).database_key_index(variable.as_id());
        assert!(events.iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == raw_key)
        }));
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                lazy_typevar_default_ingredient(&db),
                variable.as_id()
            )
            .is_ok()
        );
        let actual = raw_ordinary(&db, variable).unwrap();
        assert_eq!(actual, default);
        assert_eq!(type_name(&db, &prepared, actual), "int");
        assert_cleanup();

        let ordinary_db = fixture();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_variable =
            ordinary_variable(&ordinary_db, self::definition(&ordinary_prepared));
        let expected = raw_ordinary(&ordinary_db, ordinary_variable).unwrap();
        assert_eq!(
            type_name(&db, &prepared, actual),
            type_name(&ordinary_db, &ordinary_prepared, expected)
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn deferred_default_child_interruption_discards_parents_and_reuses_completed_children() {
    let measured_db = fixture();
    let measured_prepared = prepare(&measured_db);
    reset(false);
    let measured = controlled(
        &measured_prepared,
        definition(&measured_prepared),
        Action::Raw,
        &funded(),
    );
    assert!(
        matches!(
            measured,
            Ok(AnalysisOutcome::Complete(Value::Default(_, Some(_))))
        ),
        "{measured:?}"
    );
    let work = funded().semantic_work_limit - REMAINING.get().unwrap();
    assert_cleanup();

    let db = fixture();
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    let policy = AnalysisPolicy {
        semantic_work_limit: work,
        ..funded()
    };
    let result = controlled(&prepared, definition, Action::Raw, &policy);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        }),
        "work refusal after the deferred default child",
    );
    assert_eq!(REMAINING.get(), Some(0));
    assert_eq!(CHILDREN.get(), 1);
    assert!(ACTIVE.get() > 0);
    assert_deferred_missing(&db, definition);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .is_ok()
    );
    let variable = ordinary_variable(&db, definition);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            lazy_typevar_default_ingredient(&db),
            variable.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
        "work refusal must not publish the raw-default parent",
    );
    assert_cleanup();

    let completed_children: Vec<_> = events_db
        .take_salsa_events()
        .iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            let id = database_key.key_index();
            (db.ingredient_debug_name(database_key.ingredient_index()) == "infer_definition_types"
                && id != definition.as_id()
                && FinalSourceMemo::certify(
                    &db as &dyn Db,
                    definition_inference_ingredient(&db),
                    id,
                )
                .is_ok())
            .then_some(id)
        })
        .collect();
    assert!(!completed_children.is_empty());
    reset(false);
    let retried = controlled(&prepared, definition, Action::Raw, &funded());
    assert!(
        matches!(retried, Ok(AnalysisOutcome::Complete(Value::Default(retried_variable, Some(_)))) if retried_variable == variable),
        "{retried:?}"
    );
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(definition.as_id()),
        &events,
    );
    for child in completed_children {
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(child),
            &events,
        );
    }
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_deferred_types",
            Some(definition.as_id()),
            &events
        )
        .is_some()
    );
    assert_eq!(
        type_name(&db, &prepared, raw_ordinary(&db, variable).unwrap()),
        "int"
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn native_default_cancellation_drops_checked_visitor_and_reuses_complete_children() {
    let db = fixture();
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    let visitor = TypeVarDefaultVisitor::new(None);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(true);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, definition, Action::Checked(&visitor), &funded())
    }));
    assert!(
        matches!(result, Err(salsa::Cancelled::Local)),
        "native cancellation after the deferred default child: {result:?}",
    );
    assert_eq!(CHILDREN.get(), 1);
    assert!(ACTIVE.get() > 0);
    assert_eq!(visitor.ownership_probe_counts(), (0, 0));
    assert_cleanup();

    // Salsa masks local cancellation while cycle-capable queries run. The deferred and raw
    // children finish before cancellation unwinds the enclosing checked-default visitor.
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok(),
        "native cancellation retains the completed definition child"
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok(),
        "native cancellation retains the completed deferred child"
    );
    let variable = ordinary_variable(&db, definition);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            lazy_typevar_default_ingredient(&db),
            variable.as_id(),
        )
        .is_ok(),
        "native cancellation retains the completed raw-default child"
    );
    let deferred = crate::types::infer::infer_deferred_types(&db, definition);
    let raw = raw_ordinary(&db, variable).unwrap();
    assert_eq!(raw, deferred.expression_type(default_expression(&prepared)));
    assert_eq!(type_name(&db, &prepared, raw), "int");

    events_db.take_salsa_events();
    reset(false);
    assert_eq!(
        controlled(&prepared, definition, Action::Checked(&visitor), &funded()),
        Ok(AnalysisOutcome::Complete(Value::Default(variable, Some(raw)))),
        "native cancellation retry completes checked-default validation",
    );
    assert_eq!(CHILDREN.get(), 0);
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    let events = events_db.take_salsa_events();
    for query in ["infer_definition_types", "infer_deferred_types"] {
        assert_function_query_was_not_run_by_name(&db, query, Some(definition.as_id()), &events);
    }
    let raw_key = lazy_typevar_default_ingredient(&db).database_key_index(variable.as_id());
    assert!(!events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == raw_key)
    }), "native cancellation retry reuses the canonical raw-default child");
    assert!(std::ptr::eq(
        deferred,
        crate::types::infer::infer_deferred_types(&db, definition),
    ));
    assert_eq!(raw_ordinary(&db, variable), Some(raw));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn canonical_deferred_cycle_initial_retains_its_precise_refusal() {
    let db = fixture();
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    reset(false);
    assert_eq!(
        controlled(&prepared, definition, Action::Initial, &funded()),
        Ok(unavailable(OperationId::DeferredDefinitionCycleInitial)),
    );
    assert_eq!(CHILDREN.get(), 0);
    assert_deferred_missing(&db, definition);
    assert_cleanup();
}
