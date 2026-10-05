use std::future::{Future, ready};
use std::rc::Rc;

use ruff_db::files::File;
use salsa::execution_probe::{
    BorrowOrCopy, Demand, ExecutionWork, FieldReadProfile, FieldReturnMode, NativeValueQuote,
    PreparedSourceMemo, RunError, RunResult, TaskEndpoint,
};
use salsa::prepared_source_probe::{PreparationError, Stamp};
use ty_python_semantic::AnalysisSettings;
use ty_python_semantic::lint::RuleSelection;
use ty_python_semantic::prepared_host::{
    PreparedHostFileReads, admit_host_task_setup, boxed_future_with_fixed_transfers_at,
    generated_field_quote, local_with_fixed_transfers_at,
};

use super::{FileSettings, Settings, file_settings};
use crate::{Db, Project};

pub(crate) fn prepare<'db>(
    db: &'db dyn Db,
    file: File,
    project: Option<Project>,
    check_path: bool,
    verbose_source: VerboseSource,
) -> Result<Rc<dyn PreparedHostFileReads<'db> + 'db>, PreparationError> {
    let settings =
        file_settings::prepare_memo(db, file).map_err(|_| PreparationError::InvalidDependency)?;
    let eligibility = if !check_path || (project.is_some() && !file.path(db).is_vendored_path()) {
        Some(
            crate::should_check_file::prepare_memo(db, file)
                .map_err(|_| PreparationError::InvalidDependency)?,
        )
    } else {
        None
    };
    let reads = ProjectHostReads {
        db,
        file,
        project,
        check_path,
        verbose_source,
        stamp: Stamp::current(db),
        settings,
        eligibility,
    };
    reads.check_current()?;
    Ok(Rc::new(reads))
}

/// Selects the ordinary database implementation's source for verbose diagnostic settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VerboseSource {
    Project,
    #[cfg(any(test, feature = "testing"))]
    Disabled,
}

struct ProjectHostReads<'db> {
    db: &'db dyn Db,
    file: File,
    project: Option<Project>,
    check_path: bool,
    verbose_source: VerboseSource,
    stamp: Stamp,
    settings: PreparedSourceMemo<'db, FileSettings>,
    eligibility: Option<PreparedSourceMemo<'db, bool>>,
}

impl ProjectHostReads<'_> {
    /// Admits endpoint cloning and task-factory transfers before constructing either value.
    fn admit_task_setup(endpoint: &TaskEndpoint<'_, '_>) -> RunResult<()> {
        // Scalar setup and reference-count operations have fixed work regardless of pointer width.
        endpoint.admit_work(24)?;
        // The child endpoint exists before its capture by the factory. Eight capture-sized
        // transfers bound the factory's construction and forwarding into the task; demand
        // separately admits the stored TypedTask, including its future, plus reply
        // allocations and queue storage.
        let requested_bytes = size_of::<(Rc<Self>, TaskEndpoint<'_, '_>)>()
            .checked_mul(8)
            .and_then(|bytes| bytes.checked_add(size_of::<TaskEndpoint<'_, '_>>()))
            .ok_or(RunError::Contract("prepared host task setup bytes overflow"))?;
        endpoint.admit(ExecutionWork::Resource { requested_bytes })
    }
}

impl<'db> PreparedHostFileReads<'db> for ProjectHostReads<'db> {
    fn check_current(&self) -> Result<(), PreparationError> {
        self.settings
            .check_current()
            .map_err(|_| PreparationError::InvalidDependency)?;
        if let Some(eligibility) = &self.eligibility {
            eligibility
                .check_current()
                .map_err(|_| PreparationError::InvalidDependency)?;
        }
        if !self.stamp.belongs_to(self.db) {
            return Err(PreparationError::ChangedDatabaseStamp);
        }
        Ok(())
    }

    fn should_check_file<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<bool>>
    where
        'db: 'run,
    {
        Self::admit_task_setup(&endpoint)?;
        let child = endpoint.clone();
        endpoint.demand(move || async move {
            let check_path = child
                .local_call(|| {
                    child.admit_work(4)?;
                    if !self.stamp.belongs_to(self.db) {
                        return Err(RunError::Contract("prepared host database stamp changed"));
                    }
                    Ok(self.check_path)
                })
                .await;
            if check_path {
                let path = child
                    .read_field(self.file.read_fields(self.db).path(), &BorrowOrCopy)
                    .await;
                let excluded = child
                    .local_call(|| {
                        child.admit_work(4)?;
                        Ok(path.is_vendored_path() || self.project.is_none())
                    })
                    .await;
                if excluded {
                    return Ok(false);
                }
            }
            let eligibility = child
                .local_call(|| {
                    child.admit_work(2)?;
                    self.eligibility
                        .as_ref()
                        .ok_or(RunError::Contract("prepared eligibility memo is missing"))
                })
                .await;
            let value = child
                .read_prepared_source(crate::should_check_file::prepared_read(eligibility))
                .await;
            Ok(child
                .local_call(|| {
                    child.admit_work(1)?;
                    Ok(*value)
                })
                .await)
        })
    }

    fn rule_selection<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<&'db RuleSelection>>
    where
        'db: 'run,
    {
        Self::admit_task_setup(&endpoint)?;
        let child = endpoint.clone();
        endpoint.demand(move || async move {
            let settings = child
                .read_prepared_source(file_settings::prepared_read(&self.settings))
                .await;
            let source = child
                .local_call(|| {
                    child.admit_work(6)?;
                    match settings {
                        FileSettings::Global => {
                            self.project
                                .map(RuleSource::Global)
                                .ok_or(RunError::Contract(
                                    "prepared global settings have no project",
                                ))
                        }
                        FileSettings::File(settings) => Ok(RuleSource::File(&settings.rules)),
                    }
                })
                .await;
            match source {
                RuleSource::Global(project) => {
                    let settings = child
                        .read_field(
                            project.read_fields(self.db).settings(),
                            &ProjectSettingsBorrow,
                        )
                        .await;
                    Ok(child
                        .local_call(|| {
                            child.admit_work(3)?;
                            Ok(settings.rules())
                        })
                        .await)
                }
                RuleSource::File(rules) => Ok(rules),
            }
        })
    }

    fn verbose<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<bool>>
    where
        'db: 'run,
    {
        let make = |host: Rc<Self>, child: TaskEndpoint<'run, 'db>| move || async move {
            let source = local_with_fixed_transfers_at(&child, 3, 0, || host.verbose_source)
                .await?;
            match source {
                VerboseSource::Project => {
                    let project = local_with_fixed_transfers_at(&child, 6, 0, || {
                        host.project.ok_or(RunError::Contract(
                            "prepared verbose settings have no project",
                        ))
                    })
                    .await??;
                    let quote = generated_field_quote(
                        |project: Project, context| project.read_fields(context),
                        |project: Project, context| project.read_fields(context).verbose_flag(),
                    );
                    let read = boxed_future_with_fixed_transfers_at(&child, quote, || {
                        child.read_field(
                            project.read_fields(child.field_request_context()).verbose_flag(),
                            &BorrowOrCopy,
                        )
                    })
                    .await?;
                    Ok(read.await)
                }
                #[cfg(any(test, feature = "testing"))]
                VerboseSource::Disabled => {
                    local_with_fixed_transfers_at(&child, 1, 0, || false).await
                }
            }
        };
        admit_host_task_setup(&endpoint, &make)?;
        let child = endpoint.clone();
        endpoint.demand(make(self, child))
    }

    fn analysis_settings<'run>(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
    ) -> RunResult<Demand<&'db AnalysisSettings>>
    where
        'db: 'run,
    {
        Self::admit_task_setup(&endpoint)?;
        let child = endpoint.clone();
        endpoint.demand(move || async move {
            let settings = child
                .read_prepared_source(file_settings::prepared_read(&self.settings))
                .await;
            let source = child
                .local_call(|| {
                    child.admit_work(6)?;
                    match settings {
                        FileSettings::Global => {
                            self.project
                                .map(AnalysisSource::Global)
                                .ok_or(RunError::Contract(
                                    "prepared global settings have no project",
                                ))
                        }
                        FileSettings::File(settings) => {
                            Ok(AnalysisSource::File(&settings.analysis))
                        }
                    }
                })
                .await;
            match source {
                AnalysisSource::Global(project) => {
                    let settings = child
                        .read_field(
                            project.read_fields(self.db).settings(),
                            &ProjectSettingsBorrow,
                        )
                        .await;
                    Ok(child
                        .local_call(|| {
                            child.admit_work(3)?;
                            Ok(settings.analysis())
                        })
                        .await)
                }
                AnalysisSource::File(analysis) => Ok(analysis),
            }
        })
    }
}

enum RuleSource<'db> {
    Global(Project),
    File(&'db RuleSelection),
}

enum AnalysisSource<'db> {
    Global(Project),
    File(&'db AnalysisSettings),
}

struct ProjectSettingsBorrow;

impl FieldReadProfile<Box<Settings>> for ProjectSettingsBorrow {
    fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        _endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call Box<Settings>,
        mode: FieldReturnMode,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call {
        ready(if mode == FieldReturnMode::Deref {
            // Box dereference borrows the existing Settings allocation and owns no cleanup.
            Ok(NativeValueQuote {
                work: size_of::<&Settings>() + 1,
                requested_bytes: 0,
                cleanup_work: 0,
            })
        } else {
            Err(RunError::Contract(
                "project settings require a dereference read",
            ))
        })
    }
}

#[cfg(test)]
mod verbose_tests;

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use ruff_db::files::{File, system_path_to_file, vendored_path_to_file};
    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf, TestSystem};
    use ruff_db::testing::assert_function_query_was_not_run;
    use salsa::attempt_probe::{AttemptOutcome, Incomplete};
    use salsa::execution_probe::{ExecutionLimits, RegistryBuilder, try_with_execution_budget};
    use salsa::prepared_source_probe::{Stamp, capture, try_with_preparation};
    use ty_python_semantic::Db as _;

    use super::super::file_settings;
    use super::ProjectSettingsBorrow;
    use crate::db::testing::TestDb;
    use crate::{Db, ProjectDatabase, ProjectMetadata};

    fn check_host(db: &dyn Db, file: File, eligibility_edge: bool) {
        try_with_preparation(db, || db.prepare_analysis_file_settings(file)).unwrap();
        let host = db.prepare_analysis_host_reads(file).unwrap();
        let expected = (db.should_check_file(file), db.rule_selection(file));
        let settings_key = file_settings::prepare_memo(db, file)
            .unwrap()
            .database_key();
        let eligibility_key = crate::should_check_file::prepare_memo(db, file)
            .unwrap()
            .database_key();
        let host = &host;
        let captured = capture(db, || {
            try_with_execution_budget(
                db,
                ExecutionLimits {
                    semantic_work: 100_000,
                    requested_bytes: 1_000_000,
                },
                |budget| {
                    RegistryBuilder::with_budget(db, &budget)?
                        .seal()?
                        .run(|endpoint| async move {
                            let should_check = endpoint
                                .child_call(|| async {
                                    let (child, host) = endpoint
                                        .local_call(|| {
                                            endpoint.admit_work(40)?;
                                            Ok((endpoint.clone(), Rc::clone(host)))
                                        })
                                        .await;
                                    host.should_check_file(child)?.await
                                })
                                .await;
                            let rules = endpoint
                                .child_call(|| async {
                                    let (child, host) = endpoint
                                        .local_call(|| {
                                            endpoint.admit_work(40)?;
                                            Ok((endpoint.clone(), Rc::clone(host)))
                                        })
                                        .await;
                                    host.rule_selection(child)?.await
                                })
                                .await;
                            Ok((should_check, rules))
                        })
                },
            )
        })
        .unwrap();
        let Ok(AttemptOutcome::Complete(Ok(actual))) = captured.value else {
            panic!("prepared host read failed: {:?}", captured.value);
        };
        assert_eq!(actual.0, expected.0);
        assert!(std::ptr::eq(actual.1, expected.1));
        let keys: Vec<_> = captured.reads.iter().map(|read| read.key).collect();
        if eligibility_edge {
            assert_eq!(keys, [eligibility_key, settings_key]);
        } else {
            assert_eq!(keys, [settings_key]);
        }
        assert!(captured.reads.iter().all(|read| read.parent.is_none()));
        host.check_current().unwrap();
    }

    #[test]
    fn production_host_reads_global_override_and_script_rules() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPathBuf::from("/project");
        system.memory_file_system().write_files_all([
            (
                root.join("ty.toml"),
                r#"
                [rules]
                invalid-argument-type = "error"
                [[overrides]]
                include = ["overridden.py"]
                [overrides.rules]
                invalid-argument-type = "warn"
                "#,
            ),
            (root.join("main.py"), ""),
            (root.join("overridden.py"), ""),
            (
                root.join("script.py"),
                "# /// script\n# [tool.ty.rules]\n# invalid-argument-type = 'ignore'\n# ///\n",
            ),
        ])?;
        let metadata = ProjectMetadata::discover(&root, &system)?;
        let db = ProjectDatabase::fallible(metadata, system)?;
        for path in ["main.py", "overridden.py", "script.py"] {
            let file = system_path_to_file(&db, root.join(path))?;
            check_host(&db, file, true);
        }
        let vendored = vendored_path_to_file(&db, "stdlib/builtins.pyi")?;
        check_host(&db, vendored, false);
        Ok(())
    }

    #[test]
    fn test_host_reads_the_eligibility_memo_for_vendored_files() -> anyhow::Result<()> {
        let db = TestDb::new(ProjectMetadata::new("app", SystemPathBuf::from("/project")));
        let vendored = vendored_path_to_file(&db, "stdlib/builtins.pyi")?;
        check_host(&db, vendored, true);
        Ok(())
    }

    #[test]
    fn sealing_does_not_run_cold_settings_queries() -> anyhow::Result<()> {
        let root = SystemPathBuf::from("/project");
        let mut db = TestDb::new(ProjectMetadata::new("app", root.clone()));
        db.write_file(root.join("main.py"), "")?;
        let file = system_path_to_file(&db, root.join("main.py"))?;
        db.take_salsa_events();
        assert!(db.prepare_analysis_host_reads(file).is_err());
        let events = db.take_salsa_events();
        assert_function_query_was_not_run(&db, file_settings, file, &events);
        assert_function_query_was_not_run(&db, crate::should_check_file, file, &events);
        Ok(())
    }

    #[test]
    fn settings_dereference_read_refuses_and_retries_in_the_same_revision() {
        let db = TestDb::new(ProjectMetadata::new("app", SystemPathBuf::from("/project")));
        let project = db.project();
        let expected = project.settings(&db);
        let stamp = Stamp::current(&db);
        let delivered = Cell::new(false);

        for work in [16, 100_000] {
            let result = try_with_execution_budget(
                &db,
                ExecutionLimits {
                    semantic_work: work,
                    requested_bytes: 1_000_000,
                },
                |budget| {
                    let db = &db;
                    let delivered = &delivered;
                    RegistryBuilder::with_budget(db, &budget)?
                        .seal()?
                        .run(|endpoint| async move {
                            let settings = endpoint
                                .read_field(
                                    project.read_fields(db).settings(),
                                    &ProjectSettingsBorrow,
                                )
                                .await;
                            delivered.set(true);
                            Ok(settings)
                        })
                },
            );
            if work == 16 {
                // Field selection consumes this allowance; the dereference quote then refuses.
                assert_eq!(
                    result,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                );
                assert!(!delivered.get());
            } else {
                assert!(matches!(
                    result,
                    Ok(AttemptOutcome::Complete(Ok(actual))) if std::ptr::eq(actual, expected)
                ));
                assert!(delivered.get());
            }
            assert_eq!(Stamp::current(&db), stamp);
        }
    }
}
