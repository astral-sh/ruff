use ruff_db::files::File;
use ty_module_resolver::Db as ModuleResolverDb;

#[cfg(any(test, feature = "testing"))]
use crate::program::{Program, ProgramSettings};

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: ModuleResolverDb {
    /// Returns `true` if the file should be checked.
    fn should_check_file(&self, file: File) -> bool;
}

#[cfg(any(test, feature = "testing"))]
#[salsa::db]
pub trait TestProgramDb: Db {
    fn program_settings(&self) -> &ProgramSettings;

    // Salsa-cached because interning a program requires hashing all search paths.
    fn program(&self) -> Program<'_>
    where
        Self: Sized,
    {
        #[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
        fn program_inner(db: &dyn TestProgramDb) -> Program<'_> {
            Program::from_settings(db, db.program_settings().clone())
        }

        program_inner(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use anyhow::Context;

    use ruff_db::Db as SourceDb;
    use ruff_db::files::{File, Files};
    use ruff_db::system::{
        DbWithTestSystem, DbWithWritableSystem as _, System, SystemPath, SystemPathBuf, TestSystem,
    };
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_python_ast::PythonVersion;
    use ty_module_resolver::{Db as ModuleResolverDb, FallibleStrategy, SearchPathSettings};
    use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

    use crate::platform::PythonPlatform;
    use crate::program::ProgramSettings;

    use super::{Db, TestProgramDb};

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        program_settings: ProgramSettings,
    }

    impl TestDb {
        fn new() -> Self {
            let events = Events::default();
            let vendored = ty_vendored::file_system().clone();
            let program_settings = ProgramSettings::empty(&vendored);
            Self {
                storage: salsa::Storage::new(Some(Box::new({
                    move |event| {
                        tracing::trace!("event: {event:?}");
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored,
                files: Files::default(),
                program_settings,
            }
        }
    }

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    #[salsa::db]
    impl SourceDb for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn should_check_file(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }
    }

    #[salsa::db]
    impl ModuleResolverDb for TestDb {}

    #[salsa::db]
    impl TestProgramDb for TestDb {
        fn program_settings(&self) -> &ProgramSettings {
            &self.program_settings
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    pub(crate) struct TestDbBuilder<'a> {
        /// Target Python version
        python_version: PythonVersion,
        /// Target Python platform
        python_platform: PythonPlatform,
        /// Path and content pairs for files that should be present
        files: Vec<(&'a str, &'a str)>,
    }

    impl<'a> TestDbBuilder<'a> {
        pub(crate) fn new() -> Self {
            Self {
                python_version: PythonVersion::default(),
                python_platform: PythonPlatform::default(),
                files: vec![],
            }
        }

        pub(crate) fn with_file(
            mut self,
            path: &'a (impl AsRef<SystemPath> + ?Sized),
            content: &'a str,
        ) -> Self {
            self.files.push((path.as_ref().as_str(), content));
            self
        }

        pub(crate) fn build(self) -> anyhow::Result<TestDb> {
            let mut db = TestDb::new();

            let src_root = SystemPathBuf::from("/src");
            db.memory_file_system().create_directory_all(&src_root)?;

            db.write_files(self.files)
                .context("Failed to write test files")?;

            let program_settings = ProgramSettings {
                python_version: PythonVersionWithSource {
                    version: self.python_version,
                    source: PythonVersionSource::default(),
                },
                python_platform: self.python_platform,
                search_paths: SearchPathSettings::new(vec![src_root])
                    .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                    .context("Invalid search path settings")?,
            };
            program_settings.search_paths.try_register_static_roots(&db);
            db.program_settings = program_settings;

            Ok(db)
        }
    }
}
