use ruff_db::PythonFile;
use ruff_db::files::File;
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{ResolverEnvironment, ResolverFile};

use crate::{Db, Program};

/// A physical file interpreted in one module-resolution environment.
#[salsa::interned(
    debug,
    constructor = new_internal,
    revisions = usize::MAX,
    heap_size = ruff_memory_usage::heap_size
)]
pub struct ProgramFile<'db> {
    /// The cached parser key for `file` and the environment's Python version.
    #[returns(copy)]
    pub python_file: PythonFile<'db>,

    #[returns(copy)]
    pub resolver_environment: ResolverEnvironment<'db>,
}

impl get_size2::GetSize for ProgramFile<'_> {}

impl<'db> ProgramFile<'db> {
    pub fn new(
        db: &'db dyn Db,
        file: File,
        resolver_environment: ResolverEnvironment<'db>,
    ) -> Self {
        let python_file = PythonFile::new(db, file, resolver_environment.python_version(db));
        Self::new_internal(db, python_file, resolver_environment)
    }

    /// Returns the physical file represented by this program file.
    pub fn file(self, db: &'db dyn Db) -> File {
        self.python_file(db).file(db)
    }

    /// Returns the resolver key for this file.
    pub fn resolver_file(self, db: &'db dyn Db) -> ResolverFile<'db> {
        ResolverFile::new(db, self.file(db), self.resolver_environment(db))
    }

    /// Returns the program associated with this file.
    pub fn program(self, db: &'db dyn Db) -> Program<'db> {
        self.resolver_environment(db)
    }

    /// Returns the Python version associated with this file's resolver environment.
    pub fn python_version(self, db: &'db dyn Db) -> PythonVersion {
        self.resolver_environment(db).python_version(db)
    }
}
