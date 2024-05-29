use std::path::{Path, PathBuf};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;
use ruff_text_size::{TextRange, TextSize};

use crate::comments::shebang::ShebangDirective;
use crate::fs;

/// ## What it does
/// Checks for packages that are missing an `__init__.py` file.
///
/// ## Why is this bad?
/// Python packages are directories that contain a file named `__init__.py`.
/// The existence of this file indicates that the directory is a Python
/// package, and so it can be imported the same way a module can be
/// imported.
///
/// Directories that lack an `__init__.py` file can still be imported, but
/// they're indicative of a special kind of package, known as a "namespace
/// package" (see: [PEP 420](https://www.python.org/dev/peps/pep-0420/)).
/// Namespace packages are less widely used, so a package that lacks an
/// `__init__.py` file is typically meant to be a regular package, and
/// the absence of the `__init__.py` file is probably an oversight.
///
/// ## Options
/// - `namespace-packages`
#[violation]
pub struct ImplicitNamespacePackage {
    filename: String,
}

impl Violation for ImplicitNamespacePackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImplicitNamespacePackage { filename } = self;
        format!("File `{filename}` is part of an implicit namespace package. Add an `__init__.py`.")
    }
}

/// INP001
pub(crate) fn implicit_namespace_package(
    path: &Path,
    package: Option<&Path>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    project_root: &Path,
    src: &[PathBuf],
) -> Option<Diagnostic> {
    if package.is_none()
        // Ignore non-`.py` files, which don't require an `__init__.py`.
        && path.extension().is_some_and( |ext| ext == "py")
        // Ignore any files that are direct children of the project root.
        && !path
            .parent()
            .is_some_and( |parent| parent == project_root)
        // Ignore any files that are direct children of a source directory (e.g., `src/manage.py`).
        && !path
            .parent()
            .is_some_and( |parent| src.iter().any(|src| src == parent))
        // Ignore files that contain a shebang.
        && !comment_ranges
            .first().filter(|range| range.start() == TextSize::from(0))
            .is_some_and(|range| ShebangDirective::try_extract(locator.slice(*range)).is_some())
    {
        #[cfg(all(test, windows))]
        let path = path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"); // The snapshot test expects / as the path separator.
        Some(Diagnostic::new(
            ImplicitNamespacePackage {
                filename: fs::relativize_path(path),
            },
            TextRange::default(),
        ))
    } else {
        None
    }
}
