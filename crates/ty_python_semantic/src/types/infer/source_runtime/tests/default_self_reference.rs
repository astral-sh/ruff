use std::panic::AssertUnwindSafe;

use salsa::execution_probe::FinalSourceMemo;

use super::*;
use crate::types::KnownInstanceType;
use crate::types::typevar::{
    TypeVarDefaultVisitor, TypeVarInstance, lazy_typevar_default_ingredient,
};

#[derive(Clone, Copy, Debug, Default)]
struct ValidationObservations {
    created: usize,
    dropped: usize,
    active: usize,
    maximum_active: usize,
    visitor: Option<usize>,
    foreign_visitor: bool,
    search_queued: usize,
    search_started: usize,
    default_queued: usize,
    default_started: usize,
    remaining: Option<usize>,
}

thread_local! {
    static VALIDATION: Cell<ValidationObservations> = Cell::new(ValidationObservations::default());
    static CANCEL: Cell<bool> = const { Cell::new(false) };
    static CANCEL_QUEUED: Cell<bool> = const { Cell::new(false) };
}

fn observe_visitor(observation: &mut ValidationObservations, visitor: &TypeVarDefaultVisitor<'_>) {
    let identity = std::ptr::from_ref(visitor) as usize;
    if let Some(previous) = observation.visitor {
        observation.foreign_visitor |= previous != identity;
    } else {
        observation.visitor = Some(identity);
    }
}

pub(in crate::types::infer) fn validation_created(
    db: &dyn Db,
    _target: TypeVarInstance<'_>,
    visitor: &TypeVarDefaultVisitor<'_>,
) {
    let mut observation = VALIDATION.get();
    observe_visitor(&mut observation, visitor);
    observation.created += 1;
    observation.active += 1;
    observation.maximum_active = observation.maximum_active.max(observation.active);
    if observation.remaining.is_none() {
        observation.remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
    }
    VALIDATION.set(observation);
    if CANCEL.replace(false) {
        db.cancellation_token().cancel();
    }
}

pub(in crate::types::infer) fn validation_dropped(
    _db: &dyn Db,
    _target: TypeVarInstance<'_>,
    visitor: &TypeVarDefaultVisitor<'_>,
) {
    let mut observation = VALIDATION.get();
    observe_visitor(&mut observation, visitor);
    observation.dropped += 1;
    observation.active -= 1;
    VALIDATION.set(observation);
}

pub(in crate::types::infer) fn observe_search_queued(
    db: &dyn Db,
    _ty: Type<'_>,
    visitor: &TypeVarDefaultVisitor<'_>,
) {
    let mut observation = VALIDATION.get();
    observe_visitor(&mut observation, visitor);
    observation.search_queued += 1;
    VALIDATION.set(observation);
    if CANCEL_QUEUED.replace(false) {
        db.cancellation_token().cancel();
    }
}

pub(in crate::types::infer) fn observe_search_child(
    _db: &dyn Db,
    _ty: Type<'_>,
    visitor: &TypeVarDefaultVisitor<'_>,
) {
    let mut observation = VALIDATION.get();
    observe_visitor(&mut observation, visitor);
    observation.search_started += 1;
    VALIDATION.set(observation);
}

pub(in crate::types::infer) fn observe_default_queued(
    _db: &dyn Db,
    _variable: TypeVarInstance<'_>,
    visitor: &TypeVarDefaultVisitor<'_>,
) {
    let mut observation = VALIDATION.get();
    observe_visitor(&mut observation, visitor);
    observation.default_queued += 1;
    VALIDATION.set(observation);
}

pub(in crate::types::infer) fn observe_default_child(
    _db: &dyn Db,
    _variable: TypeVarInstance<'_>,
    visitor: &TypeVarDefaultVisitor<'_>,
) {
    let mut observation = VALIDATION.get();
    observe_visitor(&mut observation, visitor);
    observation.default_started += 1;
    VALIDATION.set(observation);
}

fn reset(cancel: bool) {
    VALIDATION.set(ValidationObservations::default());
    CANCEL.set(cancel);
    CANCEL_QUEUED.set(false);
    observations::reset(None);
}

fn fixture(source: &str) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file("src/main.py", source).unwrap();
    db
}

const SIMPLE_DEFAULT: &str = "from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n";

fn definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> Definition<'db> {
    let Some(Stmt::Assign(assignment)) = prepared.parsed_module().syntax().body.last() else {
        panic!("fixture ends with a TypeVar assignment");
    };
    assignment_definition(prepared, assignment)
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    definition: Definition<'db>,
    visitor: &TypeVarDefaultVisitor<'db>,
    variable: &Cell<Option<TypeVarInstance<'db>>>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Option<Type<'db>>>, AnalysisFailure> {
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
            let inferred_variable = access
                .endpoint()
                .local_call(|| {
                    access.endpoint().admit_work(4)?;
                    access.endpoint().check_completion()?;
                    let Some(Type::KnownInstance(KnownInstanceType::TypeVar(inferred_variable))) =
                        inference.completed_binding(definition)
                    else {
                        return Err(RunError::Contract(
                            "fixture definition is not a legacy TypeVar",
                        ));
                    };
                    variable.set(Some(inferred_variable));
                    Ok(inferred_variable)
                })
                .await;
            SourceEffects::new(&access, session.program())
                .typevar_default_with_visitor(
                    inferred_variable,
                    &ProgramEnvironment::from_file(prepared.program_file()),
                    visitor,
                )
                .await
        })
    })
}

fn ordinary_checked<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> Option<Type<'db>> {
    let definition = definition(prepared);
    let Some(Type::KnownInstance(KnownInstanceType::TypeVar(variable))) =
        infer_definition_types(db, definition).completed_binding(definition)
    else {
        panic!("fixture definition is a legacy TypeVar");
    };
    variable.default_type(db, &ProgramEnvironment::from_file(prepared.program_file()))
}

fn assert_cleanup() {
    let observation = VALIDATION.get();
    assert_eq!(observation.active, 0, "{observation:?}");
    assert_eq!(observation.created, observation.dropped, "{observation:?}");
    assert!(!observation.foreign_visitor, "{observation:?}");
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn assert_raw_memo(db: &TestDb, variable: TypeVarInstance<'_>) {
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            lazy_typevar_default_ingredient(db),
            variable.as_id(),
        )
        .is_ok()
    );
}

fn assert_raw_not_executed(db: &TestDb, variable: TypeVarInstance<'_>, events: &[salsa::Event]) {
    let key = lazy_typevar_default_ingredient(db).database_key_index(variable.as_id());
    assert!(!events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    }));
}

#[test]
fn cold_checked_default_matches_ordinary_and_reuses_exact_raw_memo() {
    let db = fixture(SIMPLE_DEFAULT);
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    let visitor = TypeVarDefaultVisitor::new(None);
    let variable = Cell::new(None);
    let revision = salsa::plumbing::current_revision(&db);
    reset(false);
    let result = controlled(&prepared, definition, &visitor, &variable, &funded());
    let Ok(AnalysisOutcome::Complete(Some(actual))) = result else {
        panic!("{result:?}");
    };
    let variable = variable.get().unwrap();
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    let observation = VALIDATION.get();
    assert_eq!(observation.created, 1);
    assert_eq!(
        observation.visitor,
        Some(std::ptr::from_ref(&visitor) as usize)
    );
    assert!(observation.search_queued > 0, "{observation:?}");
    assert_eq!(observation.search_queued, observation.search_started);
    assert_raw_memo(&db, variable);
    assert_cleanup();

    let ordinary_db = fixture(SIMPLE_DEFAULT);
    let ordinary_prepared = prepare(&ordinary_db);
    let expected = ordinary_checked(&ordinary_db, &ordinary_prepared).unwrap();
    assert_eq!(
        actual
            .display(&db, &ProgramEnvironment::from_file(prepared.program_file()))
            .to_string(),
        expected
            .display(
                &ordinary_db,
                &ProgramEnvironment::from_file(ordinary_prepared.program_file())
            )
            .to_string(),
    );

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let second_visitor = TypeVarDefaultVisitor::new(None);
    reset(false);
    assert_eq!(
        controlled(
            &prepared,
            definition,
            &second_visitor,
            &Cell::new(None),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Some(actual))),
    );
    assert_eq!(VALIDATION.get().created, 1);
    assert_eq!(second_visitor.ownership_probe_counts(), (0, 1));
    assert_raw_not_executed(&db, variable, &events_db.take_salsa_events());
    assert_cleanup();

    reset(false);
    assert_eq!(
        controlled(
            &prepared,
            definition,
            &second_visitor,
            &Cell::new(None),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Some(actual))),
    );
    assert_eq!(VALIDATION.get().created, 0);
    assert_eq!(second_visitor.ownership_probe_counts(), (0, 1));
    assert_raw_not_executed(&db, variable, &events_db.take_salsa_events());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn validation_work_refusal_drains_state_and_retries_with_completed_raw_child() {
    let measured_db = fixture(SIMPLE_DEFAULT);
    let measured_prepared = prepare(&measured_db);
    reset(false);
    let measured = controlled(
        &measured_prepared,
        definition(&measured_prepared),
        &TypeVarDefaultVisitor::new(None),
        &Cell::new(None),
        &funded(),
    );
    assert!(
        matches!(measured, Ok(AnalysisOutcome::Complete(Some(_)))),
        "{measured:?}"
    );
    let spent = funded().semantic_work_limit - VALIDATION.get().remaining.unwrap();
    assert_cleanup();

    let db = fixture(SIMPLE_DEFAULT);
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    let visitor = TypeVarDefaultVisitor::new(None);
    let variable = Cell::new(None);
    let revision = salsa::plumbing::current_revision(&db);
    reset(false);
    let result = controlled(
        &prepared,
        definition,
        &visitor,
        &variable,
        &AnalysisPolicy {
            semantic_work_limit: spent,
            ..funded()
        },
    );
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert_eq!(VALIDATION.get().remaining, Some(0));
    assert_eq!(VALIDATION.get().created, 1);
    assert_eq!(visitor.ownership_probe_counts(), (0, 0));
    let variable = variable.get().unwrap();
    assert_raw_memo(&db, variable);
    assert_cleanup();

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    let retried = controlled(&prepared, definition, &visitor, &Cell::new(None), &funded());
    assert!(
        matches!(retried, Ok(AnalysisOutcome::Complete(Some(_)))),
        "{retried:?}"
    );
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    assert_raw_not_executed(&db, variable, &events_db.take_salsa_events());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

fn native_validation_cancellation(cancel_queued: bool) {
    let db = fixture(SIMPLE_DEFAULT);
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    let visitor = TypeVarDefaultVisitor::new(None);
    let variable = Cell::new(None);
    let revision = salsa::plumbing::current_revision(&db);
    reset(!cancel_queued);
    CANCEL_QUEUED.set(cancel_queued);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, definition, &visitor, &variable, &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(VALIDATION.get().created, 1);
    if cancel_queued {
        assert!(VALIDATION.get().search_queued > 0);
    }
    assert_eq!(visitor.ownership_probe_counts(), (0, 0));
    let variable = variable.get().unwrap();
    assert_raw_memo(&db, variable);
    assert_cleanup();

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    let retried = controlled(&prepared, definition, &visitor, &Cell::new(None), &funded());
    assert!(
        matches!(retried, Ok(AnalysisOutcome::Complete(Some(_)))),
        "{retried:?}"
    );
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    assert_raw_not_executed(&db, variable, &events_db.take_salsa_events());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn native_validation_cancellation_drains_state_and_retries_with_completed_raw_child() {
    native_validation_cancellation(false);
}

#[test]
fn native_queued_search_cancellation_drains_retained_state_and_visitor() {
    native_validation_cancellation(true);
}

#[test]
fn validation_owner_allocation_refusal_retains_raw_result_without_caching_checked_default() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture(SIMPLE_DEFAULT);
        let prepared = prepare(&db);
        let visitor = TypeVarDefaultVisitor::new(None);
        reset(false);
        let result = controlled(
            &prepared,
            definition(&prepared),
            &visitor,
            &Cell::new(None),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
        );
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(Some(_)))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        completed: ()
                    })
            ),
            "{result:?}"
        );
        if VALIDATION.get().created > 0 {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_eq!(visitor.ownership_probe_counts().0, 0);
        assert_cleanup();
    }
    assert!(upper > 0);

    let db = fixture(SIMPLE_DEFAULT);
    let prepared = prepare(&db);
    let definition = definition(&prepared);
    let visitor = TypeVarDefaultVisitor::new(None);
    let variable = Cell::new(None);
    let revision = salsa::plumbing::current_revision(&db);
    reset(false);
    assert_eq!(
        controlled(
            &prepared,
            definition,
            &visitor,
            &variable,
            &AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }),
    );
    assert_eq!(VALIDATION.get().created, 0);
    assert_eq!(visitor.ownership_probe_counts(), (0, 0));
    let variable = variable.get().unwrap();
    assert_raw_memo(&db, variable);
    assert_cleanup();

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    let retried = controlled(&prepared, definition, &visitor, &Cell::new(None), &funded());
    assert!(
        matches!(retried, Ok(AnalysisOutcome::Complete(Some(_)))),
        "{retried:?}"
    );
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    assert_raw_not_executed(&db, variable, &events_db.take_salsa_events());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn nested_typevar_defaults_share_the_retained_visitor() {
    let db = fixture(
        "from typing import TypeVar\nU = TypeVar(\"U\", default=int)\nT = TypeVar(\"T\", default=U)\n",
    );
    let prepared = prepare(&db);
    let visitor = TypeVarDefaultVisitor::new(None);
    reset(false);
    let result = controlled(
        &prepared,
        definition(&prepared),
        &visitor,
        &Cell::new(None),
        &funded(),
    );
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete(Some(_)))),
        "nested default source result: {result:?}"
    );
    let observation = VALIDATION.get();
    assert!(observation.default_queued > 0, "{observation:?}");
    assert_eq!(observation.default_queued, observation.default_started);
    assert!(observation.maximum_active > 1, "{observation:?}");
    assert_eq!(visitor.ownership_probe_counts(), (0, 2));
    assert_cleanup();
}
