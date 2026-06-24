use ruff_db::files::File;
use ty_module_resolver::Db as ModuleResolverDb;

/// Database giving access to semantic information about a Python program.
#[salsa::db]
pub trait Db: ModuleResolverDb {
    /// Returns `true` if the file should be checked.
    fn should_check_file(&self, file: File) -> bool;
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
    use ty_module_resolver::{
        Db as ModuleResolverDb, FallibleStrategy, SearchPathSettings, SearchPaths,
    };
    use ty_site_packages::{PythonVersionSource, PythonVersionWithSource};

    use crate::platform::PythonPlatform;
    use crate::program::{Program, ProgramSettings};

    use super::Db;

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            let events = Events::default();
            Self {
                storage: salsa::Storage::new(Some(Box::new({
                    move |event| {
                        tracing::trace!("event: {event:?}");
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored: ty_vendored::file_system().clone(),
                files: Files::default(),
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

        fn python_version(&self) -> PythonVersion {
            Program::get(self).python_version(self)
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn should_check_file(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }
    }

    #[salsa::db]
    impl ModuleResolverDb for TestDb {
        fn search_paths(&self) -> &SearchPaths {
            Program::get(self).search_paths(self)
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

            Program::from_settings(
                &db,
                ProgramSettings {
                    python_version: PythonVersionWithSource {
                        version: self.python_version,
                        source: PythonVersionSource::default(),
                    },
                    python_platform: self.python_platform,
                    search_paths: SearchPathSettings::new(vec![src_root])
                        .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                        .context("Invalid search path settings")?,
                },
            );

            Ok(db)
        }
    }
}
