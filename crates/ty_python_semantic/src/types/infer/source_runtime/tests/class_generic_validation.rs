//! Observes class default-order, default-reference and type-variable-shadowing checks during cold inference.
//!
//! Completed checks retain diagnostics in the unpublished scope builder. These controls retry
//! in the same revision before asking ordinary checking for its result.

pub(in crate::types::infer) mod base_typevars;
mod interruptions;
mod legacy_defaults;
mod outer_gate;

use std::cell::RefCell;

use ruff_db::diagnostic::{Diagnostic, DiagnosticId};
use ruff_db::files::File;
use ruff_text_size::{Ranged, TextRange};

use super::*;
use crate::lint::RuleSelection;
use crate::types::diagnostic::TypeCheckDiagnostics;

/// Identifies the class-generic check that completed or reached a reporting boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ValidationStage {
    LegacyDefaultOrder,
    DefaultReferences,
    OwnShadowing,
    BaseShadowing,
}

/// Identifies a retained-scan or report boundary before the next operation can be interrupted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ReportBoundary {
    Retention,
    Eligibility,
    Construction,
    Insertion,
}

/// Separates report entry, completed insertion and successful scan return.
#[derive(Debug)]
enum Event {
    Boundary(salsa::Id, ValidationStage, ReportBoundary),
    Inserted(salsa::Id, ValidationStage, Vec<Diagnostic>),
    Completed(salsa::Id, ValidationStage, Vec<Diagnostic>, usize),
}

/// Records only checks in the fixture file, without reading semantic fields.
#[derive(Debug)]
struct Journal {
    file: File,
    events: Vec<Event>,
    cancellation: Option<Cancellation>,
    cancellation_requested: bool,
}

/// Requests native cancellation at one observed boundary without changing provider results.
#[derive(Debug)]
struct Cancellation {
    stage: ValidationStage,
    boundary: ReportBoundary,
    token: salsa::CancellationToken,
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

/// Records diagnostics only after a class-generic scan has returned successfully.
pub(in crate::types::infer) fn validation_completed(
    file: File,
    class: StaticClassLiteral<'_>,
    stage: ValidationStage,
    diagnostics: &TypeCheckDiagnostics,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.file == file
        {
            journal.events.push(Event::Completed(
                class.as_id(),
                stage,
                diagnostics.into_iter().cloned().collect(),
                diagnostics.used_len(),
            ));
        }
    });
}

/// Records a completed offender retention or entry into a report operation without replacing it.
pub(in crate::types::infer) fn report_boundary(
    file: File,
    class: StaticClassLiteral<'_>,
    stage: ValidationStage,
    boundary: ReportBoundary,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.file == file
        {
            journal
                .events
                .push(Event::Boundary(class.as_id(), stage, boundary));
            if journal
                .cancellation
                .as_ref()
                .is_some_and(|request| request.stage == stage && request.boundary == boundary)
                && let Some(request) = journal.cancellation.take()
            {
                journal.cancellation_requested = true;
                request.token.cancel();
            }
        }
    });
}

/// Clones diagnostics after insertion, so observation cannot break a draft's unique ownership.
pub(in crate::types::infer) fn diagnostic_inserted(
    file: File,
    class: StaticClassLiteral<'_>,
    stage: ValidationStage,
    diagnostics: &TypeCheckDiagnostics,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.file == file
        {
            journal.events.push(Event::Inserted(
                class.as_id(),
                stage,
                diagnostics.into_iter().cloned().collect(),
            ));
        }
    });
}

/// Removes the thread-local recorder on normal return and during unwinding.
#[derive(Debug)]
struct Recording;

impl Recording {
    fn start(file: File) -> Self {
        Self::with_cancellation(file, None)
    }

    fn with_cancellation(file: File, cancellation: Option<Cancellation>) -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(journal.is_none());
            *journal = Some(Journal {
                file,
                events: Vec::new(),
                cancellation,
                cancellation_requested: false,
            });
        });
        Self
    }

    fn cancellation_requested(&self) -> bool {
        JOURNAL.with_borrow(|journal| {
            journal
                .as_ref()
                .is_some_and(|journal| journal.cancellation_requested)
        })
    }

    fn take_events(&self) -> Vec<Event> {
        JOURNAL.with_borrow_mut(|journal| {
            journal
                .as_mut()
                .map(|journal| std::mem::take(&mut journal.events))
                .unwrap_or_default()
        })
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| *journal = None);
    }
}

/// Finds the class and its enclosing body scope using only the prepared syntax and scope index.
fn fixture_target<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    path: &[&str],
) -> anyhow::Result<(ScopeId<'db>, TextRange)> {
    let mut suite = prepared.parsed_module().syntax().body.as_slice();
    let mut parent = None;
    let mut target = None;
    for name in path {
        let Some(class) = suite
            .iter()
            .filter_map(Stmt::as_class_def_stmt)
            .find(|class| class.name.as_str() == *name)
        else {
            anyhow::bail!("fixture has no class at {path:?}");
        };
        parent = target;
        target = Some(class);
        suite = &class.body;
    }
    let Some(target) = target else {
        anyhow::bail!("fixture class path is empty");
    };
    let scope = if let Some(parent) = parent {
        prepared
            .semantic_index()
            .scope_ids()
            .find(|scope| {
                scope.node(db).as_class().is_some_and(|node| {
                    node.node(prepared.parsed_module()).range() == parent.range()
                })
            })
            .ok_or_else(|| anyhow::anyhow!("fixture parent has no class body scope"))?
    } else {
        ty_python_core::global_scope(db, prepared.program_file())
    };
    let target_range = StaticClassLiteral::header_range_from_node(target);
    Ok((scope, target_range))
}

/// Class default-reference and own-variable-shadowing checks finish during cold canonical scope
/// inference and same-revision retry. Explicit headlines check reference order and enclosing matches
/// independently of the ordinary implementation; ordinary checking afterward also verifies complete
/// diagnostic values. Later class checks are not yet supported by this runtime, so the scope
/// remains unpublished even after both scans finish.
#[test_case::test_case(
    "class Target[T, U = T]: ...\n", &["Target"], &[];
    "earlier default reference"
)]
#[test_case::test_case(
    "class Target[U = tuple[T, V], T = int, V = str]: ...\n", &["Target"],
    &["Default of `U` cannot reference later type parameter `T`"];
    "first invalid nested reference"
)]
#[test_case::test_case(
    "class Outer[T]:\n    class Target[U = T]: ...\n", &["Outer", "Target"],
    &["Default of `U` cannot reference out-of-scope type variable `T`"];
    "out of scope default reference"
)]
#[test_case::test_case(
    "class Outer[T]:\n    class Target[U]: ...\n", &["Outer", "Target"], &[];
    "distinct enclosing name"
)]
#[test_case::test_case(
    "class Outer[T]:\n    class Target[T]: ...\n", &["Outer", "Target"],
    &["Generic class `Target` uses type variable `T` already bound by an enclosing scope"];
    "own class name shadows"
)]
#[test_case::test_case(
    "class Outer[T]:\n    class Middle[T]:\n        class Target[T]: ...\n",
    &["Outer", "Middle", "Target"],
    &[
        "Generic class `Target` uses type variable `T` already bound by an enclosing scope",
        "Generic class `Target` uses type variable `T` already bound by an enclosing scope",
    ];
    "all enclosing matches"
)]
fn cold_class_generic_scans_preserve_reports(
    source: &str,
    path: &[&str],
    expected_headlines: &[&str],
) -> anyhow::Result<()> {
    check_class_reports(source, path, &[], expected_headlines, 0)
}

/// Disabling `shadowed-type-variable` or suppressing class-generic reports leaves both scans
/// running, and matching suppression targets are marked used. Cold attempts retain the same
/// diagnostic values as their same-revision retries and ordinary checking performed afterward.
#[test_case::test_case(
    "class Outer[T]:\n    class Target[T]: ...\n", &["Outer", "Target"],
    &["shadowed-type-variable"], 0;
    "shadow lint disabled"
)]
#[test_case::test_case(
    "class Target[U = T, T = int]: ...  # ty: ignore[invalid-generic-class]\n",
    &["Target"], &[], 1;
    "default inline suppression"
)]
#[test_case::test_case(
    "class Outer[T]:\n    class Target[T]: ...  # ty: ignore[shadowed-type-variable]\n",
    &["Outer", "Target"], &[], 1;
    "shadow inline suppression"
)]
#[test_case::test_case(
    "# ty: ignore[invalid-generic-class]\n\nclass Target[U = T, T = int]: ...\n",
    &["Target"], &[], 1;
    "default file suppression"
)]
fn class_generic_report_policy(
    source: &str,
    path: &[&str],
    disabled: &[&str],
    expected_used: usize,
) -> anyhow::Result<()> {
    check_class_reports(source, path, disabled, &[], expected_used)
}

/// Compares the completed scans and retained reports across two cold/retry attempts, then checks
/// full diagnostic values against ordinary checking without warming the controlled attempts.
fn check_class_reports(
    source: &str,
    path: &[&str],
    disabled: &[&str],
    expected_headlines: &[&str],
    expected_used: usize,
) -> anyhow::Result<()> {
    let registry = crate::default_lint_registry();
    let mut rules = RuleSelection::from_registry(registry);
    for code in disabled {
        rules.disable(registry.get(code)?);
    }
    let db = TestDbBuilder::new()
        .with_rule_selection(rules)
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", source)
        .build()?;
    let prepared = prepare(&db);
    let (scope, target_range) = fixture_target(&db, &prepared, path)?;
    let selected = |diagnostic: &Diagnostic| {
        [
            DiagnosticId::lint("invalid-generic-class"),
            DiagnosticId::lint("shadowed-type-variable"),
        ]
        .contains(&diagnostic.id())
            && diagnostic.primary_span().and_then(|span| span.range()) == Some(target_range)
    };
    let scope_key = InferScope::Bare(scope).as_id();
    let revision = salsa::plumbing::current_revision(&db);
    let mut attempts = Vec::new();

    // The second attempt uses completed canonical children from the same unchanged revision.
    for _ in 0..2 {
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
                .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        observations::reset(None);
        let recording = Recording::start(prepared.program_file().file(&db));
        let result = controlled_scope(&prepared, scope, &funded());
        let events = recording.take_events();
        drop(recording);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(())
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
                .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(_),
                    ..
                })
            ),
            "{result:?}\n{events:?}"
        );
        let completed = events
            .iter()
            .filter_map(|event| match event {
                Event::Completed(class, stage, _, _) => Some((*class, *stage)),
                Event::Boundary(_, _, _) | Event::Inserted(_, _, _) => None,
            })
            .collect::<Vec<_>>();
        let [
            (class, ValidationStage::DefaultReferences),
            (same_class, ValidationStage::OwnShadowing),
        ] = completed.as_slice()
        else {
            anyhow::bail!("both class-generic checks did not complete exactly once: {events:?}");
        };
        assert_eq!(class, same_class);
        let Some(Event::Completed(_, ValidationStage::OwnShadowing, diagnostics, used)) =
            events.last()
        else {
            anyhow::bail!("last completed check is not own-variable shadowing: {events:?}");
        };
        assert_eq!(*used, expected_used);
        let diagnostics = diagnostics
            .iter()
            .filter(|d| selected(d))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            diagnostics
                .iter()
                .map(Diagnostic::headline_message)
                .collect::<Vec<_>>(),
            expected_headlines,
            "{events:?}"
        );
        let inserted = events
            .iter()
            .filter_map(|event| match event {
                Event::Inserted(owner, stage, diagnostics) => Some((*owner, *stage, diagnostics)),
                Event::Boundary(_, _, _) | Event::Completed(_, _, _, _) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(inserted.len(), expected_headlines.len(), "{events:?}");
        for (owner, stage, snapshot) in &inserted {
            assert_eq!(owner, class);
            assert!(!snapshot.is_empty());
            let boundaries = events
                .iter()
                .filter_map(|event| match event {
                    Event::Boundary(actual, actual_stage, boundary)
                        if actual == owner && actual_stage == stage =>
                    {
                        Some(*boundary)
                    }
                    Event::Boundary(_, _, _)
                    | Event::Inserted(_, _, _)
                    | Event::Completed(_, _, _, _) => None,
                })
                .collect::<Vec<_>>();
            let report_count = inserted
                .iter()
                .filter(|(other, other_stage, _)| other == owner && other_stage == stage)
                .count();
            assert_eq!(boundaries.len(), 3 * report_count);
            assert!(boundaries.chunks_exact(3).all(|chunk| chunk
                == [
                    ReportBoundary::Eligibility,
                    ReportBoundary::Construction,
                    ReportBoundary::Insertion,
                ]));
        }
        attempts.push(diagnostics);
    }
    assert_eq!(attempts[0], attempts[1]);
    let ordinary = crate::check_file_unwrap(&db, prepared.program_file())
        .into_iter()
        .filter(selected)
        .collect::<Vec<_>>();
    assert_eq!(attempts[0], ordinary);
    Ok(())
}
