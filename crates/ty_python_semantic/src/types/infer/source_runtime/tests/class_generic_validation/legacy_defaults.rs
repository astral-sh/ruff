//! Legacy ordering reports finish before the still-unsupported explicit-base scan.

use std::panic::AssertUnwindSafe;

use super::interruptions::{Resource, assert_cleanup};
use super::*;
use crate::types::infer::infer_scope_types;

/// Supplies one default-bearing variable and enough later variables to interrupt retained storage.
fn source(base: &str, suppression: &str) -> String {
    format!(
        "from typing import Generic, Protocol, TypeVar\n\
         T = TypeVar(\"T\", default=int)\n\
         U = TypeVar(\"U\")\n\
         V = TypeVar(\"V\")\n\
         W = TypeVar(\"W\")\n\
         X = TypeVar(\"X\")\n\
         class Target({base}): ...{suppression}\n"
    )
}

/// Builds a fresh database from the supplied source using Python 3.13.
fn fixture(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()
}

/// Returns full invalid-generic-class diagnostics from a fresh ordinary database and verifies
/// the ordinary scope's suppression-use count independently of the controlled attempts.
fn ordinary_reports(source: &str, expected_used: usize) -> anyhow::Result<Vec<Diagnostic>> {
    let db = fixture(source)?;
    let prepared = prepare(&db);
    let (scope, _) = fixture_target(&db, &prepared, &["Target"])?;
    let inference = infer_scope_types(&db, scope, TypeContext::default());
    let used = inference.diagnostics().map(TypeCheckDiagnostics::used_len).unwrap_or(0);
    assert_eq!(used, expected_used);
    Ok(crate::check_file_unwrap(&db, prepared.program_file())
        .into_iter()
        .filter(|diagnostic| diagnostic.id() == DiagnosticId::lint("invalid-generic-class"))
        .collect())
}

/// Selects the first TypeVar assignment without warming its canonical definition query.
fn default_definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> anyhow::Result<ty_python_core::definition::Definition<'db>> {
    let Some(assignment) = prepared.parsed_module().syntax().body.iter().find_map(Stmt::as_assign_stmt) else {
        anyhow::bail!("legacy ordering fixture has no TypeVar assignment");
    };
    let Some(target) = assignment.targets.first().and_then(ast::Expr::as_name_expr) else {
        anyhow::bail!("legacy ordering fixture assignment has no target");
    };
    prepared.semantic_index().try_definition(target)
        .ok_or_else(|| anyhow::anyhow!("legacy ordering fixture has no TypeVar definition"))
}

/// Returns retained invalid-generic-class diagnostics after requiring all three generic scans
/// to finish and the explicit-base scan to refuse with the scope still unpublished.
fn completed_reports<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    scope: ScopeId<'db>,
    expected_used: usize,
) -> anyhow::Result<Vec<Diagnostic>> {
    let revision = salsa::plumbing::current_revision(db);
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(db));
    let result = controlled_scope(prepared, scope, &funded());
    let events = recording.take_events();
    drop(recording);
    assert_cleanup(db, scope, revision);
    assert_eq!(
        result,
        Ok(unavailable(OperationId::ClassCheck(ClassCheckOperation::GenericContext))),
        "{events:?}",
    );
    let stages = events.iter().filter_map(|event| match event {
        Event::Completed(_, stage, _, _) => Some(*stage),
        Event::Boundary(_, _, _) | Event::Inserted(_, _, _) => None,
    }).collect::<Vec<_>>();
    assert_eq!(stages, [
        ValidationStage::LegacyDefaultOrder,
        ValidationStage::DefaultReferences,
        ValidationStage::OwnShadowing,
    ], "{events:?}");
    let Some(Event::Completed(_, ValidationStage::OwnShadowing, diagnostics, used)) = events.last() else {
        anyhow::bail!("legacy ordering attempt did not finish its generic checks: {events:?}");
    };
    assert_eq!(*used, expected_used);
    Ok(diagnostics.iter()
        .filter(|diagnostic| diagnostic.id() == DiagnosticId::lint("invalid-generic-class"))
        .cloned().collect())
}

/// Valid order emits no report; invalid order preserves singular/plural messages and suppression.
/// Cold inference and retry leave the enclosing scope unpublished, reuse the canonical TypeVar
/// definition, and match complete diagnostics from an independent ordinary database.
#[test_case::test_case("Generic[U, T]", "", 0, 0; "valid order")]
#[test_case::test_case("Generic[T, U]", "", 1, 0; "single offender")]
#[test_case::test_case("Generic[T, U, V, W]", "", 1, 0; "three offenders")]
#[test_case::test_case("Protocol[T, U, V, W]", "", 1, 0; "protocol offenders")]
#[test_case::test_case("Generic[T, U, V, W]", "  # ty: ignore[invalid-generic-class]", 0, 1; "suppressed order")]
fn cold_legacy_order_matches_fresh_ordinary(
    base: &str,
    suppression: &str,
    expected_reports: usize,
    expected_used: usize,
) -> anyhow::Result<()> {
    let source = source(base, suppression);
    let db = fixture(&source)?;
    let prepared = prepare(&db);
    let (scope, _) = fixture_target(&db, &prepared, &["Target"])?;
    let definition = default_definition(&prepared)?;
    assert_cleanup(&db, scope, salsa::plumbing::current_revision(&db));
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, definition_inference_ingredient(&db), definition.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let cold = completed_reports(&db, &prepared, scope, expected_used)?;
    assert_eq!(cold.len(), expected_reports);
    let ingredient = definition_inference_ingredient(&db);
    let Ok(certified) = FinalSourceMemo::certify(&db as &dyn Db, ingredient, definition.as_id()) else {
        anyhow::bail!("controlled scan did not leave a complete TypeVar definition memo");
    };
    assert_eq!(certified.database_key(), ingredient.database_key_index(definition.as_id()));
    let canonical = infer_definition_types(&db, definition);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let retry = completed_reports(&db, &prepared, scope, expected_used)?;
    assert_eq!(cold, retry);
    assert!(std::ptr::eq(canonical, infer_definition_types(&db, definition)));
    assert_function_query_was_not_run_by_name(
        &db, "infer_definition_types", Some(definition.as_id()), &events_db.take_salsa_events(),
    );
    let ordinary = ordinary_reports(&source, expected_used)?;
    assert_eq!(cold, ordinary);
    if expected_reports == 1 {
        let [report] = cold.as_slice() else {
            anyhow::bail!("expected exactly one legacy ordering report");
        };
        assert_eq!(report.headline_message(), "Type parameters without defaults cannot follow type parameters with defaults");
        assert_eq!(report.concise_message().to_string(), "Type parameter `U` without a default cannot follow earlier parameter `T` with a default");
    }
    Ok(())
}

/// Cancellation with a partially retained offender buffer or during reporting drains the scan.
/// A funded same-revision retry retains the complete ordinary diagnostic before the base refusal.
#[test_case::test_case(ReportBoundary::Retention; "retained offenders")]
#[test_case::test_case(ReportBoundary::Construction; "report construction")]
#[test_case::test_case(ReportBoundary::Insertion; "report insertion")]
fn cancelled_legacy_order_retries(boundary: ReportBoundary) -> anyhow::Result<()> {
    let source = source("Generic[T, U, V, W, X]", "");
    let db = fixture(&source)?;
    let prepared = prepare(&db);
    let (scope, _) = fixture_target(&db, &prepared, &["Target"])?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::with_cancellation(
        prepared.program_file().file(&db),
        Some(Cancellation {
            stage: ValidationStage::LegacyDefaultOrder,
            boundary,
            token: db.cancellation_token(),
        }),
    );
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| controlled_scope(&prepared, scope, &funded())));
    let cancellation_requested = recording.cancellation_requested();
    let events = recording.take_events();
    drop(recording);
    assert!(cancellation_requested, "{result:?}\n{events:?}");
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}\n{events:?}");
    assert_cleanup(&db, scope, revision);
    let retry = completed_reports(&db, &prepared, scope, 0)?;
    let ordinary = ordinary_reports(&source, 0)?;
    assert_eq!(retry, ordinary);
    Ok(())
}

/// Selects an admission after partial retention or immediately before diagnostic insertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    SecondRetention,
    Insertion,
}

impl Admission {
    /// Reports whether recorded events reached the selected retention or insertion boundary.
    fn reached(self, events: &[Event]) -> bool {
        match self {
            Self::SecondRetention => events.iter().filter(|event| matches!(event,
                Event::Boundary(_, ValidationStage::LegacyDefaultOrder, ReportBoundary::Retention)
            )).count() >= 2,
            Self::Insertion => events.iter().any(|event| matches!(event,
                Event::Inserted(_, ValidationStage::LegacyDefaultOrder, _)
            )),
        }
    }
}

/// Runs a fresh attempt under the supplied resource limit and reports whether it reaches the boundary.
fn reaches_admission(source: &str, admission: Admission, resource: Resource, limit: usize) -> anyhow::Result<bool> {
    let db = fixture(source)?;
    let prepared = prepare(&db);
    let (scope, _) = fixture_target(&db, &prepared, &["Target"])?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(&db));
    let _result = controlled_scope(&prepared, scope, &resource.policy(limit));
    let events = recording.take_events();
    drop(recording);
    assert_cleanup(&db, scope, revision);
    Ok(admission.reached(&events))
}

/// Independent work and byte refusals preserve cleanup after partial retention and before insertion.
/// Retry uses the unchanged normal policy and matches all ordinary diagnostic values.
#[test_case::test_case(Admission::SecondRetention, Resource::Work; "partial retention work")]
#[test_case::test_case(Admission::SecondRetention, Resource::Bytes; "partial retention bytes")]
#[test_case::test_case(Admission::Insertion, Resource::Work; "insertion work")]
#[test_case::test_case(Admission::Insertion, Resource::Bytes; "insertion bytes")]
fn refused_legacy_order_retries(admission: Admission, resource: Resource) -> anyhow::Result<()> {
    let source = source("Generic[T, U, V, W, X]", "");
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches_admission(&source, admission, resource, high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_admission(&source, admission, resource, middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = fixture(&source)?;
    let prepared = prepare(&db);
    let (scope, _) = fixture_target(&db, &prepared, &["Target"])?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(&db));
    let result = controlled_scope(&prepared, scope, &resource.policy(low));
    let events = recording.take_events();
    drop(recording);
    assert_eq!(result, Ok(AnalysisOutcome::Incomplete { reason: resource.reason(), completed: () }), "{events:?}");
    assert!(!admission.reached(&events), "{events:?}");
    match admission {
        Admission::SecondRetention => assert_eq!(events.iter().filter(|event| matches!(event,
            Event::Boundary(_, ValidationStage::LegacyDefaultOrder, ReportBoundary::Retention)
        )).count(), 1, "{events:?}"),
        Admission::Insertion => assert!(events.iter().any(|event| matches!(event,
            Event::Boundary(_, ValidationStage::LegacyDefaultOrder, ReportBoundary::Insertion)
        )), "{events:?}"),
    }
    assert_cleanup(&db, scope, revision);
    let retry = completed_reports(&db, &prepared, scope, 0)?;
    let ordinary = ordinary_reports(&source, 0)?;
    assert_eq!(retry, ordinary);
    Ok(())
}
