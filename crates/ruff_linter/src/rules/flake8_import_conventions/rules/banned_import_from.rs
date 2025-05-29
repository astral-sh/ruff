use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::{Violation, checkers::ast::Checker};

/// ## What it does
/// Checks for member imports that should instead be accessed by importing the
/// module.
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for imports to make your code
/// more readable and idiomatic.
///
/// For example, it's common to import `pandas` as `pd`, and then access
/// members like `Series` via `pd.Series`, rather than importing `Series`
/// directly.
///
/// ## Example
/// ```python
/// from pandas import Series
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// pd.Series
/// ```
///
/// ## Options
/// - `lint.flake8-import-conventions.banned-from`
#[derive(ViolationMetadata)]
pub(crate) struct BannedImportFrom {
    name: String,
}

impl Violation for BannedImportFrom {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedImportFrom { name } = self;
        format!("Members of `{name}` should not be imported explicitly")
    }
}

/// ICN003
pub(crate) fn banned_import_from(
    checker: &Checker,
    stmt: &Stmt,
    name: &str,
    banned_conventions: &FxHashSet<String>,
) {
    if banned_conventions.contains(name) {
        checker.report_diagnostic(
            BannedImportFrom {
                name: name.to_string(),
            },
            stmt.range(),
        );
    }
}
