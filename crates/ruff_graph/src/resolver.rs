use red_knot_python_semantic::{ModuleName, SemanticModel};
use ruff_db::files::FilePath;

use crate::collector::CollectedImport;

/// Collect all imports for a given Python file.
pub(crate) struct Resolver<'a> {
    semantic: &'a SemanticModel<'a>,
    module_path: Option<&'a [String]>,
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(semantic: &'a SemanticModel<'a>, module_path: Option<&'a [String]>) -> Self {
        Self {
            semantic,
            module_path,
        }
    }

    pub(crate) fn resolve(&self, import: CollectedImport) -> Option<&'a FilePath> {
        match import {
            CollectedImport::Import(import) => self
                .semantic
                .resolve_module(import)
                .map(|module| module.file().path(self.semantic.db())),
            CollectedImport::ImportFrom(import) => {
                // If the import is relative, resolve it relative to the current module.
                let import = if import
                    .components()
                    .next()
                    .is_some_and(|segment| segment == ".")
                {
                    from_relative_import(self.module_path?, import)?
                } else {
                    import
                };

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

/// Format the call path for a relative import, or `None` if the relative import extends beyond
/// the root module.
fn from_relative_import(
    // The path from which the import is relative.
    module: &[String],
    // The path of the import itself (e.g., given `from ..foo import bar`, `[".", ".", "foo", "bar]`).
    import: ModuleName,
) -> Option<ModuleName> {
    let mut components = Vec::with_capacity(module.len() + import.components().count());

    // Start with the module path.
    components.extend(module.iter().map(String::as_str));

    // Remove segments based on the number of dots.
    for segment in import.components() {
        if segment == "." {
            if components.is_empty() {
                return None;
            }
            components.pop();
        } else {
            components.push(segment);
        }
    }

    ModuleName::from_components(components)
}
