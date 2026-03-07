use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{PythonVersion, Stmt, StmtImport, StmtImportFrom};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_tidy_imports::rules::BannedModuleImportPolicies;
use crate::rules::flake8_tidy_imports::settings::{
    AllImports, BannedEagerImports as BannedEagerImportsSetting,
};
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for eager imports in contexts where `lazy import` is legal.
///
/// ## Why is this bad?
/// Python 3.15 adds support for `lazy import` and `lazy from ... import ...`,
/// which defer the actual import work until the imported name is first used.
///
/// When a module should be loaded lazily, using an eager import defeats that
/// intent by importing it immediately instead.
///
/// This rule ignores contexts in which `lazy import` is invalid, such as
/// functions, classes, `try`/`except` blocks, `__future__` imports, and
/// `from ... import *` statements.
///
/// ## Example
/// ```python
/// import typing
/// ```
///
/// Use instead:
/// ```python
/// lazy import typing
/// ```
///
/// ## Options
/// - `lint.flake8-tidy-imports.banned-eager-imports`
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.5")]
pub(crate) struct BannedEagerImports {
    name: Option<String>,
}

impl Violation for BannedEagerImports {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedEagerImports { name } = self;
        match name {
            Some(name) => format!("`{name}` should be imported lazily"),
            None => "Use a `lazy` import instead of an eager import".to_string(),
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to a lazy import".to_string())
    }
}

/// TID254
pub(crate) fn banned_eager_imports(checker: &Checker, stmt: &Stmt) {
    if !is_convertible_to_lazy_import(checker, stmt) {
        return;
    }

    match &checker.settings().flake8_tidy_imports.banned_eager_imports {
        BannedEagerImportsSetting::All(AllImports::All) => report_all_banned_imports(checker, stmt),
        BannedEagerImportsSetting::Imports(imports) => {
            if imports.is_empty() {
                return;
            }

            for (policy, node) in &BannedModuleImportPolicies::new(stmt, checker) {
                if let Some(banned_import) = policy.find(imports.iter().map(String::as_str)) {
                    report_banned_eager_import(checker, stmt, node.range(), Some(banned_import));
                }
            }
        }
    }
}

fn is_convertible_to_lazy_import(checker: &Checker, stmt: &Stmt) -> bool {
    if checker.target_version() < PythonVersion::PY315 || checker.lazy_import_context().is_some() {
        return false;
    }

    match stmt {
        Stmt::Import(StmtImport { is_lazy, .. }) => !*is_lazy,
        Stmt::ImportFrom(StmtImportFrom {
            module,
            names,
            is_lazy,
            ..
        }) => {
            !*is_lazy
                && !matches!(module.as_deref(), Some("__future__"))
                && !names.iter().any(|alias| alias.name.as_str() == "*")
        }
        _ => false,
    }
}

fn report_all_banned_imports(checker: &Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(_) => {
            for (_, node) in &BannedModuleImportPolicies::new(stmt, checker) {
                report_banned_eager_import(checker, stmt, node.range(), None);
            }
        }
        Stmt::ImportFrom(_) => {
            for (_, node) in &BannedModuleImportPolicies::new(stmt, checker) {
                if !node.is_alias() {
                    report_banned_eager_import(checker, stmt, node.range(), None);
                    break;
                }
            }
        }
        _ => {}
    }
}

fn report_banned_eager_import(
    checker: &Checker,
    stmt: &Stmt,
    range: ruff_text_size::TextRange,
    name: Option<String>,
) {
    let mut diagnostic = checker.report_diagnostic(BannedEagerImports { name }, range);
    diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
        "lazy ".to_string(),
        stmt.start(),
    )));
}
