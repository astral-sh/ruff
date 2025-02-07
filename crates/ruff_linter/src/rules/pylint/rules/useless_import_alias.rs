use ruff_python_ast::Alias;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for import aliases that do not rename the original package.
///
/// ## Why is this bad?
/// The import alias is redundant and should be removed to avoid confusion.
///
/// ## Example
/// ```python
/// import numpy as numpy
/// ```
///
/// Use instead:
/// ```python
/// import numpy as np
/// ```
///
/// or
///
/// ```python
/// import numpy
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UselessImportAlias {
    required_import_conflict: bool,
}

impl Violation for UselessImportAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        #[allow(clippy::if_not_else)]
        if !self.required_import_conflict {
            "Import alias does not rename original package".to_string()
        } else {
            "Required import does not rename original package.".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        if self.required_import_conflict {
            Some("Change required import or disable rule.".to_string())
        } else {
            Some("Remove import alias".to_string())
        }
    }
}

/// PLC0414
pub(crate) fn useless_import_alias(checker: &Checker, alias: &Alias) {
    let Some(asname) = &alias.asname else {
        return;
    };
    if alias.name.as_str() != asname.as_str() {
        return;
    }
    // A required import with a useless alias causes an infinite loop.
    // See https://github.com/astral-sh/ruff/issues/14283
    let required_import_conflict = checker
        .settings
        .isort
        .requires_module_import(alias.name.to_string(), Some(asname.to_string()));
    let mut diagnostic = Diagnostic::new(
        UselessImportAlias {
            required_import_conflict,
        },
        alias.range(),
    );
    if !required_import_conflict {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            asname.to_string(),
            alias.range(),
        )));
    }

    checker.report_diagnostic(diagnostic);
}

/// PLC0414
pub(crate) fn useless_import_from_alias(
    checker: &Checker,
    alias: &Alias,
    module: Option<&str>,
    level: u32,
) {
    let Some(asname) = &alias.asname else {
        return;
    };
    if alias.name.as_str() != asname.as_str() {
        return;
    }
    // A required import with a useless alias causes an infinite loop.
    // See https://github.com/astral-sh/ruff/issues/14283
    let required_import_conflict = checker.settings.isort.requires_member_import(
        module.map(str::to_string),
        alias.name.to_string(),
        Some(asname.to_string()),
        level,
    );
    let mut diagnostic = Diagnostic::new(
        UselessImportAlias {
            required_import_conflict,
        },
        alias.range(),
    );

    if !required_import_conflict {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            asname.to_string(),
            alias.range(),
        )));
    }

    checker.report_diagnostic(diagnostic);
}
