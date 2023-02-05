use std::path::{Path, PathBuf};

use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use crate::{define_violation, fs};

define_violation!(
    pub struct ImplicitNamespacePackage(pub String);
);
impl Violation for ImplicitNamespacePackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImplicitNamespacePackage(filename) = self;
        format!("File `{filename}` is part of an implicit namespace package. Add an `__init__.py`.")
    }
}

/// INP001
pub fn implicit_namespace_package(
    path: &Path,
    package: Option<&Path>,
    project_root: &Path,
    src: &[PathBuf],
) -> Option<Diagnostic> {
    if package.is_none()
        // Ignore `.pyi` files, which don't require an `__init__.py`.
        && path.extension().map_or(true, |ext| ext != "pyi")
        // Ignore any files that are direct children of the project root.
        && !path
            .parent()
            .map_or(false, |parent| parent == project_root)
        // Ignore any files that are direct children of a source directory (e.g., `src/manage.py`).
        && !path
            .parent()
            .map_or(false, |parent| src.iter().any(|src| src == parent))
    {
        #[cfg(all(test, windows))]
        let path = path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"); // The snapshot test expects / as the path separator.
        Some(Diagnostic::new(
            ImplicitNamespacePackage(fs::relativize_path(path)),
            Range::default(),
        ))
    } else {
        None
    }
}
