use ruff_db::files::File;
use ruff_db::parsed::{ParsedModule, VersionedFile, parsed_module};
use ty_module_resolver::{ModuleGlobSet, ResolverFile};

use crate::Db;
use crate::program::Program;

/// Settings that can change inferred types and must therefore participate in inference keys.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct InferenceSettings {
    pub replace_imports_with_any: ModuleGlobSet,
}

impl Default for InferenceSettings {
    fn default() -> Self {
        Self {
            replace_imports_with_any: ModuleGlobSet::empty(),
        }
    }
}

/// A physical file interpreted as part of one program.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct ProgramFile<'db> {
    #[returns(copy)]
    pub program: Program,
    #[returns(copy)]
    pub file: File,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for ProgramFile<'_> {}

impl<'db> ProgramFile<'db> {
    pub fn versioned_file(self, db: &'db dyn Db) -> VersionedFile<'db> {
        VersionedFile::new(db, self.file(db), self.program(db).python_version(db))
    }

    pub fn resolver_file(self, db: &'db dyn Db) -> ResolverFile<'db> {
        self.program(db).resolver_file(db, self.file(db))
    }

    pub fn parsed(self, db: &'db dyn Db) -> &'db ParsedModule {
        parsed_module(db, self.versioned_file(db))
    }
}
