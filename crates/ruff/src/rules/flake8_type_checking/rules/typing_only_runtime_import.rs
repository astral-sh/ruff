use anyhow::Result;
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{AutofixKind, Diagnostic, DiagnosticKind, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Binding, NodeId, ReferenceId, Scope};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::importer::StmtImports;
use crate::rules::isort::{categorize, ImportSection, ImportType};

/// ## What it does
/// Checks for first-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import A
///
///
/// def foo(a: A) -> int:
///     return len(a)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import A
///
///
/// def foo(a: A) -> int:
///     return len(a)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyFirstPartyImport {
    qualified_name: String,
}

impl Violation for TypingOnlyFirstPartyImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move application import `{}` into a type-checking block",
            self.qualified_name
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Move into type-checking block".to_string())
    }
}

/// ## What it does
/// Checks for third-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import pandas as pd
///
///
/// def foo(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import pandas as pd
///
///
/// def foo(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyThirdPartyImport {
    qualified_name: String,
}

impl Violation for TypingOnlyThirdPartyImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move third-party import `{}` into a type-checking block",
            self.qualified_name
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Move into type-checking block".to_string())
    }
}

/// ## What it does
/// Checks for standard library imports that are only used for type
/// annotations, but aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// from pathlib import Path
///
///
/// def foo(path: Path) -> str:
///     return str(path)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     from pathlib import Path
///
///
/// def foo(path: Path) -> str:
///     return str(path)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyStandardLibraryImport {
    qualified_name: String,
}

impl Violation for TypingOnlyStandardLibraryImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move standard library import `{}` into a type-checking block",
            self.qualified_name
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Move into type-checking block".to_string())
    }
}

/// TCH001, TCH002, TCH003
pub(crate) fn typing_only_runtime_import(
    checker: &Checker,
    scope: &Scope,
    runtime_imports: &[&Binding],
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Collect all typing-only imports by statement and import type.
    let mut errors_by_statement: FxHashMap<(NodeId, ImportType), Vec<Import>> =
        FxHashMap::default();
    let mut ignores_by_statement: FxHashMap<(NodeId, ImportType), Vec<Import>> =
        FxHashMap::default();

    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);

        // If we're in un-strict mode, don't flag typing-only imports that are
        // implicitly loaded by way of a valid runtime import.
        if !checker.settings.flake8_type_checking.strict
            && runtime_imports
                .iter()
                .any(|import| is_implicit_import(binding, import))
        {
            continue;
        }

        let Some(qualified_name) = binding.qualified_name() else {
            continue;
        };

        if is_exempt(
            qualified_name,
            &checker
                .settings
                .flake8_type_checking
                .exempt_modules
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ) {
            continue;
        }

        let Some(reference_id) = binding.references.first().copied() else {
            continue;
        };

        if binding.context.is_runtime()
            && binding.references().all(|reference_id| {
                checker
                    .semantic()
                    .reference(reference_id)
                    .context()
                    .is_typing()
            })
        {
            // Extract the module base and level from the full name.
            // Ex) `foo.bar.baz` -> `foo`, `0`
            // Ex) `.foo.bar.baz` -> `foo`, `1`
            let level = qualified_name
                .chars()
                .take_while(|c| *c == '.')
                .count()
                .try_into()
                .unwrap();

            // Categorize the import, using coarse-grained categorization.
            let import_type = match categorize(
                qualified_name,
                Some(level),
                &checker.settings.src,
                checker.package(),
                &checker.settings.isort.known_modules,
                checker.settings.target_version,
            ) {
                ImportSection::Known(ImportType::LocalFolder | ImportType::FirstParty) => {
                    ImportType::FirstParty
                }
                ImportSection::Known(ImportType::ThirdParty) | ImportSection::UserDefined(_) => {
                    ImportType::ThirdParty
                }
                ImportSection::Known(ImportType::StandardLibrary) => ImportType::StandardLibrary,
                ImportSection::Known(ImportType::Future) => {
                    continue;
                }
            };

            if !checker.enabled(rule_for(import_type)) {
                continue;
            }

            let Some(stmt_id) = binding.source else {
                continue;
            };

            let import = Import {
                qualified_name,
                reference_id,
                range: binding.range,
                parent_range: binding.parent_range(checker.semantic()),
            };

            if checker.rule_is_ignored(rule_for(import_type), import.range.start())
                || import.parent_range.map_or(false, |parent_range| {
                    checker.rule_is_ignored(rule_for(import_type), parent_range.start())
                })
            {
                ignores_by_statement
                    .entry((stmt_id, import_type))
                    .or_default()
                    .push(import);
            } else {
                errors_by_statement
                    .entry((stmt_id, import_type))
                    .or_default()
                    .push(import);
            }
        }
    }

    // Generate a diagnostic for every import, but share a fix across all imports within the same
    // statement (excluding those that are ignored).
    for ((stmt_id, import_type), imports) in errors_by_statement {
        let fix = if checker.patch(rule_for(import_type)) {
            fix_imports(checker, stmt_id, &imports).ok()
        } else {
            None
        };

        for Import {
            qualified_name,
            range,
            parent_range,
            ..
        } in imports
        {
            let mut diagnostic = Diagnostic::new(
                diagnostic_for(import_type, qualified_name.to_string()),
                range,
            );
            if let Some(range) = parent_range {
                diagnostic.set_parent(range.start());
            }
            if let Some(fix) = fix.as_ref() {
                diagnostic.set_fix(fix.clone());
            }
            diagnostics.push(diagnostic);
        }
    }

    // Separately, generate a diagnostic for every _ignored_ import, to ensure that the
    // suppression comments aren't marked as unused.
    for ((_, import_type), imports) in ignores_by_statement {
        for Import {
            qualified_name,
            range,
            parent_range,
            ..
        } in imports
        {
            let mut diagnostic = Diagnostic::new(
                diagnostic_for(import_type, qualified_name.to_string()),
                range,
            );
            if let Some(range) = parent_range {
                diagnostic.set_parent(range.start());
            }
            diagnostics.push(diagnostic);
        }
    }
}

/// A runtime-required import with its surrounding context.
struct Import<'a> {
    /// The qualified name of the import (e.g., `typing.List` for `from typing import List`).
    qualified_name: &'a str,
    /// The first reference to the imported symbol.
    reference_id: ReferenceId,
    /// The trimmed range of the import (e.g., `List` in `from typing import List`).
    range: TextRange,
    /// The range of the import's parent statement.
    parent_range: Option<TextRange>,
}

/// Return the [`Rule`] for the given import type.
fn rule_for(import_type: ImportType) -> Rule {
    match import_type {
        ImportType::StandardLibrary => Rule::TypingOnlyStandardLibraryImport,
        ImportType::ThirdParty => Rule::TypingOnlyThirdPartyImport,
        ImportType::FirstParty => Rule::TypingOnlyFirstPartyImport,
        _ => unreachable!("Unexpected import type"),
    }
}

/// Return the [`Diagnostic`] for the given import type.
fn diagnostic_for(import_type: ImportType, qualified_name: String) -> DiagnosticKind {
    match import_type {
        ImportType::StandardLibrary => TypingOnlyStandardLibraryImport { qualified_name }.into(),
        ImportType::ThirdParty => TypingOnlyThirdPartyImport { qualified_name }.into(),
        ImportType::FirstParty => TypingOnlyFirstPartyImport { qualified_name }.into(),
        _ => unreachable!("Unexpected import type"),
    }
}

/// Return `true` if `this` is implicitly loaded via importing `that`.
fn is_implicit_import(this: &Binding, that: &Binding) -> bool {
    let Some(this_module) = this.module_name() else {
        return false;
    };
    let Some(that_module) = that.module_name() else {
        return false;
    };
    this_module == that_module
}

/// Return `true` if `name` is exempt from typing-only enforcement.
fn is_exempt(name: &str, exempt_modules: &[&str]) -> bool {
    let mut name = name;
    loop {
        if exempt_modules.contains(&name) {
            return true;
        }
        match name.rfind('.') {
            Some(idx) => {
                name = &name[..idx];
            }
            None => return false,
        }
    }
}

/// Generate a [`Fix`] to remove typing-only imports from a runtime context.
fn fix_imports(checker: &Checker, stmt_id: NodeId, imports: &[Import]) -> Result<Fix> {
    let stmt = checker.semantic().stmts[stmt_id];
    let parent = checker.semantic().stmts.parent(stmt);
    let qualified_names: Vec<&str> = imports
        .iter()
        .map(|Import { qualified_name, .. }| *qualified_name)
        .collect();

    // Find the first reference across all imports.
    let at = imports
        .iter()
        .map(|Import { reference_id, .. }| {
            checker.semantic().reference(*reference_id).range().start()
        })
        .min()
        .expect("Expected at least one import");

    // Step 1) Remove the import.
    let remove_import_edit = autofix::edits::remove_unused_imports(
        qualified_names.iter().copied(),
        stmt,
        parent,
        checker.locator,
        checker.stylist,
        checker.indexer,
    )?;

    // Step 2) Add the import to a `TYPE_CHECKING` block.
    let add_import_edit = checker.importer.typing_import_edit(
        &StmtImports {
            stmt,
            qualified_names,
        },
        at,
        checker.semantic(),
    )?;

    Ok(
        Fix::suggested_edits(remove_import_edit, add_import_edit.into_edits())
            .isolate(checker.isolation(parent)),
    )
}
