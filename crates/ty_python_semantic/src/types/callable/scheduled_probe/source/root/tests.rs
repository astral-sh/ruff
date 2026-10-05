use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::{
    assert_function_query_was_not_run_by_name, assert_function_query_was_run,
    find_will_execute_event_by_name,
};
use ruff_python_ast::PythonVersion;
use salsa::Database;
use salsa::plumbing::AsId;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::typevar::TypeVarKind;

const ORDERS: [(bool, bool); 4] = [(false, false), (false, true), (true, false), (true, true)];
const COMPLETE: RootPolicy = RootPolicy {
    budget: 10_000,
    reverse_execution: false,
    reverse_merge: false,
};

fn fixture(source: Option<&str>) -> anyhow::Result<(TestDb, File)> {
    let mut builder = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/main.py", "");
    if let Some(source) = source {
        builder = builder.with_file("/src/dependency.py", source);
    }
    let db = builder.build()?;
    let origin = system_path_to_file(&db, "/src/main.py")?;
    Ok((db, origin))
}

fn request(db: &TestDb, origin: File, policy: RootPolicy) -> ContextRootRequest<'_> {
    ContextRootRequest::new(
        db,
        db.program_file(origin),
        ModuleName::new_static("dependency").expect("valid module name"),
        Name::new_static("generic"),
        policy,
    )
}

#[derive(Debug, Eq, PartialEq)]
struct HeaderObservation {
    name: String,
    kind: TypeVarKind,
    deferred: bool,
    constrained: bool,
    invalid_constraint_count: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    source: Option<File>,
    context: Result<Vec<String>, Incomplete>,
    headers: Vec<HeaderObservation>,
    outstanding_headers: usize,
    outstanding_contexts: usize,
    work: usize,
}

fn observe<'db>(db: &'db TestDb, outcome: &ContextRootOutcome<'db>) -> Observation {
    Observation {
        source: outcome.source.map(|source| source.file(db)),
        context: match outcome.context {
            Completion::Complete(context) => Ok(context
                .variables(db)
                .map(|variable| variable.name(db).to_string())
                .collect()),
            Completion::Incomplete(reason) => Err(reason),
        },
        headers: outcome
            .headers
            .iter()
            .map(|(_, header)| HeaderObservation {
                name: header.variable.name(db).to_string(),
                kind: header.variable.kind(db),
                deferred: header.deferred.is_some(),
                constrained: header.variable.is_constrained(db),
                invalid_constraint_count: header.invalid_constraint_count.is_some(),
            })
            .collect(),
        outstanding_headers: outcome
            .outstanding
            .iter()
            .filter(|obligation| matches!(obligation, SourceObligation::Header(_)))
            .count(),
        outstanding_contexts: outcome
            .outstanding
            .iter()
            .filter(|obligation| matches!(obligation, SourceObligation::GenericContext(_)))
            .count(),
        work: outcome.work,
    }
}

fn assert_no_inference(db: &TestDb, events: &[salsa::Event]) {
    for query in [
        "infer_implicit_alias_type",
        "infer_definition_types",
        "infer_deferred_types",
        "infer_function_default_types",
        "infer_scope_types_impl",
        "infer_expression_types_impl",
        "infer_expression_type_impl",
        "infer_statement_types_impl",
        "infer_unpack_types",
        "infer_protocol_variance",
        "function_known_decorators",
    ] {
        assert_function_query_was_not_run_by_name(db, query, None, events);
    }
}

fn evaluate(
    db: &mut TestDb,
    origin: File,
    policy: RootPolicy,
    executes: bool,
) -> (Observation, Vec<salsa::Event>) {
    db.clear_salsa_events();
    let request = request(db, origin, policy);
    let observation = observe(db, evaluate_prepared_context_root(db, request));
    let request_id = request.as_id();
    let events = db.take_salsa_events();
    assert_eq!(
        find_will_execute_event_by_name(
            db,
            "evaluate_prepared_context_root",
            Some(request_id),
            &events,
        )
        .is_some(),
        executes,
        "{events:#?}",
    );
    assert_no_inference(db, &events);
    (observation, events)
}

fn assert_context(observation: &Observation, names: &[&str]) {
    assert_eq!(
        observation
            .context
            .as_ref()
            .map(|context| context.iter().map(String::as_str).collect::<Vec<_>>()),
        Ok(names.to_vec()),
    );
    assert_eq!(observation.outstanding_headers, 0);
    assert_eq!(observation.outstanding_contexts, 0);
    assert_eq!(
        observation
            .headers
            .iter()
            .map(|header| header.name.as_str())
            .collect::<Vec<_>>(),
        names,
    );
}

#[test]
fn headers_and_module_resolution_invalidate_the_root() -> anyhow::Result<()> {
    let (mut db, origin) = fixture(Some("def generic[T](): ...\n"))?;
    let (first, events) = evaluate(&mut db, origin, COMPLETE, true);
    assert_context(&first, &["T"]);
    let dependency = system_path_to_file(&db, "/src/dependency.py")?;
    assert_eq!(first.source, Some(dependency));
    let source = db.program_file(dependency);
    assert_function_query_was_run(&db, parsed_module, source.python_file(&db), &events);
    assert_function_query_was_run(&db, semantic_index, source, &events);
    assert!(find_will_execute_event_by_name(&db, "resolve_module_query", None, &events).is_some());
    assert_eq!(evaluate(&mut db, origin, COMPLETE, false).0, first);

    db.write_file("/src/dependency.py", "def generic[T, U](): ...\n")?;
    let (edited, events) = evaluate(&mut db, origin, COMPLETE, true);
    assert_context(&edited, &["T", "U"]);
    let source = db.program_file(dependency);
    assert_function_query_was_run(&db, parsed_module, source.python_file(&db), &events);
    assert_function_query_was_run(&db, semantic_index, source, &events);

    db.write_file("/src/dependency.pyi", "def generic[Stub](): ...\n")?;
    let (stubbed, events) = evaluate(&mut db, origin, COMPLETE, true);
    assert_context(&stubbed, &["Stub"]);
    assert_eq!(
        stubbed.source,
        Some(system_path_to_file(&db, "/src/dependency.pyi")?),
    );
    assert!(find_will_execute_event_by_name(&db, "resolve_module_query", None, &events).is_some());

    db.write_file("/src/dependency.py", "def generic[Shadowed](): ...\n")?;
    assert_eq!(evaluate(&mut db, origin, COMPLETE, false).0, stubbed);
    Ok(())
}

#[test]
fn creating_a_missing_module_invalidates_its_preparation_failure() -> anyhow::Result<()> {
    let (mut db, origin) = fixture(None)?;
    let (missing, _) = evaluate(&mut db, origin, COMPLETE, true);
    assert_eq!(
        missing.context,
        Err(Incomplete::Preparation(PreparationFailure::MissingModule)),
    );
    assert_eq!(missing.source, None);
    assert_eq!(evaluate(&mut db, origin, COMPLETE, false).0, missing);

    db.write_file("/src/dependency.py", "def generic[Created](): ...\n")?;
    let (created, events) = evaluate(&mut db, origin, COMPLETE, true);
    assert_context(&created, &["Created"]);
    assert!(find_will_execute_event_by_name(&db, "resolve_module_query", None, &events).is_some());
    Ok(())
}

#[test]
fn every_allowance_preserves_header_evidence_in_opposite_orders() -> anyhow::Result<()> {
    let (mut db, origin) = fixture(Some(
        "def generic[Z, A: (MissingA, MissingB), *Ts, **P, Invalid: ()](): ...\n",
    ))?;
    let (complete, _) = evaluate(&mut db, origin, COMPLETE, true);
    assert_context(&complete, &["Z", "A", "Ts", "P", "Invalid"]);
    assert_eq!(complete.headers[1].kind, TypeVarKind::Pep695TypeVar);
    assert!(complete.headers[1].constrained);
    assert!(complete.headers[1].deferred);
    assert_eq!(complete.headers[2].kind, TypeVarKind::Pep695TypeVarTuple);
    assert_eq!(complete.headers[3].kind, TypeVarKind::Pep695ParamSpec);
    assert!(complete.headers[4].invalid_constraint_count);

    let mut saw_partial_headers = false;
    for budget in 0..=complete.work {
        let policy = RootPolicy { budget, ..COMPLETE };
        let (baseline, _) = evaluate(&mut db, origin, policy, true);
        assert!(baseline.work <= budget);
        assert!(
            baseline
                .headers
                .iter()
                .all(|header| complete.headers.contains(header))
        );
        if budget == complete.work {
            assert_eq!(baseline, complete);
        }
        saw_partial_headers |= baseline.context.is_err() && !baseline.headers.is_empty();
        for (reverse_execution, reverse_merge) in ORDERS.into_iter().skip(1) {
            let (reordered, _) = evaluate(
                &mut db,
                origin,
                RootPolicy {
                    reverse_execution,
                    reverse_merge,
                    ..policy
                },
                true,
            );
            assert_eq!(reordered, baseline, "budget {budget}");
        }
    }
    assert!(saw_partial_headers);
    Ok(())
}

#[test]
fn cached_partial_outcomes_observe_header_and_resolution_changes() -> anyhow::Result<()> {
    let (mut db, origin) = fixture(Some("def generic[T](): ...\n"))?;
    let (complete, _) = evaluate(&mut db, origin, COMPLETE, true);
    let partial = (0..complete.work).find_map(|budget| {
        let policy = RootPolicy { budget, ..COMPLETE };
        let (outcome, _) = evaluate(&mut db, origin, policy, true);
        (outcome.context == Err(Incomplete::Allowance) && !outcome.headers.is_empty())
            .then_some((policy, outcome))
    });
    let (policy, partial) = partial.ok_or_else(|| anyhow::anyhow!("no partial header outcome"))?;
    assert_eq!(partial.headers[0].name, "T");
    assert_eq!(evaluate(&mut db, origin, policy, false).0, partial);

    db.write_file("/src/dependency.py", "def generic[U](): ...\n")?;
    let (edited, _) = evaluate(&mut db, origin, policy, true);
    assert_eq!(edited.context, Err(Incomplete::Allowance));
    assert_eq!(edited.headers[0].name, "U");

    db.write_file("/src/dependency.pyi", "def generic[V](): ...\n")?;
    let (stubbed, _) = evaluate(&mut db, origin, policy, true);
    assert_eq!(stubbed.context, Err(Incomplete::Allowance));
    assert_eq!(stubbed.headers[0].name, "V");
    assert_eq!(
        stubbed.source,
        Some(system_path_to_file(&db, "/src/dependency.pyi")?),
    );
    Ok(())
}

#[test]
fn cancellation_with_pending_work_does_not_cache_an_outcome() -> anyhow::Result<()> {
    let (mut db, origin) = fixture(Some("def generic[T, U](): ...\n"))?;
    db.clear_salsa_events();
    let request = request(&db, origin, COMPLETE);
    CANCEL_NEXT_ROOT.set(Some((1, db.cancellation_token())));
    let cancelled = salsa::Cancelled::catch(|| evaluate_prepared_context_root(&db, request));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert!(CANCEL_NEXT_ROOT.take().is_none());
    let events = db.clone().take_salsa_events();
    assert_function_query_was_run(&db, evaluate_prepared_context_root, request, &events);
    assert_no_inference(&db, &events);

    let (retried, _) = evaluate(&mut db, origin, COMPLETE, true);
    assert_context(&retried, &["T", "U"]);
    assert_eq!(evaluate(&mut db, origin, COMPLETE, false).0, retried);
    Ok(())
}
