use red_knot_python_semantic::resolve_module;
use ruff_db::files::FilePath;

use crate::collector::CollectedImport;
use crate::ModuleDb;

/// Collect all imports for a given Python file.
pub(crate) struct Resolver<'a> {
    db: &'a ModuleDb,
}

impl<'a> Resolver<'a> {
    /// Initialize a [`Resolver`] with a given [`ModuleDb`].
    pub(crate) fn new(db: &'a ModuleDb) -> Self {
        Self { db }
    }

    /// Resolve the [`CollectedImport`] into a [`FilePath`].
    pub(crate) fn resolve(&self, import: CollectedImport) -> Option<&'a FilePath> {
        match import {
            CollectedImport::Import(import) => {
                resolve_module(self.db, import).map(|module| module.file().path(self.db))
            }
            CollectedImport::ImportFrom(import) => {
                // Attempt to resolve the member (e.g., given `from foo import bar`, look for `foo.bar`).
                let parent = import.parent();

                resolve_module(self.db, import)
                    .map(|module| module.file().path(self.db))
                    .or_else(|| {
                        // Attempt to resolve the module (e.g., given `from foo import bar`, look for `foo`).

                        resolve_module(self.db, parent?).map(|module| module.file().path(self.db))
                    })
            }
        }
    }
}
