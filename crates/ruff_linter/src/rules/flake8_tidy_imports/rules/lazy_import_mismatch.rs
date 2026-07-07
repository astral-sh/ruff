use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{AtomicNodeIndex, PythonVersion, Stmt, StmtImport, StmtImportFrom};
use ruff_python_trivia::indentation_at_offset;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_tidy_imports::matchers::{MatchNameOrParent, NameMatchPolicy};
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
/// - `lint.flake8-tidy-imports.require-lazy`
/// - `lint.flake8-tidy-imports.ban-lazy`
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.6")]
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

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
    let Some(policy) = lazy_import_policy(checker, stmt) else {
        return;
    };

    let selector = match policy {
        LazyImportPolicy::RequireLazy => &checker.settings().flake8_tidy_imports.require_lazy,
        LazyImportPolicy::BanLazy => &checker.settings().flake8_tidy_imports.ban_lazy,
    };

    if selector.includes_all() {
        report_all_matching_imports(checker, stmt, policy, selector);
        return;
    }

    for (import_policy, node) in &BannedModuleImportPolicies::new(stmt, checker) {
        if let Some(m) = selector.find(&import_policy) {
            report_lazy_import_policy(checker, stmt, node.range(), m.name(), policy, selector);
        }
    }
}

fn lazy_import_policy(checker: &Checker, stmt: &Stmt) -> Option<LazyImportPolicy> {
    if checker.target_version() < PythonVersion::PY315 || checker.lazy_import_context().is_some() {
        return None;
    }

    match stmt {
        Stmt::Import(StmtImport { is_lazy, .. }) => Some(if *is_lazy {
            LazyImportPolicy::BanLazy
        } else {
            LazyImportPolicy::RequireLazy
        }),
        Stmt::ImportFrom(StmtImportFrom {
            module,
            names,
            is_lazy,
            ..
        }) => {
            if matches!(module.as_deref(), Some("__future__"))
                || names.iter().any(|alias| alias.name.as_str() == "*")
            {
                None
            } else {
                Some(if *is_lazy {
                    LazyImportPolicy::BanLazy
                } else {
                    LazyImportPolicy::RequireLazy
                })
            }
        }
        _ => None,
    }
}

fn report_all_matching_imports(
    checker: &Checker,
    stmt: &Stmt,
    policy: LazyImportPolicy,
    selector: &crate::rules::flake8_tidy_imports::settings::ImportSelector,
) {
    match stmt {
        Stmt::Import(_) => {
            for (import_policy, node) in &BannedModuleImportPolicies::new(stmt, checker) {
                if let Some(m) = selector.find(&import_policy) {
                    report_lazy_import_policy(
                        checker,
                        stmt,
                        node.range(),
                        m.name(),
                        policy,
                        selector,
                    );
                }
            }
        }
        Stmt::ImportFrom(_) => {
            for (import_policy, node) in &BannedModuleImportPolicies::new(stmt, checker) {
                if !node.is_alias() && selector.find(&import_policy).is_some() {
                    report_lazy_import_policy(checker, stmt, node.range(), None, policy, selector);
                    break;
                }
            }
        }
        _ => {}
    }
}

fn report_lazy_import_policy(
    checker: &Checker,
    stmt: &Stmt,
    range: ruff_text_size::TextRange,
    name: Option<String>,
    policy: LazyImportPolicy,
    selector: &crate::rules::flake8_tidy_imports::settings::ImportSelector,
) {
    let mut diagnostic = checker.report_diagnostic(LazyImportMismatch { policy, name }, range);
    if let Stmt::Import(import) = stmt
        && let Some(fix) = split_import_fix(checker, stmt, import, policy, selector)
    {
        diagnostic.set_fix(fix);
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

fn split_import_fix(
    checker: &Checker,
    stmt: &Stmt,
    import: &StmtImport,
    policy: LazyImportPolicy,
    selector: &crate::rules::flake8_tidy_imports::settings::ImportSelector,
) -> Option<Fix> {
    if import.names.len() < 2 {
        return None;
    }

    let mut matching = Vec::new();
    let mut non_matching = Vec::new();
    for alias in &import.names {
        let import_policy = NameMatchPolicy::MatchNameOrParent(MatchNameOrParent {
            module: &alias.name,
        });
        if selector.find(&import_policy).is_some() {
            matching.push(alias.clone());
        } else {
            non_matching.push(alias.clone());
        }
    }

    if matching.is_empty() || non_matching.is_empty() {
        return None;
    }

    let matching_is_lazy = matches!(policy, LazyImportPolicy::RequireLazy);
    let matching_stmt = import_stmt(matching, matching_is_lazy);
    let non_matching_stmt = import_stmt(non_matching, !matching_is_lazy);

    let first_alias = import.names.first()?;
    let first_matches = selector
        .find(&NameMatchPolicy::MatchNameOrParent(MatchNameOrParent {
            module: &first_alias.name,
        }))
        .is_some();
    let (first, second) = if first_matches {
        (&matching_stmt, &non_matching_stmt)
    } else {
        (&non_matching_stmt, &matching_stmt)
    };

    let indentation = indentation_at_offset(stmt.start(), checker.source()).unwrap_or_default();
    let line_ending = checker.stylist().line_ending().as_str();
    let replacement = format!(
        "{}{line_ending}{indentation}{}",
        checker.generator().stmt(&Stmt::Import(first.clone())),
        checker.generator().stmt(&Stmt::Import(second.clone()))
    );

    Some(Fix::unsafe_edit(Edit::range_replacement(
        replacement,
        stmt.range(),
    )))
}

fn import_stmt(names: Vec<ruff_python_ast::Alias>, is_lazy: bool) -> StmtImport {
    StmtImport {
        node_index: AtomicNodeIndex::NONE,
        range: TextRange::default(),
        names,
        is_lazy,
    }
}
