use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{PythonVersion, Stmt};
use ruff_python_semantic::ImportLaziness;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_tidy_imports::rules::BannedModuleImportPolicies;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Enforces the configured lazy-import policy in contexts where `lazy import`
/// is legal.
///
/// ## Why is this bad?
/// Python 3.15 adds support for `lazy import` and `lazy from ... import ...`,
/// which defer the actual import work until the imported name is first used.
///
/// Depending on the policy, some modules should be imported lazily to defer
/// import work until the name is first used, while others should remain eager
/// to preserve import-time side effects.
///
/// The rule also recognizes imports made lazy by a literal `__lazy_modules__`
/// declaration, including when the target version is older than Python 3.15.
/// This declaration allows a module to use lazy imports on Python 3.15 and
/// later while retaining eager imports on older versions. Dynamic assignments
/// to `__lazy_modules__` (e.g. `__lazy_modules__ = non_literal()`) are ignored,
/// as their effects cannot be determined statically.
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
/// ## Fix availability
///
/// The fix is only available for statements that import a single name, since
/// changing `lazy` on a multi-member import could violate another name's policy.
///
/// The fix is also unavailable for imports marked lazy by a `__lazy_modules__`
/// declaration.
///
/// ## Fix safety
///
/// This rule's fix is marked as unsafe because changing when a module is
/// imported can affect runtime behavior, including import-time side effects.
///
/// ## Options
/// - `lint.flake8-tidy-imports.require-lazy`
/// - `lint.flake8-tidy-imports.ban-lazy`
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.6", category = Category::Restriction)]
pub(crate) struct LazyImportMismatch {
    policy: LazyImportPolicy,
    name: Option<String>,
}

#[derive(Debug, Copy, Clone)]
enum LazyImportPolicy {
    RequireLazy,
    BanLazy,
}

impl Violation for LazyImportMismatch {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match (self.policy, &self.name) {
            (LazyImportPolicy::RequireLazy, Some(name)) => {
                format!("`{name}` should be imported lazily")
            }
            (LazyImportPolicy::RequireLazy, None) => {
                "Use a `lazy` import instead of an eager import".to_string()
            }
            (LazyImportPolicy::BanLazy, Some(name)) => {
                format!("`{name}` should be imported eagerly")
            }
            (LazyImportPolicy::BanLazy, None) => {
                "Use an eager import instead of a `lazy` import".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(match self.policy {
            LazyImportPolicy::RequireLazy => "Convert to a lazy import".to_string(),
            LazyImportPolicy::BanLazy => "Convert to an eager import".to_string(),
        })
    }
}

/// TID254
pub(crate) fn lazy_import_mismatch(checker: &Checker, stmt: &Stmt) {
    if checker.lazy_import_context().is_some()
        || (checker.target_version() < PythonVersion::PY315
            && checker.semantic().lazy_modules.is_none())
    {
        return;
    }
    let names = match stmt {
        Stmt::Import(import) => &import.names,
        Stmt::ImportFrom(import)
            if import.module.as_deref() != Some("__future__")
                && !import.names.iter().any(|alias| alias.name.as_str() == "*") =>
        {
            &import.names
        }
        _ => return,
    };
    for (import_policy, node) in &BannedModuleImportPolicies::new(stmt, checker) {
        let Some(alias) = node.as_alias().copied().or_else(|| names.first()) else {
            continue;
        };
        let policy = match checker.semantic().import_laziness(stmt, alias) {
            ImportLaziness::Lazy => LazyImportPolicy::BanLazy,
            ImportLaziness::Eager => LazyImportPolicy::RequireLazy,
            ImportLaziness::Unknown => continue,
        };
        let selector = match policy {
            LazyImportPolicy::RequireLazy => &checker.settings().flake8_tidy_imports.require_lazy,
            LazyImportPolicy::BanLazy => &checker.settings().flake8_tidy_imports.ban_lazy,
        };
        if selector.includes_all() && stmt.is_import_from_stmt() && node.is_alias() {
            continue;
        }
        if let Some(m) = selector.find(&import_policy) {
            report_lazy_import_policy(checker, stmt, node.range(), m.name(), policy);
        }
    }
}

fn report_lazy_import_policy(
    checker: &Checker,
    stmt: &Stmt,
    range: ruff_text_size::TextRange,
    name: Option<String>,
    policy: LazyImportPolicy,
) {
    let mut diagnostic = checker.report_diagnostic(LazyImportMismatch { policy, name }, range);
    let names = match stmt {
        Stmt::Import(import) => &import.names,
        Stmt::ImportFrom(import) => &import.names,
        _ => return,
    };
    // Changing the entire statement could violate another imported name's policy.
    if names.len() != 1 || checker.semantic().lazy_modules.is_some() {
        return;
    }
    match policy {
        LazyImportPolicy::RequireLazy => {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                "lazy ".to_string(),
                stmt.start(),
            )));
        }
        LazyImportPolicy::BanLazy => {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::at(
                stmt.start(),
                TextSize::from(5),
            ))));
        }
    }
}
