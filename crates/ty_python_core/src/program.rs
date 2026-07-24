use crate::{Db, platform::PythonPlatform};

use ruff_db::files::File;
use ruff_db::system::SystemPath;
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use salsa::Durability;
use salsa::Setter;
use ty_module_resolver::{ResolverEnvironment, SearchPaths};
use ty_site_packages::PythonVersionWithSource;

use crate::ProgramFile;

// Re-export the misconfiguration strategy types from ty_module_resolver.
pub use ty_module_resolver::{FallibleStrategy, MisconfigurationStrategy, UseDefaultStrategy};

#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub struct Program {
    // FIXME: Move the source out of `Program`. Different source locations prevent otherwise
    // equivalent programs from being reused across scripts.
    #[returns(ref)]
    pub python_version_with_source: PythonVersionWithSource,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(ref)]
    pub search_paths: SearchPaths,
}

impl get_size2::GetSize for Program {}

#[salsa::tracked]
impl Program {
    pub fn from_settings(db: &dyn Db, settings: ProgramSettings) -> Self {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        search_paths.try_register_static_roots(db);

        Program::builder(python_version, python_platform, search_paths)
            .durability(Durability::HIGH)
            .new(db)
    }

    pub fn python_version(self, db: &dyn Db) -> PythonVersion {
        self.python_version_with_source(db).version
    }

    /// Returns the module-resolution environment for this program.
    #[salsa::tracked(returns(copy))]
    pub fn resolver_environment(self, db: &dyn Db) -> ResolverEnvironment<'_> {
        ResolverEnvironment::new(db, self.python_version(db), self.search_paths(db))
    }

    pub fn program_file(self, db: &dyn Db, file: File) -> ProgramFile<'_> {
        ProgramFile::new(db, file, self)
    }

    pub fn update_from_settings(self, db: &mut dyn Db, settings: ProgramSettings) {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        if self.search_paths(db) != &search_paths {
            tracing::debug!("Updating search paths");
            search_paths.try_register_static_roots(db);
            self.set_search_paths(db).to(search_paths);
        }

        if &python_platform != self.python_platform(db) {
            tracing::debug!("Updating python platform: `{python_platform:?}`");
            self.set_python_platform(db).to(python_platform);
        }

        if &python_version != self.python_version_with_source(db) {
            tracing::debug!(
                "Updating python version: Python {version}",
                version = python_version.version
            );
            self.set_python_version_with_source(db).to(python_version);
        }
    }

    /// Permanently freezes all program inputs.
    pub fn freeze(self, db: &mut dyn Db) {
        let durability = Durability::NEVER_CHANGE;
        let python_version = self.python_version_with_source(db).clone();
        let python_platform = self.python_platform(db).clone();
        let search_paths = self.search_paths(db).clone();

        self.set_python_version_with_source(db)
            .with_durability(durability)
            .to(python_version);
        self.set_python_platform(db)
            .with_durability(durability)
            .to(python_platform);
        self.set_search_paths(db)
            .with_durability(durability)
            .to(search_paths);
    }

    pub fn custom_stdlib_search_path(self, db: &dyn Db) -> Option<&SystemPath> {
        self.search_paths(db).custom_stdlib()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramSettings {
    pub python_version: PythonVersionWithSource,
    pub python_platform: PythonPlatform,
    pub search_paths: SearchPaths,
}

impl ProgramSettings {
    pub fn empty(vendored: &VendoredFileSystem) -> Self {
        Self {
            python_version: PythonVersionWithSource::default(),
            python_platform: PythonPlatform::default(),
            search_paths: SearchPaths::empty(vendored),
        }
    }
}
