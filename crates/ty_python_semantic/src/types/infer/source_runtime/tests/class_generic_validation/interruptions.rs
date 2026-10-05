//! Resource refusal and native cancellation during real class-generic reporting.

use std::panic::AssertUnwindSafe;

use super::*;

/// Selects an invalid default or an enclosing-name collision, each producing one report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    DefaultReference,
    OwnShadowing,
}

impl Case {
    const fn fixture(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::DefaultReference => ("class Target[U = T, T = int]: ...\n", &["Target"]),
            Self::OwnShadowing => (
                "class Outer[T]:\n    class Target[T]: ...\n",
                &["Outer", "Target"],
            ),
        }
    }

    const fn stage(self) -> ValidationStage {
        match self {
            Self::DefaultReference => ValidationStage::DefaultReferences,
            Self::OwnShadowing => ValidationStage::OwnShadowing,
        }
    }
}

/// Selects which cumulative resource is exhausted, leaving the other at its normal limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Resource {
    Work,
    Bytes,
}

impl Resource {
    pub(super) fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    pub(super) fn policy(self, limit: usize) -> AnalysisPolicy {
        match self {
            Self::Work => AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
            Self::Bytes => AnalysisPolicy {
                requested_bytes_limit: limit,
                ..funded()
            },
        }
    }

    pub(super) const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Checks builder counts, idle analysis state, and the absence of the unfinished scope memo.
pub(super) fn assert_cleanup<'db>(db: &'db TestDb, scope: ScopeId<'db>, revision: salsa::Revision) {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(db, || ()),
        Ok(())
    );
    assert_eq!(salsa::plumbing::current_revision(db), revision);
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            scope_inference_ingredient(db),
            InferScope::Bare(scope).as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
}

/// Retries after interruption and checks the full retained report against ordinary checking afterward.
/// Later unsupported class checks still prevent publication of the scope memo.
fn check_retry<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    scope: ScopeId<'db>,
    range: TextRange,
    stage: ValidationStage,
    revision: salsa::Revision,
) -> anyhow::Result<()> {
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(db));
    let retry = controlled_scope(prepared, scope, &funded());
    let events = recording.take_events();
    drop(recording);
    assert!(
        matches!(
            retry,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(_),
                ..
            })
        ),
        "{retry:?}\n{events:?}"
    );
    assert_cleanup(db, scope, revision);
    let Some(Event::Completed(_, ValidationStage::OwnShadowing, diagnostics, used)) = events.last()
    else {
        anyhow::bail!("retry did not finish both class-generic scans: {events:?}");
    };
    assert_eq!(*used, 0);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Inserted(_, actual, _) if *actual == stage))
            .count(),
        1,
        "{events:?}"
    );
    let selected = |diagnostic: &Diagnostic| {
        diagnostic.primary_span().and_then(|span| span.range()) == Some(range)
            && [
                DiagnosticId::lint("invalid-generic-class"),
                DiagnosticId::lint("shadowed-type-variable"),
            ]
            .contains(&diagnostic.id())
    };
    let retained = diagnostics
        .iter()
        .filter(|d| selected(d))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(retained.len(), 1);
    let ordinary = crate::check_file_unwrap(db, prepared.program_file())
        .into_iter()
        .filter(selected)
        .collect::<Vec<_>>();
    assert_eq!(retained, ordinary);
    Ok(())
}

/// Native cancellation at report eligibility, construction, or insertion leaves the scope
/// unpublished. Retrying in the same revision completes both scans and retains the exact report,
/// before later unsupported class checks stop scope inference.
#[test_case::test_case(Case::DefaultReference, ReportBoundary::Eligibility; "default eligibility")]
#[test_case::test_case(Case::DefaultReference, ReportBoundary::Construction; "default construction")]
#[test_case::test_case(Case::DefaultReference, ReportBoundary::Insertion; "default insertion")]
#[test_case::test_case(Case::OwnShadowing, ReportBoundary::Eligibility; "shadow eligibility")]
#[test_case::test_case(Case::OwnShadowing, ReportBoundary::Construction; "shadow construction")]
#[test_case::test_case(Case::OwnShadowing, ReportBoundary::Insertion; "shadow insertion")]
fn cancelled_reports_retry_in_the_same_revision(
    case: Case,
    boundary: ReportBoundary,
) -> anyhow::Result<()> {
    let (source, path) = case.fixture();
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()?;
    let prepared = prepare(&db);
    let (scope, range) = fixture_target(&db, &prepared, path)?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::with_cancellation(
        prepared.program_file().file(&db),
        Some(Cancellation {
            stage: case.stage(),
            boundary,
            token: db.cancellation_token(),
        }),
    );
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_scope(&prepared, scope, &funded())
    }));
    let cancellation_requested = recording.cancellation_requested();
    let events = recording.take_events();
    drop(recording);
    assert!(cancellation_requested, "{result:?}\n{events:?}");
    assert!(
        matches!(result, Err(salsa::Cancelled::Local)),
        "{result:?}\n{events:?}"
    );
    assert_cleanup(&db, scope, revision);
    check_retry(&db, &prepared, scope, range, case.stage(), revision)
}

/// Observes real insertion on a fresh cold database under the selected resource limit.
fn insertion_completes(case: Case, resource: Resource, limit: usize) -> anyhow::Result<bool> {
    let (source, path) = case.fixture();
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()?;
    let prepared = prepare(&db);
    let (scope, _) = fixture_target(&db, &prepared, path)?;
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(&db));
    let _result = controlled_scope(&prepared, scope, &resource.policy(limit));
    let events = recording.take_events();
    drop(recording);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    Ok(events
        .iter()
        .any(|event| matches!(event, Event::Inserted(_, stage, _) if *stage == case.stage())))
}

/// Independent work and byte limits refuse the final report insertion before it mutates the
/// builder. Cold probes locate this admission without fixing the test to particular cost constants;
/// the interrupted database then retries under the unchanged normal limits in the same revision.
#[test_case::test_case(Case::DefaultReference, Resource::Work; "default work")]
#[test_case::test_case(Case::DefaultReference, Resource::Bytes; "default bytes")]
#[test_case::test_case(Case::OwnShadowing, Resource::Work; "shadow work")]
#[test_case::test_case(Case::OwnShadowing, Resource::Bytes; "shadow bytes")]
fn refused_insertions_remain_unpublished_and_retry(
    case: Case,
    resource: Resource,
) -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(insertion_completes(case, resource, high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if insertion_completes(case, resource, middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let (source, path) = case.fixture();
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()?;
    let prepared = prepare(&db);
    let (scope, range) = fixture_target(&db, &prepared, path)?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(&db));
    let result = controlled_scope(&prepared, scope, &resource.policy(low));
    let events = recording.take_events();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        }),
        "{events:?}"
    );
    assert!(events.iter().any(|event| matches!(event, Event::Boundary(_, stage, ReportBoundary::Insertion) if *stage == case.stage())), "{events:?}");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Inserted(_, stage, _) if *stage == case.stage())),
        "{events:?}"
    );
    assert_cleanup(&db, scope, revision);
    check_retry(&db, &prepared, scope, range, case.stage(), revision)
}
