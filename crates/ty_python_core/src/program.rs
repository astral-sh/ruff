use crate::{Db, platform::PythonPlatform};

use ruff_db::files::File;
use ruff_db::system::SystemPath;
use ruff_db::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{ResolverEnvironment, SearchPaths};
use ty_site_packages::PythonVersionWithSource;

use crate::ProgramFile;

// Re-export the misconfiguration strategy types from ty_module_resolver.
pub use ty_module_resolver::{FallibleStrategy, MisconfigurationStrategy, UseDefaultStrategy};

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct Program<'db> {
    // FIXME: Move the source out of `Program`. Different source locations prevent otherwise
    // equivalent programs from being reused across scripts.
    #[returns(ref)]
    pub python_version_with_source: PythonVersionWithSource,

    #[returns(ref)]
    pub python_platform: PythonPlatform,

    #[returns(ref)]
    pub search_paths: SearchPaths,
}

impl get_size2::GetSize for Program<'_> {}

#[salsa::tracked]
impl<'db> Program<'db> {
    pub fn from_settings(db: &'db dyn Db, settings: ProgramSettings) -> Self {
        let ProgramSettings {
            python_version,
            python_platform,
            search_paths,
        } = settings;

        search_paths.try_register_static_roots(db);

        Program::new(db, python_version, python_platform, search_paths)
    }

    pub fn python_version(self, db: &'db dyn Db) -> PythonVersion {
        self.python_version_with_source(db).version
    }

    /// Returns the module-resolution environment for this program.
    #[salsa::tracked(returns(copy))]
    pub fn resolver_environment(self, db: &'db dyn Db) -> ResolverEnvironment<'db> {
        ResolverEnvironment::new(db, self.python_version(db), self.search_paths(db))
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
