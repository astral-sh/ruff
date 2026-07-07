use ruff_db::files::File;
use ruff_python_ast::PythonVersion;
use salsa::Durability;

use crate::{Db, SearchPaths};

/// The portion of a Python program used during parsing and module resolution.
#[salsa::input(heap_size = ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub struct ResolverProgram {
    #[returns(copy)]
    pub python_version: PythonVersion,

    #[returns(ref)]
    pub search_paths: SearchPaths,
}

impl get_size2::GetSize for ResolverProgram {}

impl ResolverProgram {
    pub fn create(db: &dyn Db, python_version: PythonVersion, search_paths: &SearchPaths) -> Self {
        Self::from_settings(db, python_version, search_paths.clone())
    }

    fn from_settings(
        db: &dyn Db,
        python_version: PythonVersion,
        search_paths: SearchPaths,
    ) -> Self {
        search_paths.try_register_static_roots(db);
        Self::builder(python_version, search_paths)
            .durability(Durability::NEVER_CHANGE)
            .new(db)
    }

    #[must_use]
    pub fn with_settings(
        self,
        db: &dyn Db,
        python_version: PythonVersion,
        search_paths: SearchPaths,
    ) -> Self {
        if self.python_version(db) == python_version && self.search_paths(db) == &search_paths {
            self
        } else {
            Self::from_settings(db, python_version, search_paths)
        }
    }
}

/// A physical file interpreted in one module-resolution environment.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct ResolverFile<'db> {
    #[returns(copy)]
    pub program: ResolverProgram,
    #[returns(copy)]
    pub file: File,
}
