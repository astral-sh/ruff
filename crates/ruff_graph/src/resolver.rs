use ruff_db::files::FilePath;
use ty_python_semantic::{ModuleName, resolve_module, resolve_real_module};

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
    pub(crate) fn resolve(&self, import: CollectedImport) -> impl Iterator<Item = &'a FilePath> {
        match import {
            CollectedImport::Import(import) => {
                // Attempt to resolve the module (e.g., given `import foo`, look for `foo`).
                let file = self.resolve_module(&import);

                // If the file is a stub, look for the corresponding source file.
                let source_file = file
                    .is_some_and(|file| file.extension() == Some("pyi"))
                    .then(|| self.resolve_real_module(&import))
                    .flatten();

                std::iter::once(file)
                    .chain(std::iter::once(source_file))
                    .flatten()
            }
            CollectedImport::ImportFrom(import) => {
                // Attempt to resolve the member (e.g., given `from foo import bar`, look for `foo.bar`).
                if let Some(file) = self.resolve_module(&import) {
                    // If the file is a stub, look for the corresponding source file.
                    let source_file = (file.extension() == Some("pyi"))
                        .then(|| self.resolve_real_module(&import))
                        .flatten();

                    return std::iter::once(Some(file))
                        .chain(std::iter::once(source_file))
                        .flatten();
                }

                // Attempt to resolve the module (e.g., given `from foo import bar`, look for `foo`).
                let parent = import.parent();
                let file = parent
                    .as_ref()
                    .and_then(|parent| self.resolve_module(parent));

                // If the file is a stub, look for the corresponding source file.
                let source_file = file
                    .is_some_and(|file| file.extension() == Some("pyi"))
                    .then(|| {
                        parent
                            .as_ref()
                            .and_then(|parent| self.resolve_real_module(parent))
                    })
                    .flatten();

                std::iter::once(file)
                    .chain(std::iter::once(source_file))
                    .flatten()
            }
        }
    }

    /// Resolves a module name to a module.
    pub(crate) fn resolve_module(&self, module_name: &ModuleName) -> Option<&'a FilePath> {
        let module = resolve_module(self.db, module_name)?;
        Some(module.file(self.db)?.path(self.db))
    }

    /// Resolves a module name to a module (stubs not allowed).
    fn resolve_real_module(&self, module_name: &ModuleName) -> Option<&'a FilePath> {
        let module = resolve_real_module(self.db, module_name)?;
        Some(module.file(self.db)?.path(self.db))
    }
}
