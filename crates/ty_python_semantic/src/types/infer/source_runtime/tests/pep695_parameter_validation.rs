//! Observes PEP 695 parameter validation during cold canonical module inference.
//!
//! A later class check can refuse after both parameter checks finish. These controls distinguish
//! the completed checks and their privately retained diagnostics from publication of the enclosing
//! module scope. The examples also appear in the ordinary PEP 695 mdtests.

use std::cell::RefCell;

use ruff_db::diagnostic::{Diagnostic, DiagnosticId};
use ruff_db::files::File;
use ruff_text_size::{Ranged, TextRange};

use super::*;
use crate::lint::RuleSelection;
use crate::types::diagnostic::TypeCheckDiagnostics;

/// Identifies which of the two ordered parameter-list checks reached an observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ValidationStage {
    SinglePack,
    DefaultsAfterPack,
}

/// Identifies a diagnostic operation before it starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ReportBoundary {
    Eligibility,
    Construction,
    Insertion,
}

/// Records completed checks separately from entry into a possibly interrupted report operation.
#[derive(Debug)]
enum Event {
    Boundary(ValidationStage, ReportBoundary),
    Inserted(ValidationStage, Vec<Diagnostic>),
    Completed(ValidationStage, CompletedReports),
}

/// Retains completed diagnostics and the number of suppression targets marked used.
#[derive(Debug)]
struct CompletedReports {
    diagnostics: Vec<Diagnostic>,
    used_suppressions: usize,
}

/// Retains observations only for the class selected by the current control.
#[derive(Debug)]
struct Journal {
    file: File,
    class_range: TextRange,
    events: Vec<Event>,
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

/// Records a successful validation return and the diagnostics already inserted into its builder.
pub(in crate::types::infer) fn validation_completed(
    _db: &dyn Db,
    file: File,
    class_range: TextRange,
    stage: ValidationStage,
    diagnostics: &TypeCheckDiagnostics,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.file == file
            && journal.class_range == class_range
        {
            journal.events.push(Event::Completed(
                stage,
                CompletedReports {
                    diagnostics: diagnostics.into_iter().cloned().collect(),
                    used_suppressions: diagnostics.used_len(),
                },
            ));
        }
    });
}

/// Records entry before a real reporting operation without replacing or completing that operation.
pub(in crate::types::infer) fn report_boundary(
    _db: &dyn Db,
    file: File,
    class_range: TextRange,
    stage: ValidationStage,
    boundary: ReportBoundary,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.file == file
            && journal.class_range == class_range
        {
            journal.events.push(Event::Boundary(stage, boundary));
        }
    });
}

/// Captures diagnostics after insertion, so the recorder's shared references cannot force
/// copy-on-write cloning during diagnostic construction.
pub(in crate::types::infer) fn diagnostic_inserted(
    _db: &dyn Db,
    file: File,
    class_range: TextRange,
    stage: ValidationStage,
    diagnostics: &TypeCheckDiagnostics,
) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal
            && journal.file == file
            && journal.class_range == class_range
        {
            journal.events.push(Event::Inserted(
                stage,
                diagnostics.into_iter().cloned().collect(),
            ));
        }
    });
}

/// Removes the thread-local recorder even if the request or an assertion unwinds.
#[derive(Debug)]
struct Recording;

impl Recording {
    fn start(file: File, class_range: TextRange) -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(journal.is_none());
            *journal = Some(Journal {
                file,
                class_range,
                events: Vec::new(),
            });
        });
        Self
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

/// Selects only the two parameter-list diagnostics when comparing against ordinary file checking.
fn parameter_diagnostic(diagnostic: &Diagnostic) -> bool {
    [
        DiagnosticId::lint("invalid-type-form"),
        DiagnosticId::lint("invalid-type-variable-default"),
    ]
    .contains(&diagnostic.id())
}

/// Cold canonical module inference completes both parameter scans and their reports before the
/// unsupported nominal-variance check refuses. Inference runs twice in the same revision; each
/// attempt leaves the module scope unpublished and cleans up its builders. Ordinary checking runs
/// only afterward and verifies the full diagnostic values, including annotations and concise messages.
#[test_case::test_case("class Ok3[*Ts]: ...\n", &[]; "valid single pack")]
#[test_case::test_case("class Ok1[T, *Ts]: ...\n", &[]; "valid trailing pack")]
#[test_case::test_case("class Array[*Ts1, *Ts2]: ...\n", &["invalid-type-form"]; "duplicate packs")]
#[test_case::test_case("class Foo[*Ts, T = int]: ...\n", &["invalid-type-variable-default"]; "single default")]
#[test_case::test_case("class Baz[*Ts, T1 = int, T2 = str]: ...\n", &["invalid-type-variable-default"]; "multiple defaults")]
#[test_case::test_case("class Qux[*Ts, **P = [int, str]]: ...\n", &["invalid-type-variable-default"]; "paramspec default")]
#[test_case::test_case("class Grault[*Us, *Ts = *tuple[int, str]]: ...\n", &["invalid-type-form", "invalid-type-variable-default"]; "both validations report")]
fn cold_parameter_checks_finish_before_later_class_refusal(
    source: &str,
    expected_codes: &'static [&'static str],
) -> anyhow::Result<()> {
    check_parameter_reports(source, &[], expected_codes, 0)
}

/// Each lint can be disabled or suppressed independently without skipping the second scan.
/// The cold attempt and same-revision retry retain exactly the expected diagnostics and mark
/// matching suppression targets used in the unpublished module builder.
#[test_case::test_case(
    "class Grault[*Us, *Ts = *tuple[int, str]]: ...\n",
    &["invalid-type-form"], &["invalid-type-variable-default"], 0;
    "duplicate pack disabled"
)]
#[test_case::test_case(
    "class Grault[*Us, *Ts = *tuple[int, str]]: ...\n",
    &["invalid-type-variable-default"], &["invalid-type-form"], 0;
    "later default disabled"
)]
#[test_case::test_case(
    "class Grault[*Us, *Ts = *tuple[int, str]]: ...\n",
    &["invalid-type-form", "invalid-type-variable-default"], &[], 0;
    "both disabled"
)]
#[test_case::test_case(
    "class Grault[*Us, *Ts = *tuple[int, str]]: ...  # ty: ignore[invalid-type-form]\n",
    &[], &["invalid-type-variable-default"], 1;
    "inline duplicate pack suppression"
)]
#[test_case::test_case(
    "class Grault[*Us, *Ts = *tuple[int, str]]: ...  # ty: ignore[invalid-type-variable-default]\n",
    &[], &["invalid-type-form"], 1;
    "inline later default suppression"
)]
#[test_case::test_case(
    "# ty: ignore[invalid-type-form, invalid-type-variable-default]\n\nclass Grault[*Us, *Ts = *tuple[int, str]]: ...\n",
    &[], &[], 2;
    "file suppression"
)]
fn cold_parameter_reporting_respects_rules_and_suppressions(
    source: &str,
    disabled: &[&str],
    expected_codes: &[&'static str],
    expected_used: usize,
) -> anyhow::Result<()> {
    check_parameter_reports(source, disabled, expected_codes, expected_used)
}

/// Checks both completed parameter scans across cold inference and same-revision retry, then
/// compares their retained diagnostic values with ordinary checking performed afterward.
fn check_parameter_reports(
    source: &str,
    disabled: &[&str],
    expected_codes: &[&'static str],
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
    let [Stmt::ClassDef(class)] = prepared.parsed_module().syntax().body.as_slice() else {
        anyhow::bail!("parameter fixture must contain exactly one class");
    };
    let scope = ty_python_core::global_scope(&db, prepared.program_file());
    let scope_key = InferScope::Bare(scope).as_id();
    let revision = salsa::plumbing::current_revision(&db);
    let mut completed_reports = None;

    // These attempts deliberately share one database so the second exercises same-revision retry.
    for _ in 0..2 {
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
                .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        observations::reset(None);
        let recording = Recording::start(prepared.program_file().file(&db), class.range());
        let result = controlled_module(&db, &prepared, &funded());
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
        // The unsupported nominal-variance check also uses the `ProtocolVariance` operation label.
        //
        assert_eq!(
            result,
            Ok(unavailable(OperationId::ClassCheck(
                ClassCheckOperation::ProtocolVariance
            ))),
            "{events:?}"
        );
        let stages = events
            .iter()
            .filter_map(|event| match event {
                Event::Completed(stage, _) => Some(*stage),
                Event::Boundary(_, _) | Event::Inserted(_, _) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            stages,
            [
                ValidationStage::SinglePack,
                ValidationStage::DefaultsAfterPack
            ],
            "{events:?}"
        );
        let Some(Event::Completed(ValidationStage::DefaultsAfterPack, reports)) = events.last()
        else {
            anyhow::bail!("defaults check did not complete: {events:?}");
        };
        assert_eq!(reports.used_suppressions, expected_used, "{events:?}");
        let reports = reports
            .diagnostics
            .iter()
            .filter(|diagnostic| parameter_diagnostic(diagnostic))
            .cloned()
            .collect::<Vec<_>>();
        let codes = reports.iter().map(Diagnostic::id).collect::<Vec<_>>();
        let expected = expected_codes
            .iter()
            .map(|code| DiagnosticId::lint(code))
            .collect::<Vec<_>>();
        assert_eq!(codes, expected, "{events:?}");
        let inserted = events
            .iter()
            .filter_map(|event| match event {
                Event::Inserted(stage, diagnostics) => Some((*stage, diagnostics)),
                Event::Boundary(_, _) | Event::Completed(_, _) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(inserted.len(), expected_codes.len(), "{events:?}");
        for (stage, diagnostics) in inserted {
            assert!(!diagnostics.is_empty());
            let boundaries = events
                .iter()
                .filter_map(|event| match event {
                    Event::Boundary(actual, boundary) if *actual == stage => Some(*boundary),
                    Event::Boundary(_, _) | Event::Inserted(_, _) | Event::Completed(_, _) => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                boundaries,
                [
                    ReportBoundary::Eligibility,
                    ReportBoundary::Construction,
                    ReportBoundary::Insertion
                ]
            );
        }
        if let Some(previous) = &completed_reports {
            assert_eq!(&reports, previous);
        }
        completed_reports = Some(reports);
    }

    let ordinary = crate::check_file_unwrap(&db, prepared.program_file())
        .into_iter()
        .filter(parameter_diagnostic)
        .collect::<Vec<_>>();
    assert_eq!(completed_reports, Some(ordinary));
    Ok(())
}
