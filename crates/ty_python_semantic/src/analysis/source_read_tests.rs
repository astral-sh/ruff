use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use ruff_db::diagnostic::Severity;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{SourceText, source_text};
use ruff_db::system::DbWithWritableSystem;
use salsa::Database;
use salsa::attempt_probe::{AttemptOutcome, Incomplete, StartError};
use salsa::execution_probe::{
    ExecutionLimits, ExecutionReceipt, ExecutionWork, RegistryBuilder, RunResult, TaskEndpoint,
    try_with_metered_execution_budget,
};
use salsa::prepared_source_probe::{Stamp, Status, assert_no_active_attempt, capture};
use ty_module_resolver::ModuleName;
use ty_python_core::semantic_index;

use super::{
    AnalysisFailure, AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, AnalysisSession,
    OperationId, PreparedAnalysisFile, check_file_with_policy, prepare_file, with_analysis_session,
};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::default_lint_registry;
use crate::lint::{LintId, LintSource, RuleSelection};
use crate::suppression::{
    Suppressions, UNUSED_IGNORE_COMMENT, UNUSED_TYPE_IGNORE_COMMENT, suppressions,
};

const SOURCE: &str = "# ty: ignore\nvalue = True\n";
const LIMITS: ExecutionLimits = ExecutionLimits {
    semantic_work: 1_000_000,
    requested_bytes: 1_000_000,
};

fn fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", SOURCE).unwrap();
    db
}

fn read<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    limits: ExecutionLimits,
) -> ExecutionReceipt<RunResult<(SourceText, &'db Suppressions)>> {
    try_with_metered_execution_budget(prepared.root.db, limits, |budget| {
        RegistryBuilder::with_budget(prepared.root.db, &budget)?
            .seal()?
            .run(|endpoint| async move {
                let source = prepared.root.read_source_text(&endpoint).await;
                let suppressions = prepared.root.read_suppressions(&endpoint).await;
                Ok((source, suppressions))
            })
    })
    .unwrap()
}

#[test]
fn root_reads_preserve_canonical_sources_after_prepared_handle_drops() {
    let db = fixture();
    let file = system_path_to_file(&db, "src/main.py").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let source_memo = source_text::prepare_memo(&db, file).unwrap();
    let suppressions_memo =
        suppressions::prepare_memo(&db, prepared.program_file().python_file(&db)).unwrap();
    let expected_source = source_memo.value().unwrap();
    let expected_suppressions = suppressions_memo.value().unwrap();
    let captured = capture(&db, || read(&prepared, LIMITS)).unwrap();
    let AttemptOutcome::Complete(Ok((source, suppressions))) = captured.value.outcome else {
        panic!("prepared source reads did not complete");
    };
    assert_eq!(
        captured
            .reads
            .iter()
            .map(|read| read.key)
            .collect::<Vec<_>>(),
        [source_memo.database_key(), suppressions_memo.database_key()]
    );
    assert!(captured.reads.iter().all(|read| {
        read.parent.is_none() && read.status == Status::Final && read.stamp == captured.stamp
    }));
    drop(prepared);
    assert_eq!(source.as_str(), SOURCE);
    assert!(std::ptr::eq(source.as_str(), expected_source.as_str()));
    assert!(std::ptr::eq(suppressions, expected_suppressions));
    assert_no_active_attempt();
}

#[test]
fn index_only_reads_deliver_the_canonical_index_without_reading_the_parser() {
    let db = fixture();
    let file = system_path_to_file(&db, "src/main.py").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let memo = semantic_index::prepare_memo(&db, prepared.program_file()).unwrap();
    let captured = capture(&db, || {
        try_with_metered_execution_budget(&db, LIMITS, |budget| {
            RegistryBuilder::with_budget(&db, &budget)?
                .seal()?
                .run(|endpoint| {
                    let prepared = &prepared;
                    async move { Ok(prepared.root.read_semantic_index(&endpoint).await) }
                })
        })
        .unwrap()
    })
    .unwrap();
    let AttemptOutcome::Complete(Ok(index)) = captured.value.outcome else {
        panic!("prepared index read did not complete");
    };
    assert_eq!(captured.reads.len(), 1);
    let read = &captured.reads[0];
    assert_eq!(read.key, memo.database_key());
    assert_eq!(read.parent, None);
    assert_eq!(read.status, Status::Final);
    assert_eq!(read.stamp, captured.stamp);
    drop(prepared);
    assert!(std::ptr::eq(index, memo.value().unwrap()));
    assert_no_active_attempt();
}

#[test]
fn refused_source_reads_retry_in_the_same_revision() {
    let db = fixture();
    let file = system_path_to_file(&db, "src/main.py").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let expected_suppressions =
        suppressions::prepare_memo(&db, prepared.program_file().python_file(&db))
            .unwrap()
            .value()
            .unwrap();
    let stamp = Stamp::current(&db);
    let complete = read(&prepared, LIMITS);
    assert!(matches!(complete.outcome, AttemptOutcome::Complete(Ok(_))));
    for (limits, reason) in [
        (
            ExecutionLimits {
                semantic_work: complete.usage.semantic_work.checked_sub(1).unwrap(),
                ..LIMITS
            },
            Incomplete::Allowance,
        ),
        (
            ExecutionLimits {
                requested_bytes: complete.usage.requested_bytes.checked_sub(1).unwrap(),
                ..LIMITS
            },
            Incomplete::RequestedAllocation,
        ),
    ] {
        assert!(matches!(
            read(&prepared, limits).outcome,
            AttemptOutcome::Incomplete(actual) if actual == reason
        ));
        assert_no_active_attempt();
        assert_eq!(Stamp::current(&db), stamp);
        prepared.root.check_current().unwrap();
        let AttemptOutcome::Complete(Ok((source, suppressions))) = read(&prepared, LIMITS).outcome
        else {
            panic!("prepared source retry did not complete");
        };
        assert_eq!(source.as_str(), SOURCE);
        assert!(std::ptr::eq(suppressions, expected_suppressions));
        assert_no_active_attempt();
        assert_eq!(Stamp::current(&db), stamp);
    }
}

#[test]
fn source_read_cancellation_preserves_native_payload_and_drains_the_run() {
    let db = fixture();
    let file = system_path_to_file(&db, "src/main.py").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let prepared = &prepared;
    let db = &db;
    let stamp = Stamp::current(db);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        try_with_metered_execution_budget(db, LIMITS, |budget| {
            RegistryBuilder::with_budget(db, &budget)?
                .seal()?
                .run(|endpoint| async move {
                    let source = prepared.root.read_source_text(&endpoint).await;
                    endpoint
                        .local_call(|| {
                            endpoint.admit_work(1)?;
                            db.cancellation_token().cancel();
                            Ok(())
                        })
                        .await;
                    let suppressions = prepared.root.read_suppressions(&endpoint).await;
                    Ok((source, suppressions))
                })
        })
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)));
    assert_no_active_attempt();
    assert_eq!(Stamp::current(db), stamp);
    prepared.root.check_current().unwrap();
    assert!(matches!(
        read(prepared, LIMITS).outcome,
        AttemptOutcome::Complete(Ok(_))
    ));
}

fn catalogue_fixture() -> TestDb {
    TestDbBuilder::new()
        .with_file("src/main.py", SOURCE)
        .with_file("src/dependency.py", "value = False\n")
        .build()
        .unwrap()
}

fn catalogue_policy() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: LIMITS.semantic_work,
        requested_bytes_limit: LIMITS.requested_bytes,
    }
}

fn read_catalogue_dependency<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    dependency: File,
    attempts: &Cell<usize>,
    after_reads: impl Fn(&AnalysisSession<'_, 'db>, &TaskEndpoint<'_, 'db>) -> RunResult<()>,
) -> Result<AnalysisOutcome<SourceText>, AnalysisFailure> {
    let program = prepared.program_file().program(prepared.root.db);
    let present = ModuleName::new_static("dependency").unwrap();
    let absent = ModuleName::new_static("absent_catalogue_dependency").unwrap();
    with_analysis_session(prepared, &catalogue_policy(), |session| {
        attempts.set(attempts.get() + 1);
        let present = &present;
        let absent = &absent;
        let after_reads = &after_reads;
        RegistryBuilder::with_budget(session.db(), session.budget())?
            .seal()?
            .run(|endpoint| async move {
                let source = session.source(&endpoint, dependency, program).await?;
                let text = source.read_source_text(&endpoint).await;
                let resolved = session
                    .resolve_module(&endpoint, program, present, None)
                    .await?;
                let unresolved = session
                    .resolve_module(&endpoint, program, absent, None)
                    .await?;
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        assert!(resolved.is_some());
                        assert!(unresolved.is_none());
                        after_reads(session, &endpoint)
                    })
                    .await;
                Ok(text)
            })
    })
}

#[test]
fn prepared_catalogue_retains_sources_and_module_resolutions_across_invocations() {
    let db = catalogue_fixture();
    let root = system_path_to_file(&db, "src/main.py").unwrap();
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let prepared = prepare_file(&db, root).unwrap();
    let stamp = Stamp::current(&db);
    let attempts = Cell::new(0);
    let initial = read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| Ok(()));
    let Ok(AnalysisOutcome::Complete(initial)) = initial else {
        panic!("catalogue preparation did not complete: {initial:?}");
    };
    assert_eq!(attempts.get(), 1);
    assert_eq!(initial.as_str(), "value = False\n");
    let (catalogue, source) = {
        let sources = prepared.sources.borrow();
        let entries = sources.entries.borrow();
        assert_eq!(entries.files.len(), 2);
        assert_eq!(entries.modules.len(), 2);
        (
            Rc::as_ptr(&sources),
            Rc::as_ptr(&entries.files[&dependency]),
        )
    };

    attempts.set(0);
    let retry = capture(&db, || {
        read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| Ok(()))
    })
    .unwrap();
    let Ok(AnalysisOutcome::Complete(retried)) = retry.value else {
        panic!("catalogue retry did not complete: {:?}", retry.value);
    };
    assert_eq!(attempts.get(), 1);
    assert!(std::ptr::eq(initial.as_str(), retried.as_str()));
    assert_eq!(retry.reads.len(), 3);
    assert!(retry.reads.iter().all(|read| {
        read.parent.is_none() && read.status == Status::Final && read.stamp == stamp
    }));
    let sources = prepared.sources.borrow();
    let entries = sources.entries.borrow();
    assert_eq!(Rc::as_ptr(&sources), catalogue);
    assert_eq!(Rc::as_ptr(&entries.files[&dependency]), source);
    assert_eq!(entries.files.len(), 2);
    assert_eq!(entries.modules.len(), 2);
    assert_eq!(Stamp::current(&db), stamp);
    assert_no_active_attempt();
}

#[test]
fn prepared_catalogue_clones_share_new_preparation() {
    let db = catalogue_fixture();
    let root = system_path_to_file(&db, "src/main.py").unwrap();
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let prepared = prepare_file(&db, root).unwrap();
    let cloned = prepared.clone();
    let attempts = Cell::new(0);
    assert!(matches!(
        read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| Ok(())),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(attempts.get(), 1);
    assert!(Rc::ptr_eq(&prepared.root, &cloned.root));
    assert!(Rc::ptr_eq(&prepared.sources, &cloned.sources));
    drop(prepared);

    attempts.set(0);
    assert!(matches!(
        read_catalogue_dependency(&cloned, dependency, &attempts, |_, _| Ok(())),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(attempts.get(), 1);
    assert_no_active_attempt();
}

#[test]
fn prepared_catalogue_rejects_same_handle_and_clone_reentry() {
    let db = catalogue_fixture();
    let root = system_path_to_file(&db, "src/main.py").unwrap();
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let prepared = prepare_file(&db, root).unwrap();
    let cloned = prepared.clone();
    let attempts = Cell::new(0);
    assert!(matches!(
        read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| {
            for handle in [&prepared, &cloned] {
                let nested = with_analysis_session(handle, &catalogue_policy(), |_| {
                    panic!("nested invocation entered semantic execution");
                });
                assert_eq!(
                    nested,
                    Err::<AnalysisOutcome<()>, _>(AnalysisFailure::Start(
                        StartError::NestedAttempt
                    ))
                );
                assert_eq!(
                    check_file_with_policy(handle, &catalogue_policy()),
                    Err(AnalysisFailure::Start(StartError::NestedAttempt))
                );
            }
            Ok(())
        }),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(attempts.get(), 1);
    attempts.set(0);
    assert!(matches!(
        read_catalogue_dependency(&cloned, dependency, &attempts, |_, _| Ok(())),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(attempts.get(), 1);
    assert_no_active_attempt();
}

#[test]
fn prepared_catalogue_retains_completed_preparation_after_refusal() {
    for reason in [
        AnalysisIncomplete::WorkLimit,
        AnalysisIncomplete::RequestedAllocationLimit,
        AnalysisIncomplete::UnavailableOperation(OperationId::Finalization),
    ] {
        let db = catalogue_fixture();
        let root = system_path_to_file(&db, "src/main.py").unwrap();
        let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
        let prepared = prepare_file(&db, root).unwrap();
        let stamp = Stamp::current(&db);
        let attempts = Cell::new(0);
        assert_eq!(
            read_catalogue_dependency(&prepared, dependency, &attempts, |session, endpoint| {
                match reason {
                    AnalysisIncomplete::WorkLimit => endpoint.admit_work(LIMITS.semantic_work + 1),
                    AnalysisIncomplete::RequestedAllocationLimit => {
                        endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: LIMITS.requested_bytes + 1,
                        })
                    }
                    AnalysisIncomplete::UnavailableOperation(operation) => {
                        session.unavailable(endpoint, operation)
                    }
                }
            }),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: (),
            })
        );
        assert_eq!(attempts.get(), 1);
        assert_no_active_attempt();
        attempts.set(0);
        assert!(matches!(
            read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| Ok(())),
            Ok(AnalysisOutcome::Complete(_))
        ));
        assert_eq!(attempts.get(), 1);
        assert_eq!(Stamp::current(&db), stamp);
        assert_no_active_attempt();
    }
}

#[test]
fn prepared_catalogue_survives_panic_and_retries_in_the_same_revision() {
    let db = catalogue_fixture();
    let root = system_path_to_file(&db, "src/main.py").unwrap();
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let prepared = prepare_file(&db, root).unwrap();
    let stamp = Stamp::current(&db);
    let attempts = Cell::new(0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| {
            panic!("prepared catalogue panic");
        })
    }));
    let payload = result.unwrap_err();
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"prepared catalogue panic")
    );
    assert_eq!(attempts.get(), 1);
    assert_no_active_attempt();
    attempts.set(0);
    assert!(matches!(
        read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| Ok(())),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(attempts.get(), 1);
    assert_eq!(Stamp::current(&db), stamp);
    assert_no_active_attempt();
}

#[test]
fn prepared_catalogue_preserves_native_cancellation_and_retries_in_the_same_revision() {
    let db = catalogue_fixture();
    let root = system_path_to_file(&db, "src/main.py").unwrap();
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let prepared = prepare_file(&db, root).unwrap();
    let stamp = Stamp::current(&db);
    let attempts = Cell::new(0);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        read_catalogue_dependency(&prepared, dependency, &attempts, |session, _| {
            session.db().cancellation_token().cancel();
            Ok(())
        })
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(attempts.get(), 1);
    assert_no_active_attempt();
    attempts.set(0);
    assert!(matches!(
        read_catalogue_dependency(&prepared, dependency, &attempts, |_, _| Ok(())),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(attempts.get(), 1);
    assert_eq!(Stamp::current(&db), stamp);
    assert_no_active_attempt();
}

#[test]
fn prepared_catalogue_releases_sources_after_the_last_handle_drops() {
    let db = catalogue_fixture();
    let root = system_path_to_file(&db, "src/main.py").unwrap();
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let prepared = prepare_file(&db, root).unwrap();
    let cloned = prepared.clone();
    assert!(matches!(
        read_catalogue_dependency(&prepared, dependency, &Cell::new(0), |_, _| Ok(())),
        Ok(AnalysisOutcome::Complete(_))
    ));
    let weak_root = Rc::downgrade(&prepared.root);
    let weak_shared = Rc::downgrade(&prepared.sources);
    let (weak_catalogue, weak_dependency) = {
        let sources = prepared.sources.borrow();
        let entries = sources.entries.borrow();
        (
            Rc::downgrade(&sources),
            Rc::downgrade(&entries.files[&dependency]),
        )
    };
    drop(prepared);
    assert!(weak_root.upgrade().is_some());
    assert!(weak_shared.upgrade().is_some());
    assert!(weak_catalogue.upgrade().is_some());
    assert!(weak_dependency.upgrade().is_some());
    drop(cloned);
    assert!(weak_root.upgrade().is_none());
    assert!(weak_shared.upgrade().is_none());
    assert!(weak_catalogue.upgrade().is_none());
    assert!(weak_dependency.upgrade().is_none());
    assert_no_active_attempt();
}

#[test]
fn file_checks_preserve_the_unused_suppression_source_read_short_circuit() {
    for enabled in [false, true] {
        let mut rules = RuleSelection::from_registry(default_lint_registry());
        for lint in [&UNUSED_IGNORE_COMMENT, &UNUSED_TYPE_IGNORE_COMMENT] {
            if enabled {
                rules.enable(LintId::of(lint), Severity::Warning, LintSource::Default);
            } else {
                rules.disable(LintId::of(lint));
            }
        }
        let db = TestDbBuilder::new()
            .with_rule_selection(rules)
            .with_file(
                "src/main.py",
                "def choose(value):\n    return value\nchoose(True)\n",
            )
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "src/main.py").unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        let program_file = prepared.program_file();
        let source_key = source_text::prepare_memo(&db, file).unwrap().database_key();
        let suppressions_key = suppressions::prepare_memo(&db, program_file.python_file(&db))
            .unwrap()
            .database_key();
        let parsed_key = parsed_module::prepare_memo(&db, program_file.python_file(&db))
            .unwrap()
            .database_key();
        let index_key = semantic_index::prepare_memo(&db, program_file)
            .unwrap()
            .database_key();
        let controlled = capture(&db, || {
            check_file_with_policy(
                &prepared,
                &AnalysisPolicy {
                    semantic_work_limit: 1_000_000,
                    requested_bytes_limit: 16 * 1024 * 1024,
                },
            )
        })
        .unwrap();
        assert!(
            matches!(&controlled.value, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
            "{:?}",
            controlled.value
        );
        let ordinary = capture(&db, || crate::check_file(&db, program_file)).unwrap();
        assert!(ordinary.value.unwrap().is_empty());
        let root_sources = |reads: &[salsa::prepared_source_probe::Read]| {
            reads
                .iter()
                .filter(|read| {
                    read.parent.is_none() && [source_key, suppressions_key].contains(&read.key)
                })
                .map(|read| read.key)
                .collect::<Vec<_>>()
        };
        let expected = if enabled {
            vec![source_key, suppressions_key, source_key]
        } else {
            vec![source_key, suppressions_key]
        };
        assert_eq!(root_sources(&controlled.reads), expected);
        assert_eq!(root_sources(&ordinary.reads), expected);
        let root_structure = |reads: &[salsa::prepared_source_probe::Read]| {
            reads
                .iter()
                .filter(|read| {
                    read.parent.is_none() && [parsed_key, index_key].contains(&read.key)
                })
                .map(|read| read.key)
                .collect::<Vec<_>>()
        };
        assert_eq!(root_structure(&controlled.reads), root_structure(&ordinary.reads));
        assert_no_active_attempt();
    }
}
