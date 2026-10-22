use crate::{Db, platform::PythonPlatform};

use std::fmt;

use ruff_db::files::File;
use ruff_db::system::SystemPath;
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{ResolverEnvironment, SearchPaths};
use ty_site_packages::{PythonEnvironment, PythonVersionWithSource};

use crate::ProgramFile;

// Re-export the misconfiguration strategy types from ty_module_resolver.
pub use ty_module_resolver::{FallibleStrategy, MisconfigurationStrategy, UseDefaultStrategy};

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct Program<'db> {
    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(copy)]
    pub resolver_environment: ResolverEnvironment<'db>,
}

impl get_size2::GetSize for Program<'_> {}

impl<'db> Program<'db> {
    /// Creates a program from settings whose search roots have already been registered.
    pub fn from_settings(db: &'db dyn Db, settings: &ProgramSettings) -> Self {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
            python_environment: _,
        } = settings;

        let resolver_environment =
            ResolverEnvironment::new(db, python_version.version, search_paths);
        Program::new(db, python_platform, resolver_environment)
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
    pub search_paths: SearchPaths,
    /// The resolved Python environment. Queries must use this instead of resolving the environment
    /// again, so that changes invalidate their cached results.
    pub python_environment: Result<Option<PythonEnvironment>, PythonEnvironmentError>,
}

impl ProgramSettings {
    pub fn empty(vendored: &VendoredFileSystem) -> Self {
        Self {
            python_version: PythonVersionWithSource::default(),
            python_platform: PythonPlatform::default(),
            search_paths: SearchPaths::empty(vendored),
            python_environment: Ok(None),
        }
    }

    /// The root of the resolved virtual environment, for watching `pyvenv.cfg` and directory changes.
    /// System installations are unlikely to be recreated, and watching a prefix such as `/usr`
    /// recursively would be expensive.
    pub fn virtual_environment(&self) -> Option<&SystemPath> {
        let environment = match &self.python_environment {
            Ok(environment) => environment,
            Err(error) => &error.last_usable,
        };
        environment
            .as_ref()
            .filter(|environment| environment.is_virtual())
            .map(|environment| &**environment.sys_prefix())
    }
}

/// A failed environment selection that retains the previous environment for watching and recovery.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct PythonEnvironmentError {
    pub message: Box<str>,
    pub last_usable: Option<PythonEnvironment>,
}

impl fmt::Display for PythonEnvironmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for PythonEnvironmentError {}
