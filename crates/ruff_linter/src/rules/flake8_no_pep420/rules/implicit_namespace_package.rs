use std::path::{Path, PathBuf};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::PySourceType;
use ruff_python_ast::script::ScriptTag;
use ruff_python_trivia::CommentRanges;
use ruff_text_size::{TextRange, TextSize};

use crate::Locator;
use crate::Violation;
use crate::checkers::ast::LintContext;
use crate::comments::shebang::ShebangDirective;
use crate::fs;
use crate::package::PackageRoot;

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
/// package" (see: [PEP 420](https://peps.python.org/pep-0420/)).
/// Namespace packages are less widely used, so a package that lacks an
/// `__init__.py` file is typically meant to be a regular package, and
/// the absence of the `__init__.py` file is probably an oversight.
///
/// ## Options
/// - `namespace-packages`
#[derive(ViolationMetadata)]
pub(crate) struct ImplicitNamespacePackage {
    filename: String,
    parent: Option<String>,
}

impl Violation for ImplicitNamespacePackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImplicitNamespacePackage { filename, parent } = self;
        match parent {
            None => {
                format!(
                    "File `{filename}` is part of an implicit namespace package. Add an `__init__.py`."
                )
            }
            Some(parent) => {
                format!(
                    "File `{filename}` declares a package, but is nested under an implicit namespace package. Add an `__init__.py` to `{parent}`."
                )
            }
        }
    }
}

/// INP001
#[expect(clippy::too_many_arguments)]
pub(crate) fn implicit_namespace_package(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    project_root: &Path,
    src: &[PathBuf],
    allow_nested_roots: bool,
    context: &LintContext,
) {
    if package.is_none()
        // Ignore non-`.py` files, which don't require an `__init__.py`.
        && PySourceType::try_from_path(path).is_some_and(PySourceType::is_py_file)
        // Ignore any files that are direct children of the project root.
        && path
            .parent()
            .is_none_or( |parent| parent != project_root)
        // Ignore any files that are direct children of a source directory (e.g., `src/manage.py`).
        && !path
            .parent()
            .is_some_and( |parent| src.iter().any(|src| src == parent))
        // Ignore files that contain a shebang.
        && comment_ranges
            .first().filter(|range| range.start() == TextSize::from(0))
            .is_none_or(|range| ShebangDirective::try_extract(locator.slice(*range)).is_none())
        // Ignore PEP 723 scripts.
        && ScriptTag::parse(locator.contents().as_bytes()).is_none()
    {
        context.report_diagnostic(
            ImplicitNamespacePackage {
                filename: fs::relativize_path(path),
                parent: None,
            },
            TextRange::default(),
        );
    } else if allow_nested_roots {
        if let Some(PackageRoot::Nested { path: root }) = package.as_ref() {
            if path.ends_with("__init__.py") {
                // Identify the intermediary package that's missing the `__init__.py` file.
                if let Some(parent) = root
                    .ancestors()
                    .find(|parent| !parent.join("__init__.py").exists())
                {
                    context.report_diagnostic(
                        ImplicitNamespacePackage {
                            filename: fs::relativize_path(path),
                            parent: Some(fs::relativize_path(parent)),
                        },
                        TextRange::default(),
                    );
                }
            }
        }
    }
}
