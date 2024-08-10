use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::SourceRow;

/// ## What it does
/// Checks for import bindings that are shadowed by loop variables.
///
/// ## Why is this bad?
/// Shadowing an import with loop variables makes the code harder to read and
/// reason about, as the identify of the imported binding is no longer clear.
/// It's also often indicative of a mistake, as it's unlikely that the loop
/// variable is intended to be used as the imported binding.
///
/// Consider using a different name for the loop variable.
///
/// ## Example
/// ```python
/// from os import path
///
/// for path in files:
///     print(path)
/// ```
///
/// Use instead:
/// ```python
/// from os import path
///
///
/// for filename in files:
///     print(filename)
/// ```
#[violation]
pub struct ImportShadowedByLoopVar {
    pub(crate) name: String,
    pub(crate) row: SourceRow,
}

impl Violation for ImportShadowedByLoopVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportShadowedByLoopVar { name, row } = self;
        format!("Import `{name}` from {row} shadowed by loop variable")
    }
}

/// ## What it does
/// Checks for the use of wildcard imports.
///
/// ## Why is this bad?
/// Wildcard imports (e.g., `from module import *`) make it hard to determine
/// which symbols are available in the current namespace, and from which module
/// they were imported. They're also discouraged by [PEP 8].
///
/// ## Example
/// ```python
/// from math import *
///
///
/// def area(radius):
///     return pi * radius**2
/// ```
///
/// Use instead:
/// ```python
/// from math import pi
///
///
/// def area(radius):
///     return pi * radius**2
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[violation]
pub struct UndefinedLocalWithImportStar {
    pub(crate) name: String,
}

impl Violation for UndefinedLocalWithImportStar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocalWithImportStar { name } = self;
        format!("`from {name} import *` used; unable to detect undefined names")
    }
}

/// ## What it does
/// Checks for `__future__` imports that are not located at the beginning of a
/// file.
///
/// ## Why is this bad?
/// Imports from `__future__` must be placed the beginning of the file, before any
/// other statements (apart from docstrings). The use of `__future__` imports
/// elsewhere is invalid and will result in a `SyntaxError`.
///
/// ## Example
/// ```python
/// from pathlib import Path
///
/// from __future__ import annotations
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from pathlib import Path
/// ```
///
/// ## References
/// - [Python documentation: Future statements](https://docs.python.org/3/reference/simple_stmts.html#future)
#[violation]
pub struct LateFutureImport;

impl Violation for LateFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`from __future__` imports must occur at the beginning of the file")
    }
}

/// ## What it does
/// Checks for names that might be undefined, but may also be defined in a
/// wildcard import.
///
/// ## Why is this bad?
/// Wildcard imports (e.g., `from module import *`) make it hard to determine
/// which symbols are available in the current namespace. If a module contains
/// a wildcard import, and a name in the current namespace has not been
/// explicitly defined or imported, then it's unclear whether the name is
/// undefined or was imported by the wildcard import.
///
/// If the name _is_ defined in via a wildcard import, that member should be
/// imported explicitly to avoid confusion.
///
/// If the name is _not_ defined in a wildcard import, it should be defined or
/// imported.
///
/// ## Example
/// ```python
/// from math import *
///
///
/// def area(radius):
///     return pi * radius**2
/// ```
///
/// Use instead:
/// ```python
/// from math import pi
///
///
/// def area(radius):
///     return pi * radius**2
/// ```
#[violation]
pub struct UndefinedLocalWithImportStarUsage {
    pub(crate) name: String,
}

impl Violation for UndefinedLocalWithImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocalWithImportStarUsage { name } = self;
        format!("`{name}` may be undefined, or defined from star imports")
    }
}

/// ## What it does
/// Check for the use of wildcard imports outside of the module namespace.
///
/// ## Why is this bad?
/// The use of wildcard imports outside of the module namespace (e.g., within
/// functions) can lead to confusion, as the import can shadow local variables.
///
/// Though wildcard imports are discouraged by [PEP 8], when necessary, they
/// should be placed in the module namespace (i.e., at the top-level of a
/// module).
///
/// ## Example
///
/// ```python
/// def foo():
///     from math import *
/// ```
///
/// Use instead:
///
/// ```python
/// from math import *
///
///
/// def foo(): ...
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[violation]
pub struct UndefinedLocalWithNestedImportStarUsage {
    pub(crate) name: String,
}

impl Violation for UndefinedLocalWithNestedImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocalWithNestedImportStarUsage { name } = self;
        format!("`from {name} import *` only allowed at module level")
    }
}
