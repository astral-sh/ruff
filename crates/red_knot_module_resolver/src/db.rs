use ruff_db::Upcast;

use crate::resolver::{
    file_to_module,
    internal::{ModuleNameIngredient, ModuleResolverSettings},
    resolve_module_query,
};
use crate::typeshed::parse_typeshed_versions;

#[salsa::jar(db=Db)]
pub struct Jar(
    ModuleNameIngredient<'_>,
    ModuleResolverSettings,
    resolve_module_query,
    file_to_module,
    parse_typeshed_versions,
);

pub trait Db: salsa::DbWithJar<Jar> + ruff_db::Db + Upcast<dyn ruff_db::Db> {}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync;

    use salsa::DebugWithDb;

    use ruff_db::files::Files;
    use ruff_db::system::{
        DbWithTestSystem, MemoryFileSystem, SystemPath, SystemPathBuf, TestSystem,
    };
    use ruff_db::vendored::VendoredFileSystem;

    use crate::resolver::{set_module_resolution_settings, RawModuleResolutionSettings};
    use crate::supported_py_version::TargetVersion;
    use crate::vendored_typeshed_stubs;

    use super::*;

    #[salsa::db(Jar, ruff_db::Jar)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        system: TestSystem,
        vendored: VendoredFileSystem,
        files: Files,
        events: sync::Arc<sync::Mutex<Vec<salsa::Event>>>,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: vendored_typeshed_stubs().snapshot(),
                events: sync::Arc::default(),
                files: Files::default(),
            }
        }

        /// Takes the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = sync::Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

            let events = inner.get_mut().unwrap();
            std::mem::take(&mut *events)
        }

        /// Clears the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_salsa_events();
        }
    }

    impl Upcast<dyn ruff_db::Db> for TestDb {
        fn upcast(&self) -> &(dyn ruff_db::Db + 'static) {
            self
        }
    }

    impl ruff_db::Db for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn ruff_db::system::System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }
    }

    impl Db for TestDb {}

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: salsa::Event) {
            tracing::trace!("event: {:?}", event.debug(self));
            let mut events = self.events.lock().unwrap();
            events.push(event);
        }
    }

    impl salsa::ParallelDatabase for TestDb {
        fn snapshot(&self) -> salsa::Snapshot<Self> {
            salsa::Snapshot::new(Self {
                storage: self.storage.snapshot(),
                system: self.system.snapshot(),
                vendored: self.vendored.snapshot(),
                files: self.files.snapshot(),
                events: self.events.clone(),
            })
        }
    }

    pub(crate) struct TestCaseBuilder {
        db: TestDb,
        src: SystemPathBuf,
        site_packages: SystemPathBuf,
        target_version: Option<TargetVersion>,
        with_vendored_stubs: bool,
    }

    impl TestCaseBuilder {
        #[must_use]
        pub(crate) fn with_target_version(mut self, target_version: TargetVersion) -> Self {
            self.target_version = Some(target_version);
            self
        }

        #[must_use]
        pub(crate) fn with_vendored_stubs_used(mut self) -> Self {
            self.with_vendored_stubs = true;
            self
        }

        fn create_mocked_typeshed(
            typeshed_dir: &SystemPath,
            fs: &MemoryFileSystem,
        ) -> std::io::Result<()> {
            static VERSIONS_DATA: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11
            collections: 3.9-           # 'Regular' package on py39+
            functools: 3.8-
            importlib: 3.9-             # Namespace package on py39+
            xml: 3.8-3.8                # Namespace package on py38 only
            ";

            fs.create_directory_all(typeshed_dir)?;
            fs.write_file(typeshed_dir.join("stdlib/VERSIONS"), VERSIONS_DATA)?;

            // Regular package on py38+
            fs.create_directory_all(typeshed_dir.join("stdlib/asyncio"))?;
            fs.touch(typeshed_dir.join("stdlib/asyncio/__init__.pyi"))?;
            fs.write_file(
                typeshed_dir.join("stdlib/asyncio/tasks.pyi"),
                "class Task: ...",
            )?;

            // Regular package on py39+
            fs.create_directory_all(typeshed_dir.join("stdlib/collections"))?;
            fs.touch(typeshed_dir.join("stdlib/collections/__init__.pyi"))?;

            // Namespace package on py38 only
            fs.create_directory_all(typeshed_dir.join("stdlib/xml"))?;
            fs.touch(typeshed_dir.join("stdlib/xml/etree.pyi"))?;

            // Namespace package on py39+
            fs.create_directory_all(typeshed_dir.join("stdlib/importlib"))?;
            fs.touch(typeshed_dir.join("stdlib/importlib/abc.pyi"))?;

            fs.write_file(
                typeshed_dir.join("stdlib/functools.pyi"),
                "def update_wrapper(): ...",
            )
        }

        pub(crate) fn build(self) -> std::io::Result<TestCase> {
            let TestCaseBuilder {
                mut db,
                src,
                with_vendored_stubs,
                site_packages,
                target_version,
            } = self;

            let typeshed_dir = SystemPathBuf::from("/typeshed");

            let custom_typeshed = if with_vendored_stubs {
                None
            } else {
                Self::create_mocked_typeshed(&typeshed_dir, db.memory_file_system())?;
                Some(typeshed_dir.clone())
            };

            let settings = RawModuleResolutionSettings {
                target_version: target_version.unwrap_or_default(),
                extra_paths: vec![],
                workspace_root: src.clone(),
                custom_typeshed: custom_typeshed.clone(),
                site_packages: Some(site_packages.clone()),
            };

            set_module_resolution_settings(&mut db, settings);

            Ok(TestCase {
                db,
                src: src.clone(),
                custom_typeshed: typeshed_dir,
                site_packages: site_packages.clone(),
            })
        }
    }

    pub(crate) struct TestCase {
        pub(crate) db: TestDb,
        pub(crate) src: SystemPathBuf,
        pub(crate) custom_typeshed: SystemPathBuf,
        pub(crate) site_packages: SystemPathBuf,
    }

    pub(crate) fn create_resolver_builder() -> std::io::Result<TestCaseBuilder> {
        let db = TestDb::new();

        let src = SystemPathBuf::from("/src");
        let site_packages = SystemPathBuf::from("/site_packages");

        let fs = db.memory_file_system();

        fs.create_directory_all(&src)?;
        fs.create_directory_all(&site_packages)?;

        Ok(TestCaseBuilder {
            db,
            src,
            with_vendored_stubs: false,
            site_packages,
            target_version: None,
        })
    }
}
