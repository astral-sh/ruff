use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Alias, AtomicNodeIndex, PythonVersion, Stmt, StmtImport, StmtImportFrom};
use ruff_python_trivia::{indentation_at_offset, is_python_whitespace};
use ruff_source_file::LineRanges;
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
    if let Stmt::Import(import) = stmt {
        match split_import_fix(checker, stmt, import, policy, selector) {
            SplitImportFix::Fix(fix) => {
                diagnostic.set_fix(fix);
                return;
            }
            SplitImportFix::Unavailable => return,
            SplitImportFix::NotMixed => {}
        }
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

enum SplitImportFix {
    Fix(Fix),
    NotMixed,
    Unavailable,
}

fn split_import_fix(
    checker: &Checker,
    stmt: &Stmt,
    import: &StmtImport,
    policy: LazyImportPolicy,
    selector: &crate::rules::flake8_tidy_imports::settings::ImportSelector,
) -> SplitImportFix {
    if import.names.len() < 2 {
        return SplitImportFix::NotMixed;
    }

    let matching_is_lazy = matches!(policy, LazyImportPolicy::RequireLazy);
    let mut has_matching = false;
    let mut has_non_matching = false;
    let mut runs: Vec<StmtImport> = Vec::new();
    let mut current_names = Vec::new();
    let mut current_is_lazy = None;
    let mut bound_names = Vec::new();

    for alias in &import.names {
        let import_policy = NameMatchPolicy::MatchNameOrParent(MatchNameOrParent {
            module: &alias.name,
        });
        let is_match = selector.find(&import_policy).is_some();
        if is_match {
            has_matching = true;
        } else {
            has_non_matching = true;
        }
        let is_lazy = if is_match {
            matching_is_lazy
        } else {
            !matching_is_lazy
        };
        let bound_name = import_alias_bound_name(alias);
        if bound_names
            .iter()
            .any(|(name, previous_is_lazy)| *name == bound_name && *previous_is_lazy != is_lazy)
        {
            return SplitImportFix::Unavailable;
        }
        bound_names.push((bound_name, is_lazy));

        if current_is_lazy == Some(is_lazy) {
            current_names.push(alias.clone());
        } else {
            if let Some(previous_is_lazy) = current_is_lazy {
                runs.push(import_stmt(
                    std::mem::take(&mut current_names),
                    previous_is_lazy,
                ));
            }
            current_is_lazy = Some(is_lazy);
            current_names.push(alias.clone());
        }
    }

    if !has_matching || !has_non_matching {
        return SplitImportFix::NotMixed;
    }

    if checker
        .indexer()
        .preceded_by_multi_statement_line(stmt, checker.source())
        || has_trailing_comment_or_content(stmt.end(), checker.source())
    {
        return SplitImportFix::Unavailable;
    }

    if let Some(is_lazy) = current_is_lazy {
        runs.push(import_stmt(current_names, is_lazy));
    }

    let indentation = indentation_at_offset(stmt.start(), checker.source()).unwrap_or_default();
    let line_ending = checker.stylist().line_ending().as_str();
    let mut replacement = String::new();
    for (index, run) in runs.into_iter().enumerate() {
        if index > 0 {
            replacement.push_str(line_ending);
            replacement.push_str(indentation);
        }
        replacement.push_str(&checker.generator().stmt(&Stmt::Import(run)));
    }

    SplitImportFix::Fix(Fix::unsafe_edit(Edit::range_replacement(
        replacement,
        stmt.range(),
    )))
}

fn has_trailing_comment_or_content(offset: TextSize, source: &str) -> bool {
    let line_end = source.line_end(offset);
    source[TextRange::new(offset, line_end)]
        .chars()
        .any(|char| !is_python_whitespace(char))
}

fn import_alias_bound_name(alias: &Alias) -> &str {
    alias.asname.as_deref().unwrap_or_else(|| {
        alias
            .name
            .as_str()
            .split_once('.')
            .map_or(alias.name.as_str(), |(name, _)| name)
    })
}

fn import_stmt(names: Vec<Alias>, is_lazy: bool) -> StmtImport {
    StmtImport {
        node_index: AtomicNodeIndex::NONE,
        range: TextRange::default(),
        names,
        is_lazy,
    }
}
