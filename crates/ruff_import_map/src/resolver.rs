use crate::db::ModuleDb;
use red_knot_python_semantic::{Module, ModuleName};
use ruff_db::files::FilePath;

use crate::collector::CollectedImport;
use ruff_python_ast::name::{QualifiedName, QualifiedNameBuilder};
use ruff_python_stdlib::identifiers::is_identifier;
use std::borrow::Cow;

/// Collect all imports for a given Python file.
pub(crate) struct Resolver<'ast> {
    module_path: Option<&'ast [String]>,
    db: &'ast ModuleDb,
}

impl<'ast> Resolver<'ast> {
    pub(crate) fn new(module_path: Option<&'ast [String]>, db: &'ast ModuleDb) -> Self {
        Self { module_path, db }
    }

    pub(crate) fn resolve(&self, import: &'ast CollectedImport<'ast>) -> Option<&'ast FilePath> {
        match import {
            CollectedImport::Import(import) => Some(
                resolve_module(self.db, import.segments())?
                    .file()
                    .path(self.db),
            ),
            CollectedImport::ImportFrom(import) => {
                let module_path = if import.is_unresolved_import() {
                    // Only fix is the module path is known, and the import doesn't extend beyond it.
                    let module_path = from_relative_import(self.module_path?, import)?;

                    // Require import to be a valid module:
                    // https://python.org/dev/peps/pep-0008/#package-and-module-names
                    if !module_path
                        .segments()
                        .iter()
                        .all(|segment| is_identifier(segment))
                    {
                        return None;
                    }

                    Cow::Owned(module_path)
                } else {
                    Cow::Borrowed(import)
                };
                // Attempt to resolve the member (e.g., given `from foo import bar`, look for `foo.bar`).
                Some(
                    resolve_module(self.db, module_path.segments())
                        .or_else(|| {
                            // Attempt to resolve the module (e.g., given `import foo`, look for `foo`).
                            let segments = if module_path.segments().len() > 1 {
                                Some(&module_path.segments()[..module_path.segments().len() - 1])
                            } else {
                                None
                            }?;
                            resolve_module(self.db, segments)
                        })?
                        .file()
                        .path(self.db),
                )
            }
        }
    }
}

/// Format the call path for a relative import, or `None` if the relative import extends beyond
/// the root module.
fn from_relative_import<'a>(
    // The path from which the import is relative.
    module: &'a [String],
    // The path of the import itself (e.g., given `from ..foo import bar`, `[".", ".", "foo", "bar]`).
    import: &QualifiedName<'a>,
) -> Option<QualifiedName<'a>> {
    let mut qualified_name_builder =
        QualifiedNameBuilder::with_capacity(module.len() + import.segments().len());

    // Start with the module path.
    qualified_name_builder.extend(module.iter().map(String::as_str));

    // Remove segments based on the number of dots.
    for segment in import.segments() {
        if *segment == "." {
            if qualified_name_builder.is_empty() {
                return None;
            }
            qualified_name_builder.pop();
        } else {
            qualified_name_builder.push(segment);
        }
    }

    Some(qualified_name_builder.build())
}

pub(crate) fn resolve_module<'path>(
    db: &ModuleDb,
    module_name: &'path [&'path str],
) -> Option<Module> {
    let module_name = ModuleName::from_components(module_name.iter().copied())?;
    red_knot_python_semantic::resolve_module(db, module_name)
}
