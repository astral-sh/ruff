use itertools::Itertools;
use rustpython_parser::ast::{Alias, Ranged};

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AutofixKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::OneIndexed;
use ruff_python_stdlib::future::ALL_FEATURE_NAMES;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum UnusedImportContext {
    ExceptHandler,
    Init,
}

/// ## What it does
/// Checks for unused imports.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles. They also increase the cognitive load of reading the code.
///
/// If an import statement is used to check for the availability or existence
/// of a module, consider using `importlib.util.find_spec` instead.
///
/// ## Options
///
/// - `pyflakes.extend-generics`
///
/// ## Example
/// ```python
/// import numpy as np  # unused import
///
///
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// Use instead:
/// ```python
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// To check the availability of a module, use `importlib.util.find_spec`:
/// ```python
/// from importlib.util import find_spec
///
/// if find_spec("numpy") is not None:
///     print("numpy is installed")
/// else:
///     print("numpy is not installed")
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation](https://docs.python.org/3/library/importlib.html#importlib.util.find_spec)
#[violation]
pub struct UnusedImport {
    pub(crate) name: String,
    pub(crate) context: Option<UnusedImportContext>,
    pub(crate) multiple: bool,
}

impl Violation for UnusedImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedImport { name, context, .. } = self;
        match context {
            Some(UnusedImportContext::ExceptHandler) => {
                format!(
                    "`{name}` imported but unused; consider using `importlib.util.find_spec` to test for availability"
                )
            }
            Some(UnusedImportContext::Init) => {
                format!(
                    "`{name}` imported but unused; consider adding to `__all__` or using a redundant \
                     alias"
                )
            }
            None => format!("`{name}` imported but unused"),
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let UnusedImport { name, multiple, .. } = self;

        Some(if *multiple {
            "Remove unused import".to_string()
        } else {
            format!("Remove unused import: `{name}`")
        })
    }
}

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
    pub(crate) line: OneIndexed,
}

impl Violation for ImportShadowedByLoopVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportShadowedByLoopVar { name, line } = self;
        format!("Import `{name}` from line {line} shadowed by loop variable")
    }
}

/// ## What it does
/// Checks for the use of wildcard imports.
///
/// ## Why is this bad?
/// Wildcard imports (e.g., `from module import *`) make it hard to determine
/// which symbols are available in the current namespace, and from which module
/// they were imported.
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
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#imports)
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
/// - [PEP 8](https://peps.python.org/pep-0008/#module-level-dunder-names)
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
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#imports)
#[violation]
pub struct UndefinedLocalWithImportStarUsage {
    pub(crate) name: String,
    pub(crate) sources: Vec<String>,
}

impl Violation for UndefinedLocalWithImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocalWithImportStarUsage { name, sources } = self;
        let sources = sources
            .iter()
            .map(|source| format!("`{source}`"))
            .join(", ");
        format!("`{name}` may be undefined, or defined from star imports: {sources}")
    }
}

/// ## What it does
/// Check for the use of wildcard imports outside of the module namespace.
///
/// ## Why is this bad?
/// The use of wildcard imports outside of the module namespace (e.g., within
/// functions) can lead to confusion, as the import can shadow local variables.
///
/// Though wildcard imports are discouraged, when necessary, they should be placed
/// in the module namespace (i.e., at the top-level of a module).
///
/// ## Example
/// ```python
/// def foo():
///     from math import *
/// ```
///
/// Use instead:
/// ```python
/// from math import *
///
///
/// def foo():
///     ...
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#imports)
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

/// ## What it does
/// Checks for `__future__` imports that are not defined in the current Python
/// version.
///
/// ## Why is this bad?
/// Importing undefined or unsupported members from the `__future__` module is
/// a `SyntaxError`.
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/__future__.html)
#[violation]
pub struct FutureFeatureNotDefined {
    name: String,
}

impl Violation for FutureFeatureNotDefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureFeatureNotDefined { name } = self;
        format!("Future feature `{name}` is not defined")
    }
}

pub(crate) fn future_feature_not_defined(checker: &mut Checker, alias: &Alias) {
    if !ALL_FEATURE_NAMES.contains(&alias.name.as_str()) {
        checker.diagnostics.push(Diagnostic::new(
            FutureFeatureNotDefined {
                name: alias.name.to_string(),
            },
            alias.range(),
        ));
    }
}
