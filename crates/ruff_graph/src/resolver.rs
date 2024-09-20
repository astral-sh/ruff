use red_knot_python_semantic::SemanticModel;
use ruff_db::files::FilePath;

use crate::collector::CollectedImport;

/// Collect all imports for a given Python file.
pub(crate) struct Resolver<'a> {
    semantic: &'a SemanticModel<'a>,
}

impl<'a> Resolver<'a> {
    /// Initialize a [`Resolver`] with a given [`SemanticModel`].
    pub(crate) fn new(semantic: &'a SemanticModel<'a>) -> Self {
        Self { semantic }
    }

    /// Resolve the [`CollectedImport`] into a [`FilePath`].
    pub(crate) fn resolve(&self, import: CollectedImport) -> Option<&'a FilePath> {
        match import {
            CollectedImport::Import(import) => self
                .semantic
                .resolve_module(import)
                .map(|module| module.file().path(self.semantic.db())),
            CollectedImport::ImportFrom(import) => {
                // Attempt to resolve the member (e.g., given `from foo import bar`, look for `foo.bar`).
                let parent = import.parent();
                self.semantic
                    .resolve_module(import)
                    .map(|module| module.file().path(self.semantic.db()))
                    .or_else(|| {
                        // Attempt to resolve the module (e.g., given `from foo import bar`, look for `foo`).
                        self.semantic
                            .resolve_module(parent?)
                            .map(|module| module.file().path(self.semantic.db()))
                    })
            }
        }
    }
}
