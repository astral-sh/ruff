use ruff_db::PythonFile;
use ruff_db::files::File;
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{ResolverEnvironment, ResolverFile};

use crate::{Db, program::Program};

/// A physical file interpreted in one program.
#[salsa::interned(
    debug,
    revisions = usize::MAX,
    heap_size = ruff_memory_usage::heap_size
)]
pub struct ProgramFile<'db> {
    #[returns(copy)]
    pub file: File,

    #[returns(copy)]
    pub program: Program<'db>,
}

impl get_size2::GetSize for ProgramFile<'_> {}

#[salsa::tracked]
impl<'db> ProgramFile<'db> {
    /// Returns the parser key for this file and its program's Python version.
    #[salsa::tracked(returns(copy))]
    pub fn python_file(self, db: &'db dyn Db) -> PythonFile<'db> {
        PythonFile::new(db, self.file(db), self.program(db).python_version(db))
    }

    /// Returns the module-resolution environment for this program file.
    pub fn resolver_environment(self, db: &'db dyn Db) -> ResolverEnvironment<'db> {
        self.program(db).resolver_environment(db)
    }

    /// Returns the resolver key for this file.
    pub fn resolver_file(self, db: &'db dyn Db) -> ResolverFile<'db> {
        ResolverFile::new(db, self.file(db), self.resolver_environment(db))
    }

    /// Returns the Python version associated with this file's program.
    pub fn python_version(self, db: &'db dyn Db) -> PythonVersion {
        self.program(db).python_version(db)
    }
}
