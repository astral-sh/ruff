use ruff_db::files::File;
use ruff_db::parsed::VersionedFile;
use ty_module_resolver::{ModuleGlobSet, ProgramFile};

use crate::Db;
use crate::program::Program;

/// Settings that can change inferred types and must therefore participate in inference keys.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
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
pub struct AnalysisFile<'db> {
    pub program: Program<'db>,
    pub file: File,
}

// The Salsa allocation is tracked separately.
impl get_size2::GetSize for AnalysisFile<'_> {}

impl<'db> AnalysisFile<'db> {
    pub fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.program(db).file(db, self.file(db))
    }

    pub fn versioned_file(self, db: &'db dyn Db) -> VersionedFile<'db> {
        self.program(db).versioned_file(db, self.file(db))
    }
}
