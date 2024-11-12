use ruff_python_ast::Alias;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic as semantic;
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
#[violation]
pub struct UselessImportAlias;

impl AlwaysFixableViolation for UselessImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Import alias does not rename original package".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove import alias".to_string()
    }
}

/// PLC0414
pub(crate) fn useless_import_alias(checker: &mut Checker, alias: &Alias) {
    let Some(asname) = &alias.asname else {
        return;
    };
    if alias.name.contains('.') {
        return;
    }
    if alias.name.as_str() != asname.as_str() {
        return;
    }
    // See https://github.com/astral-sh/ruff/issues/14283
    let required_imports = &checker.settings.isort.required_imports;
    if !required_imports.is_empty() {
        let semantic_alias = semantic::Alias {
            name: alias.name.as_str().to_owned(),
            as_name: Some(asname.as_str().to_owned()),
        };
        if required_imports.contains(&semantic::NameImport::Import(semantic::ModuleNameImport {
            name: semantic_alias,
        })) {
            let diagnostic = Diagnostic::new(UselessImportAlias, alias.range());
            checker.diagnostics.push(diagnostic);
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(UselessImportAlias, alias.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        asname.to_string(),
        alias.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// PLC0414
pub(crate) fn useless_importfrom_alias(
    checker: &mut Checker,
    alias: &Alias,
    module: Option<&str>,
    level: u32,
) {
    let Some(asname) = &alias.asname else {
        return;
    };
    if alias.name.contains('.') {
        return;
    }
    if alias.name.as_str() != asname.as_str() {
        return;
    }
    // See https://github.com/astral-sh/ruff/issues/14283
    let required_imports = &checker.settings.isort.required_imports;
    if !required_imports.is_empty() {
        let semantic_alias = semantic::Alias {
            name: alias.name.as_str().to_owned(),
            as_name: Some(asname.as_str().to_owned()),
        };
        if required_imports.contains(&semantic::NameImport::ImportFrom(
            semantic::MemberNameImport {
                name: semantic_alias,
                module: module.map(str::to_string),
                level,
            },
        )) {
            let diagnostic = Diagnostic::new(UselessImportAlias, alias.range());
            checker.diagnostics.push(diagnostic);
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(UselessImportAlias, alias.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        asname.to_string(),
        alias.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
