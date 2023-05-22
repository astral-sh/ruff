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
/// Checks for imports that are never used.
///
/// ## Why is this bad?
/// Unused imports confuse the reader and may impact performance. They should
/// be removed.
///
/// If the import is used to check for availability of a module, consider using
/// `importlib.util.find_spec` instead.
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
/// ## Example (checking for availability of a module)
/// ```python
/// try:
///     import numpy as np
/// except ImportError:
///     print("numpy is not installed")
/// else:
///     print("numpy is installed")
/// ```
///
/// Use instead:
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
/// Checks for imports that are shadowed by a loop variable.
///
/// ## Why is this bad?
/// Shadowing imports with loop variables makes the code harder to read.
///
/// Consider using a different name for the loop variable.
///
/// ## Example
/// ```python
/// import email
///
///
/// for email in emails:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import email
///
///
/// for e in emails:
///     ...
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation](https://docs.python.org/3/reference/compound_stmts.html#the-for-statement)
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
/// Checks for `from module import *` statements.
///
/// ## Why is this bad?
/// Wildcard imports makes it hard to determine from where names are imported.
/// They also pollute the namespace, making it harder to determine which names
/// are defined in the current module.
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
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation](https://docs.python.org/3/tutorial/modules.html#importing-from-a-package)
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
/// Checks for `__future__` imports that are not at the beginning of the file.
///
/// ## Why is this bad?
/// Imports from `__future__` must be at the beginning of the file before any
/// other code, except docstrings.
///
/// ## Example
/// ```python
/// """Example docstring."""
///
///
/// from pathlib import Path
///
/// from __future__ import annotations
/// ```
///
/// Use instead:
/// ```python
/// """Example docstring."""
///
///
/// from __future__ import annotations
///
/// from pathlib import Path
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#module-level-dunder-names)
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#future-statements)
/// - [Python documentation](https://docs.python.org/3/library/__future__.html)
#[violation]
pub struct LateFutureImport;

impl Violation for LateFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`from __future__` imports must occur at the beginning of the file")
    }
}

/// ## What it does
/// Checks for names that could be undefined but could be defined in a wildcard
/// import.
///
/// ## Why is this bad?
/// If the name is defined in a wildcard import, it should be imported
/// explicitly to avoid confusion.
///
/// If the name is not defined in a wildcard import, it should be defined or
/// imported.
///
/// ## Example (name in wildcard import)
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
/// ## Example (name undefined)
/// ```python
/// from math import *
///
///
/// def weight_on_earth(mass):
///     return mass * G  # G is undefined
/// ```
///
/// Use instead:
/// ```python
/// from math import *
///
/// G = 9.8
///
///
/// def weight_on_earth(mass):
///     return mass * G
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#imports)
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation](https://docs.python.org/3/tutorial/modules.html#importing-from-a-package)
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
/// Check for wildcard imports not in the module-level namespace.
///
/// ## Why is this bad?
/// Wildcard imports at non-module level are confusing and can shadow names
/// imported in the module-level namespace.
///
/// If you are going to use wildcard imports, they should be at the module level
/// and not nested inside functions or classes.
///
/// ## Example
/// ```python
/// def foo():
///     from math import *
///     ...
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
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation](https://docs.python.org/3/tutorial/modules.html#importing-from-a-package)
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
/// Checks for `__future__` imports that are not defined.
///
/// ## Why is this bad?
/// `__future__` imports must be defined in the Python version you are using.
///
/// To fix this, check the Python version you are using and make sure the
/// `__future__` import is defined in that version. If it is not, remove the
/// import.
///
/// ## Example
/// ```python
/// # using Python <3.7
/// from __future__ import annotations
/// ```
///
/// Use instead:
/// ```python
/// # using Python >=3.7
/// from __future__ import annotations
/// ```
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
