use ruff_db::files::File;
use ruff_python_ast::PythonVersion;
use salsa::{Durability, Setter};

use crate::{Db, SearchPaths};

/// The portion of a Python environment used during parsing and module resolution.
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub struct ResolverProgram {
    pub python_version: PythonVersion,

    #[returns(ref)]
    pub search_paths: SearchPaths,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for ResolverProgram {}

#[salsa::input(singleton)]
struct DefaultResolverProgram {
    program: ResolverProgram,
}

impl ResolverProgram {
    pub fn create(db: &dyn Db, python_version: PythonVersion, search_paths: SearchPaths) -> Self {
        search_paths.try_register_static_roots(db);
        Self::builder(python_version, search_paths)
            .durability(Durability::HIGH)
            .new(db)
    }

    pub fn get(db: &dyn Db) -> Self {
        if let Some(program) = Self::try_get(db) {
            return program;
        }

        // Compatibility for databases that still initialize the resolver through the
        // `Db::python_version` and `Db::search_paths` accessors. New callers should create and
        // carry a `ResolverProgram` explicitly.
        let program = Self::create(db, db.python_version(), db.search_paths().clone());
        Self::ensure_default(db, program);
        program
    }

    pub fn try_get(db: &dyn Db) -> Option<Self> {
        DefaultResolverProgram::try_get(db).map(|default| default.program(db))
    }

    pub fn ensure_default(db: &dyn Db, program: Self) {
        if DefaultResolverProgram::try_get(db).is_none() {
            let _ = DefaultResolverProgram::builder(program)
                .durability(Durability::HIGH)
                .new(db);
        }
    }

    pub fn set_default(db: &mut dyn Db, program: Self) {
        match DefaultResolverProgram::try_get(db) {
            Some(default) if default.program(db) != program => {
                default.set_program(db).to(program);
            }
            Some(_) => {}
            None => {
                let _ = DefaultResolverProgram::builder(program)
                    .durability(Durability::HIGH)
                    .new(db);
            }
        }
    }

    pub fn update(self, db: &mut dyn Db, python_version: PythonVersion, search_paths: SearchPaths) {
        if self.search_paths(db) != &search_paths {
            search_paths.try_register_static_roots(db);
            self.set_search_paths(db).to(search_paths);
        }
        if self.python_version(db) != python_version {
            self.set_python_version(db).to(python_version);
        }
    }
}

/// A physical file interpreted in one module-resolution environment.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct ProgramFile<'db> {
    pub program: ResolverProgram,
    pub file: File,
}
