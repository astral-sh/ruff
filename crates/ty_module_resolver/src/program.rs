use ruff_db::files::File;
use ruff_python_ast::PythonVersion;

use crate::{Db, SearchPaths};

/// The portion of a Python program used during parsing and module resolution.
#[salsa::interned(
    debug,
    heap_size = ruff_memory_usage::heap_size
)]
pub struct ResolverProgram<'db> {
    pub python_version: PythonVersion,

    #[returns(ref)]
    pub search_paths: SearchPaths,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for ResolverProgram<'_> {}

impl<'db> ResolverProgram<'db> {
    pub fn create(
        db: &'db dyn Db,
        python_version: PythonVersion,
        search_paths: &SearchPaths,
    ) -> Self {
        search_paths.try_register_static_roots(db);
        Self::new(db, python_version, search_paths)
    }
}

/// A physical file interpreted in one module-resolution environment.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct ProgramFile<'db> {
    pub program: ResolverProgram<'db>,
    pub file: File,
}
