use std::panic::AssertUnwindSafe;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::cyclic::CycleDetectorVisit;
use crate::types::typevar::{
    TypeVarBoundOrConstraints, TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation,
    TypeVarDefaultVisitor, TypeVarIdentity, TypeVarInstance, TypeVarKind,
    lazy_typevar_default_ingredient,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    Query,
    Pending,
    Prepared,
}

thread_local! {
    static QUERIES: Cell<usize> = const { Cell::new(0) };
    static PENDING: Cell<usize> = const { Cell::new(0) };
    static PREPARED: Cell<usize> = const { Cell::new(0) };
    static QUERY_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static PENDING_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static PREPARED_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static CANCEL: Cell<Option<Boundary>> = const { Cell::new(None) };
}

fn observe(db: &dyn Db, boundary: Boundary) {
    let remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
    match boundary {
        Boundary::Query => {
            QUERIES.set(QUERIES.get() + 1);
            if QUERY_REMAINING.get().is_none() {
                QUERY_REMAINING.set(remaining);
            }
        }
        Boundary::Pending => {
            PENDING.set(PENDING.get() + 1);
            if PENDING_REMAINING.get().is_none() {
                PENDING_REMAINING.set(remaining);
            }
        }
        Boundary::Prepared => {
            PREPARED.set(PREPARED.get() + 1);
            if PREPARED_REMAINING.get().is_none() {
                PREPARED_REMAINING.set(remaining);
            }
        }
    }
    if CANCEL.get() == Some(boundary) {
        CANCEL.set(None);
        db.cancellation_token().cancel();
    }
}

pub(in crate::types::infer) fn observe_query(db: &dyn Db, _variable: TypeVarInstance<'_>) {
    observe(db, Boundary::Query);
}

pub(in crate::types::infer) fn observe_pending(db: &dyn Db, _variable: TypeVarInstance<'_>) {
    observe(db, Boundary::Pending);
}

pub(in crate::types::infer) fn observe_prepared(db: &dyn Db, _variable: TypeVarInstance<'_>) {
    observe(db, Boundary::Prepared);
}

fn reset(cancel: Option<Boundary>) {
    QUERIES.set(0);
    PENDING.set(0);
    PREPARED.set(0);
    QUERY_REMAINING.set(None);
    PENDING_REMAINING.set(None);
    PREPARED_REMAINING.set(None);
    CANCEL.set(cancel);
    observations::reset(None);
}

#[derive(Clone, Copy)]
enum Operation<'visitor, 'db> {
    Raw,
    Checked(&'visitor TypeVarDefaultVisitor<'db>),
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    variable: TypeVarInstance<'db>,
    operation: Operation<'_, 'db>,
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
            match operation {
                Operation::Raw => access.lazy_typevar_default(variable).await,
                Operation::Checked(visitor) => {
                    let effects = SourceEffects::new(&access, session.program());
                    let env = ProgramEnvironment::from_file(prepared.program_file());
                    effects
                        .typevar_default_with_visitor(variable, &env, visitor)
                        .await
                }
            }
        })
    })
}

fn fixture(source: &str) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file("src/main.py", source).unwrap();
    db
}

fn source_definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> Definition<'db> {
    match prepared.parsed_module().syntax().body.last().unwrap() {
        Stmt::ClassDef(class) => {
            if let Some(parameters) = &class.type_params {
                match &parameters.type_params[0] {
                    ast::TypeParam::TypeVar(variable) => {
                        prepared.semantic_index().expect_single_definition(variable)
                    }
                    ast::TypeParam::ParamSpec(variable) => {
                        prepared.semantic_index().expect_single_definition(variable)
                    }
                    ast::TypeParam::TypeVarTuple(variable) => {
                        prepared.semantic_index().expect_single_definition(variable)
                    }
                }
            } else {
                prepared.semantic_index().expect_single_definition(class)
            }
        }
        Stmt::Assign(assignment) => assignment_definition(prepared, assignment),
        _ => panic!("fixture ends with a class or assignment"),
    }
}

fn variable<'db>(
    db: &'db TestDb,
    definition: Option<Definition<'db>>,
    default: Option<TypeVarDefaultEvaluation<'db>>,
) -> TypeVarInstance<'db> {
    let identity = TypeVarIdentity::new(
        db,
        Name::new_static("T"),
        definition,
        TypeVarKind::LegacyTypeVar,
    );
    TypeVarInstance::new(db, identity, None, None, default)
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn assert_missing(db: &TestDb, variable: TypeVarInstance<'_>) {
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            lazy_typevar_default_ingredient(db),
            variable.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
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

fn assert_raw_query_ran(db: &TestDb, variable: TypeVarInstance<'_>, events: &[salsa::Event]) {
    let key = lazy_typevar_default_ingredient(db).database_key_index(variable.as_id());
    assert!(events.iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    }));
}

#[test]
fn cold_raw_absence_covers_missing_source_unsupported_definition_and_missing_defaults() {
    for (source, has_definition) in [
        ("class Marker: ...\n", false),
        ("class Marker: ...\n", true),
        ("T = 1\n", true),
        ("class Marker[T]: ...\n", true),
        ("class Marker[**P]: ...\n", true),
        ("class Marker[*Ts]: ...\n", true),
    ] {
        let db = fixture(source);
        let prepared = prepare(&db);
        let definition = has_definition.then(|| source_definition(&prepared));
        let variable = variable(&db, definition, Some(TypeVarDefaultEvaluation::Lazy));
        let revision = salsa::plumbing::current_revision(&db);
        assert_missing(&db, variable);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset(None);
        assert_eq!(
            controlled(&prepared, variable, Operation::Raw, &funded()),
            Ok(AnalysisOutcome::Complete(None)),
            "{source} has_definition={has_definition}",
        );
        assert_eq!(QUERIES.get(), 1);
        assert_raw_query_ran(&db, variable, &events_db.take_salsa_events());
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                lazy_typevar_default_ingredient(&db),
                variable.as_id(),
            )
            .is_ok()
        );
        assert_eq!(raw_ordinary(&db, variable), None);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();

        let ordinary_db = fixture(source);
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_definition = has_definition.then(|| source_definition(&ordinary_prepared));
        let ordinary_variable = self::variable(
            &ordinary_db,
            ordinary_definition,
            Some(TypeVarDefaultEvaluation::Lazy),
        );
        assert_eq!(raw_ordinary(&ordinary_db, ordinary_variable), None);
    }
}

#[test]
fn warm_raw_canonical_memo_is_reused_without_reentering_the_producer() {
    let db = fixture("class Marker: ...\n");
    let prepared = prepare(&db);
    let variable = variable(&db, None, Some(TypeVarDefaultEvaluation::Lazy));
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(raw_ordinary(&db, variable), None);
    let ingredient = lazy_typevar_default_ingredient(&db);
    let canonical = salsa::attach(&db, || {
        ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            variable.as_id(),
        )
    });
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(None);
    assert_eq!(
        controlled(&prepared, variable, Operation::Raw, &funded()),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert_eq!(QUERIES.get(), 0);
    assert_function_query_was_not_run_by_name(
        &db,
        "lazy_default_unchecked",
        Some(variable.as_id()),
        &events_db.take_salsa_events(),
    );
    let reused = salsa::attach(&db, || {
        ingredient.fetch(
            &db as &dyn Db,
            (&db as &dyn Db).zalsa(),
            (&db as &dyn Db).zalsa_local(),
            variable.as_id(),
        )
    });
    assert!(std::ptr::eq(canonical, reused));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn raw_query_work_refusal_keeps_the_memo_missing_and_retries_at_the_same_revision() {
    let measured_db = fixture("class Marker: ...\n");
    let measured_prepared = prepare(&measured_db);
    let measured_variable = variable(&measured_db, None, Some(TypeVarDefaultEvaluation::Lazy));
    reset(None);
    assert_eq!(
        controlled(
            &measured_prepared,
            measured_variable,
            Operation::Raw,
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(None)),
    );
    let work = funded().semantic_work_limit - QUERY_REMAINING.get().unwrap();

    let db = fixture("class Marker: ...\n");
    let prepared = prepare(&db);
    let variable = variable(&db, None, Some(TypeVarDefaultEvaluation::Lazy));
    let revision = salsa::plumbing::current_revision(&db);
    reset(None);
    assert_eq!(
        controlled(
            &prepared,
            variable,
            Operation::Raw,
            &AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            }
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    assert_eq!(QUERIES.get(), 1);
    assert_eq!(QUERY_REMAINING.get(), Some(0));
    assert_missing(&db, variable);
    assert_cleanup();
    reset(None);
    assert_eq!(
        controlled(&prepared, variable, Operation::Raw, &funded()),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert_eq!(QUERIES.get(), 1);
    assert_eq!(raw_ordinary(&db, variable), None);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn native_raw_query_cancellation_drains_and_allows_same_revision_retry() {
    let db = fixture("class Marker: ...\n");
    let prepared = prepare(&db);
    let variable = variable(&db, None, Some(TypeVarDefaultEvaluation::Lazy));
    let revision = salsa::plumbing::current_revision(&db);
    reset(Some(Boundary::Query));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, variable, Operation::Raw, &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(QUERIES.get(), 1);
    assert_cleanup();
    // A cycle-capable query may finish before deferred local cancellation is delivered. Any
    // surviving memo must contain the complete raw result, and a missing memo is retried below.
    match FinalSourceMemo::certify(
        &db as &dyn Db,
        lazy_typevar_default_ingredient(&db),
        variable.as_id(),
    ) {
        Ok(_) => assert_eq!(raw_ordinary(&db, variable), None),
        Err(error) => assert_eq!(error, FinalSourceError::MissingMemo),
    }
    reset(None);
    assert_eq!(
        controlled(&prepared, variable, Operation::Raw, &funded()),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert_eq!(raw_ordinary(&db, variable), None);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Absent and eager defaults leave a supplied visitor empty and never fetch the raw lazy query.
#[test_case::test_case(None; "absent")]
#[test_case::test_case(Some(1); "eager")]
fn stored_defaults_do_not_enter_the_visitor(value: Option<i64>) {
    let db = fixture("class Marker: ...\n");
    let prepared = prepare(&db);
    let visitor = TypeVarDefaultVisitor::new(None);
    let before = visitor.ownership_probe_storage();
    let revision = salsa::plumbing::current_revision(&db);
    let expected = value.map(Type::int_literal);
    let variable = variable(&db, None, expected.map(TypeVarDefaultEvaluation::Eager));
    reset(None);
    assert_eq!(
        controlled(&prepared, variable, Operation::Checked(&visitor), &funded()),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert_eq!(PENDING.get(), 0);
    assert_eq!(PREPARED.get(), 0);
    assert_eq!(QUERIES.get(), 0);
    assert_eq!(visitor.ownership_probe_counts(), (0, 0));
    assert_eq!(visitor.ownership_probe_storage(), before);
    assert_missing(&db, variable);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Checks that a raw default is already canonical and has the expected display, then returns it.
fn assert_raw_default<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    variable: TypeVarInstance<'db>,
    expected: Option<&str>,
) -> Option<Type<'db>> {
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            lazy_typevar_default_ingredient(db),
            variable.as_id(),
        )
        .is_ok()
    );
    let raw = raw_ordinary(db, variable);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        raw.map(|ty| ty.display(db, &env).to_string()),
        expected.map(str::to_owned),
    );
    raw
}

/// Creates distinct lazy instances of one source-defined variable by varying their eager bounds.
fn lazy_variables<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> [TypeVarInstance<'db>; 3] {
    let identity = TypeVarIdentity::new(
        db,
        Name::new_static("T"),
        Some(source_definition(prepared)),
        TypeVarKind::LegacyTypeVar,
    );
    [1, 2, 3].map(|bound| {
        TypeVarInstance::new(
            db,
            identity,
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(
                TypeVarBoundOrConstraints::UpperBound(Type::int_literal(bound)),
            )),
            None,
            Some(TypeVarDefaultEvaluation::Lazy),
        )
    })
}

/// Completed absent and present defaults are cached separately for each full lazy instance.
#[test_case::test_case("from typing import TypeVar\nT = TypeVar(\"T\")\n", None; "absent")]
#[test_case::test_case("from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n", Some("int"); "present")]
fn completed_visitor_results_use_full_instances_and_reuse_none_and_present_values(
    source: &str,
    expected_display: Option<&str>,
) {
    let db = fixture(source);
    let prepared = prepare(&db);
    let visitor = TypeVarDefaultVisitor::new(None);
    let [first, second, third] = lazy_variables(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(first.identity(&db), second.identity(&db));
    assert_eq!(second.identity(&db), third.identity(&db));
    assert_ne!(first, second);
    assert_ne!(first, third);
    assert_ne!(second, third);
    for (variable, cached) in [(first, 1), (second, 2), (third, 3)] {
        assert_missing(&db, variable);
        reset(None);
        let result = controlled(&prepared, variable, Operation::Checked(&visitor), &funded());
        let expected = assert_raw_default(&db, &prepared, variable, expected_display);
        assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)));
        assert_eq!(PENDING.get(), 1);
        assert_eq!(PREPARED.get(), 1);
        // These instances have equal raw defaults, but each full instance has its own query key.
        //
        assert_eq!(QUERIES.get(), 1);
        assert_eq!(visitor.ownership_probe_counts(), (0, cached));
        assert_cleanup();
        reset(None);
        assert_eq!(
            controlled(&prepared, variable, Operation::Checked(&visitor), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(PENDING.get(), 0);
        assert_eq!(PREPARED.get(), 0);
        assert_eq!(QUERIES.get(), 0);
        assert_eq!(visitor.ownership_probe_counts(), (0, cached));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// Reentry for the same active instance returns `None`; a different instance with the same identity
/// can complete.
#[test]
fn active_exact_reentry_does_not_merge_distinct_instances_with_one_logical_identity() {
    let db = fixture("from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n");
    let prepared = prepare(&db);
    let visitor = TypeVarDefaultVisitor::new(None);
    let [first, second, _] = lazy_variables(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(first.identity(&db), second.identity(&db));
    assert_ne!(first, second);
    let CycleDetectorVisit::Pending(scope) = visitor.try_begin_visit(&db, first, |_| true) else {
        panic!("first visit is pending");
    };
    reset(None);
    assert_eq!(
        controlled(&prepared, first, Operation::Checked(&visitor), &funded()),
        Ok(AnalysisOutcome::Complete(None)),
    );
    assert_eq!(PENDING.get(), 0);
    assert_eq!(QUERIES.get(), 0);
    assert_eq!(visitor.ownership_probe_counts(), (1, 0));
    assert_missing(&db, first);
    let result = controlled(&prepared, second, Operation::Checked(&visitor), &funded());
    let expected = assert_raw_default(&db, &prepared, second, Some("int"));
    assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)));
    assert_eq!(PENDING.get(), 1);
    assert_eq!(PREPARED.get(), 1);
    assert_eq!(QUERIES.get(), 1);
    assert_eq!(visitor.ownership_probe_counts(), (1, 1));
    drop(scope);
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    reset(None);
    assert_eq!(
        controlled(&prepared, first, Operation::Checked(&visitor), &funded()),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert_eq!(PENDING.get(), 1);
    assert_eq!(PREPARED.get(), 1);
    assert_eq!(QUERIES.get(), 1);
    assert_eq!(assert_raw_default(&db, &prepared, first, Some("int")), expected);
    assert_eq!(visitor.ownership_probe_counts(), (0, 2));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Interrupted lazy evaluation drops the pending visitor entry and retries in the same revision.
/// A raw query completed before the interruption remains available to the retry.
#[test]
fn rejected_or_cancelled_visitor_completion_drops_pending_scope_without_caching() {
    let source = "from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n";
    let measured_db = fixture(source);
    let measured_prepared = prepare(&measured_db);
    let measured_variable = variable(
        &measured_db,
        Some(source_definition(&measured_prepared)),
        Some(TypeVarDefaultEvaluation::Lazy),
    );
    let measured_visitor = TypeVarDefaultVisitor::new(None);
    reset(None);
    let measured_result = controlled(
        &measured_prepared,
        measured_variable,
        Operation::Checked(&measured_visitor),
        &funded(),
    );
    let measured_expected = assert_raw_default(
        &measured_db,
        &measured_prepared,
        measured_variable,
        Some("int"),
    );
    assert_eq!(measured_result, Ok(AnalysisOutcome::Complete(measured_expected)));
    let pending_work = funded().semantic_work_limit - PENDING_REMAINING.get().unwrap();
    let prepared_work = funded().semantic_work_limit - PREPARED_REMAINING.get().unwrap();

    for (boundary, work) in [
        (Boundary::Pending, pending_work),
        (Boundary::Prepared, prepared_work),
    ] {
        for cancel in [false, true] {
            let db = fixture(source);
            let prepared = prepare(&db);
            let variable = variable(
                &db,
                Some(source_definition(&prepared)),
                Some(TypeVarDefaultEvaluation::Lazy),
            );
            let visitor = TypeVarDefaultVisitor::new(None);
            let revision = salsa::plumbing::current_revision(&db);
            reset(cancel.then_some(boundary));
            let policy = if cancel {
                funded()
            } else {
                AnalysisPolicy {
                    semantic_work_limit: work,
                    ..funded()
                }
            };
            let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled(&prepared, variable, Operation::Checked(&visitor), &policy)
            }));
            match result {
                Err(salsa::Cancelled::Local) if cancel => {}
                Ok(result) if !cancel => assert_eq!(
                    result,
                    Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    })
                ),
                other => panic!("{boundary:?} cancel={cancel}: {other:?}"),
            }
            assert_eq!(PENDING.get(), 1);
            assert_eq!(visitor.ownership_probe_counts(), (0, 0));
            if boundary == Boundary::Prepared {
                assert_eq!(PREPARED.get(), 1);
                assert_eq!(QUERIES.get(), 1);
                // The raw child is canonical before the checked visitor result is committed.
                //
                let _ = assert_raw_default(&db, &prepared, variable, Some("int"));
            } else {
                assert_eq!(boundary, Boundary::Pending);
                assert_eq!(PREPARED.get(), 0);
                assert_eq!(QUERIES.get(), 0);
                assert_missing(&db, variable);
            }
            assert_cleanup();
            reset(None);
            let result = controlled(&prepared, variable, Operation::Checked(&visitor), &funded());
            let expected = assert_raw_default(&db, &prepared, variable, Some("int"));
            assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)));
            assert_eq!(PENDING.get(), 1);
            assert_eq!(PREPARED.get(), 1);
            assert_eq!(QUERIES.get(), usize::from(boundary == Boundary::Pending));
            assert_eq!(visitor.ownership_probe_counts(), (0, 1));
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_cleanup();
        }
    }
}

#[test]
fn checked_lazy_default_completes_after_canonical_raw_inference() {
    let db = fixture("from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n");
    let prepared = prepare(&db);
    let variable = variable(
        &db,
        Some(source_definition(&prepared)),
        Some(TypeVarDefaultEvaluation::Lazy),
    );
    let visitor = TypeVarDefaultVisitor::new(None);
    reset(None);
    let result = controlled(&prepared, variable, Operation::Checked(&visitor), &funded());
    let Ok(AnalysisOutcome::Complete(Some(checked))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            lazy_typevar_default_ingredient(&db),
            variable.as_id(),
        )
        .is_ok()
    );
    assert_eq!(
        raw_ordinary(&db, variable)
            .unwrap()
            .display(&db, &ProgramEnvironment::from_file(prepared.program_file()))
            .to_string(),
        "int",
    );
    assert_eq!(raw_ordinary(&db, variable), Some(checked));
    assert_cleanup();
}

/// Completes two lazy instances of a source variable with default `int` in the visitor's inline cache,
/// leaving a third instance cold.
fn seeded_default_visitor<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> (TypeVarDefaultVisitor<'db>, [TypeVarInstance<'db>; 3]) {
    let visitor = TypeVarDefaultVisitor::new(None);
    let variables = lazy_variables(db, prepared);
    for variable in &variables[..2] {
        reset(None);
        let result = controlled(prepared, *variable, Operation::Checked(&visitor), &funded());
        let expected = assert_raw_default(db, prepared, *variable, Some("int"));
        assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)));
        assert_cleanup();
    }
    assert_eq!(visitor.ownership_probe_counts(), (0, 2));
    assert_eq!(visitor.ownership_probe_storage().cache_capacity, None);
    (visitor, variables)
}

/// Refusing cache-promotion storage admission preserves the inline defaults and allows a retry.
#[test]
fn cache_promotion_allocation_refusal_preserves_completed_defaults_and_allows_retry() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture("from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n");
        let prepared = prepare(&db);
        let (visitor, variables) = seeded_default_visitor(&db, &prepared);
        reset(None);
        let result = controlled(
            &prepared,
            variables[2],
            Operation::Checked(&visitor),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
        );
        match result {
            Ok(AnalysisOutcome::Complete(Some(value))) => {
                assert_eq!(
                    Some(value),
                    assert_raw_default(&db, &prepared, variables[2], Some("int")),
                );
                assert_eq!(PREPARED.get(), 1);
            }
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                completed: (),
            }) => {}
            other => panic!("{other:?}"),
        }
        // Promotion precedes further finish admissions and the Prepared marker. Locate
        // promotion itself so upper - 1 refuses storage growth before it changes the cache.
        if visitor.ownership_probe_storage().cache_capacity.is_some() {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_eq!(visitor.ownership_probe_counts().0, 0);
        assert_cleanup();
    }
    assert!(upper > 0);

    let db = fixture("from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n");
    let prepared = prepare(&db);
    let (visitor, variables) = seeded_default_visitor(&db, &prepared);
    let before = visitor.ownership_probe_storage();
    let revision = salsa::plumbing::current_revision(&db);
    reset(None);
    assert_eq!(
        controlled(
            &prepared,
            variables[2],
            Operation::Checked(&visitor),
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
    assert_eq!(PENDING.get(), 1);
    assert_eq!(PREPARED.get(), 0);
    assert_eq!(visitor.ownership_probe_counts(), (0, 2));
    assert_eq!(visitor.ownership_probe_storage(), before);
    assert_eq!(QUERIES.get(), 1);
    let expected = assert_raw_default(&db, &prepared, variables[2], Some("int"));
    assert_cleanup();

    for variable in &variables[..2] {
        reset(None);
        assert_eq!(
            controlled(
                &prepared,
                *variable,
                Operation::Checked(&visitor),
                &funded()
            ),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(PENDING.get(), 0);
        assert_eq!(PREPARED.get(), 0);
        assert_eq!(QUERIES.get(), 0);
        assert_eq!(visitor.ownership_probe_counts(), (0, 2));
        assert_cleanup();
    }

    reset(None);
    assert_eq!(
        controlled(
            &prepared,
            variables[2],
            Operation::Checked(&visitor),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert_eq!(PENDING.get(), 1);
    assert_eq!(PREPARED.get(), 1);
    assert_eq!(QUERIES.get(), 0);
    let after = visitor.ownership_probe_storage();
    assert_eq!((after.active_len, after.cache_len), (0, 3));
    assert!(after.cache_capacity.is_some());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn checked_lazy_default_reuses_a_warm_raw_value() {
    let db = fixture("from typing import TypeVar\nT = TypeVar(\"T\", default=int)\n");
    let prepared = prepare(&db);
    let variable = variable(
        &db,
        Some(source_definition(&prepared)),
        Some(TypeVarDefaultEvaluation::Lazy),
    );
    let raw = raw_ordinary(&db, variable).unwrap();
    let visitor = TypeVarDefaultVisitor::new(None);
    reset(None);
    assert_eq!(
        controlled(&prepared, variable, Operation::Checked(&visitor), &funded()),
        Ok(AnalysisOutcome::Complete(Some(raw))),
    );
    assert_eq!(QUERIES.get(), 0);
    assert_eq!(visitor.ownership_probe_counts(), (0, 1));
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            lazy_typevar_default_ingredient(&db),
            variable.as_id(),
        )
        .is_ok()
    );
    assert_cleanup();
}

pub(in crate::types::infer) mod paramspec_conversion;
