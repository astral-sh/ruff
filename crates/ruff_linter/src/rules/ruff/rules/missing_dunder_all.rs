use std::path::{Path, PathBuf};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_parser::parse_module;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for wildcard imports (`from .module import *`) inside `__init__.py`
/// files where the imported local module does not define `__all__`.
///
/// ## Why is this bad?
/// When a package's `__init__.py` uses `from .module import *`, every public
/// name in `module` is re-exported — unless `module` defines `__all__`.
/// Without `__all__`, removing or adding names in `module` silently changes
/// the package's public API.
///
/// Explicitly defining `__all__` in the imported module makes the public API
/// intentional and reviewable, and prevents accidental re-exports.
///
/// ## Example
/// ```python
/// # connected.py — missing __all__
/// class ConnectedRepository: ...
/// ```
///
/// ```python
/// # __init__.py
/// from .connected import *  # unclear what is exported
/// ```
///
/// Use instead:
/// ```python
/// # connected.py
/// __all__ = ["ConnectedRepository"]
///
/// class ConnectedRepository: ...
/// ```
///
/// ```python
/// # __init__.py
/// from .connected import *  # safe — __all__ controls what is exported
/// ```
///
/// ## References
/// - [Python documentation: `__all__`](https://docs.python.org/3/tutorial/modules.html#importing-from-a-package)
/// - [PEP 8 – Imports](https://peps.python.org/pep-0008/#imports)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.16.0")]
pub(crate) struct MissingDunderAll {
    module: String,
}

impl Violation for MissingDunderAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingDunderAll { module } = self;
        format!(
            "`{module}` is missing `__all__`; \
             `from {module} import *` re-exports every public name"
        )
    }
}

/// RUF074
pub(crate) fn missing_dunder_all(
    checker: &Checker,
    import_from: &ast::StmtImportFrom,
) {
    // Only fire inside `__init__.py`.
    if !checker.in_init_module() {
        return;
    }

    let ast::StmtImportFrom {
        names,
        module,
        level,
        ..
    } = import_from;

    // Only relative imports (level > 0) — i.e. `from .X import *`.
    // Absolute wildcard imports are already flagged by F403.
    if *level == 0 {
        return;
    }

    // We need a module name to resolve a file. `from . import *` (bare
    // relative, no module name) is unusual; skip it.
    let Some(module_name) = module.as_deref() else {
        return;
    };

    for alias in names {
        if &alias.name != "*" {
            continue;
        }

        // Format the dotted name for display (e.g. `.connected`, `..sibling`).
        let formatted = format!("{}{}", ".".repeat(*level as usize), module_name);

        // Try to locate the source file for the imported module.
        // If we can't find it (external, compiled, etc.) stay silent.
        let Some(module_path) =
            resolve_module_path(checker.path(), *level, module_name)
        else {
            continue;
        };

        // Read and parse the target file, then check for a top-level `__all__`
        // assignment.  If parsing fails or `__all__` is present, do nothing.
        if source_defines_dunder_all(&module_path).unwrap_or(true) {
            continue;
        }

        checker.report_diagnostic(
            MissingDunderAll { module: formatted },
            alias.range(),
        );
    }
}

/// Resolve a relative import to its source file path.
///
/// Given the path of the current `__init__.py`, the import level, and the
/// module name (e.g. `level=1, module="connected"` for `from .connected`),
/// returns the path to `connected.py` or `connected/__init__.py` if either
/// exists on disk.
fn resolve_module_path(init_path: &Path, level: u32, module_name: &str) -> Option<PathBuf> {
    // The package directory is the parent of `__init__.py`.
    let mut base = init_path.parent()?.to_path_buf();

    // A level of 1 means the current package.  Each additional level goes up
    // one directory.
    for _ in 1..level {
        base = base.parent()?.to_path_buf();
    }

    // Support dotted names like `from .a.b import *` → `a/b.py`.
    for component in module_name.split('.') {
        base.push(component);
    }

    // Try `<base>.py` first, then `<base>/__init__.py`.
    let as_file = base.with_extension("py");
    if as_file.exists() {
        return Some(as_file);
    }

    let as_package = base.join("__init__.py");
    if as_package.exists() {
        return Some(as_package);
    }

    None
}

/// Return `true` if the file at `path` contains a top-level `__all__`
/// assignment, `false` if it does not, or `None` if the file cannot be
/// read or parsed.
fn source_defines_dunder_all(path: &Path) -> Option<bool> {
    let source = std::fs::read_to_string(path).ok()?;
    let parsed = parse_module(&source).ok()?;

    for stmt in parsed.suite() {
        let targets: Vec<&Expr> = match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                targets.iter().collect()
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                vec![target]
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                vec![target]
            }
            _ => continue,
        };

        for target in targets {
            if let Expr::Name(ast::ExprName { id, .. }) = target {
                if id == "__all__" {
                    return Some(true);
                }
            }
        }
    }

    Some(false)
}
