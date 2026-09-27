use ruff_db::PythonFile;
use ruff_db::files::File;
use ruff_python_ast::PythonVersion;
use ty_module_resolver::{ResolverEnvironment, ResolverFile};

use crate::{Db, program::Program};

/// A file interpreted within a particular Python program.
///
/// The same file can participate in multiple programs, each with different Python versions, search
/// paths, or other settings that affect type inference.
///
/// For example:
///
/// ```text
/// project/
/// ├── app.py         # Project program: Python 3.11
/// ├── generate.py    # Script program:  Python 3.12
/// └── shared.py      # Imported by both
/// ```
///
/// In `shared.py`, version-dependent code can produce different types:
///
/// ```python
/// import sys
///
/// if sys.version_info >= (3, 12):
///     value = 1
/// else:
///     value = "one"
/// ```
///
/// The two interpretations therefore need separate semantic identities:
///
/// ```text
/// ProgramFile(shared.py, project program) -> value: str
/// ProgramFile(shared.py, script program)  -> value: int
/// ```
///
/// Semantic queries, such as `semantic_index`, use `ProgramFile` to avoid sharing results between
/// incompatible programs. Lower-level operations use narrower identities where possible:
///
/// ```text
/// program_file.python_file(db)   -> File + Python version
/// program_file.resolver_file(db) -> File + resolver environment
/// ```
///
/// This allows programs with the same Python version to share parsed syntax, and programs with
/// equivalent resolver environments to share module resolution, while keeping type inference
/// isolated.
#[salsa::interned(
    debug,
    constructor = new_internal,
    heap_size = ruff_memory_usage::heap_size
)]
pub struct ProgramFile<'db> {
    /// Cache the parser key even though its Python version is redundant with `program`:
    /// program files are created infrequently, but their parser keys are looked up extensively.
    #[returns(copy)]
    pub python_file: PythonFile<'db>,

    #[returns(copy)]
    pub program: Program<'db>,
}

impl get_size2::GetSize for ProgramFile<'_> {}

impl<'db> ProgramFile<'db> {
    pub fn new(db: &'db dyn Db, file: File, program: Program<'db>) -> Self {
        let python_file = PythonFile::new(db, file, program.python_version(db));
        Self::new_internal(db, python_file, program)
    }

    /// Returns the physical file represented by this program file.
    pub fn file(self, db: &'db dyn Db) -> File {
        self.python_file(db).file(db)
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
