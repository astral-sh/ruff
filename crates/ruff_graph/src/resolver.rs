use ruff_db::files::FilePath;
use ty_python_semantic::resolve_module;

use crate::ModuleDb;
use crate::collector::CollectedImport;

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
                let module = resolve_module(self.db, &import)?;
                Some(module.file()?.path(self.db))
            }
            CollectedImport::ImportFrom(import) => {
                // Attempt to resolve the member (e.g., given `from foo import bar`, look for `foo.bar`).
                let parent = import.parent();

                let module = resolve_module(self.db, &import).or_else(|| {
                    // Attempt to resolve the module (e.g., given `from foo import bar`, look for `foo`).

                    resolve_module(self.db, &parent?)
                })?;

                Some(module.file()?.path(self.db))
            }
        }
    }
}
