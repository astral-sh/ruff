use crate::{Db, platform::PythonPlatform};

use ruff_db::files::File;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{ResolverEnvironment, SearchPaths};
use ty_site_packages::PythonVersionWithSource;

use crate::ProgramFile;

// Re-export the misconfiguration strategy types from ty_module_resolver.
pub use ty_module_resolver::{FallibleStrategy, MisconfigurationStrategy, UseDefaultStrategy};

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct Program<'db> {
    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(copy)]
    pub resolver_environment: ResolverEnvironment<'db>,

    #[returns(ref)]
    pub python_executable: Option<SystemPathBuf>,
}

impl get_size2::GetSize for Program<'_> {}

impl<'db> Program<'db> {
    /// Creates a program from settings whose search roots have already been registered.
    pub fn from_settings(db: &'db dyn Db, settings: &ProgramSettings) -> Self {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
            python_executable,
        } = settings;

        let resolver_environment =
            ResolverEnvironment::new(db, python_version.version, search_paths);
        Program::new(db, python_platform, resolver_environment, python_executable)
    }

    pub fn python_version(self, db: &'db dyn Db) -> PythonVersion {
        self.resolver_environment(db).python_version(db)
    }

    pub fn search_paths(self, db: &'db dyn Db) -> &'db SearchPaths {
        self.resolver_environment(db).search_paths(db)
    }

    pub fn program_file(self, db: &'db dyn Db, file: File) -> ProgramFile<'db> {
        ProgramFile::new(db, file, self)
    }

    pub fn custom_stdlib_search_path(self, db: &'db dyn Db) -> Option<&'db SystemPath> {
        self.search_paths(db).custom_stdlib()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct ProgramSettings {
    pub python_version: PythonVersionWithSource,
    pub python_platform: PythonPlatform,
    pub python_executable: Option<SystemPathBuf>,
    pub search_paths: SearchPaths,
}

impl ProgramSettings {
    pub fn empty(vendored: &VendoredFileSystem) -> Self {
        Self {
            python_version: PythonVersionWithSource::default(),
            python_platform: PythonPlatform::default(),
            search_paths: SearchPaths::empty(vendored),
            python_executable: None,
        }
    }
}
