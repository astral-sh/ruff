use std::borrow::Cow;

use anyhow::Result;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::{Binding, Imported, NodeId, Scope};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::fix;
use crate::importer::ImportedMembers;
use crate::rules::flake8_type_checking::helpers::{
    filter_contained, is_typing_reference, quote_annotation,
};
use crate::rules::flake8_type_checking::imports::ImportBinding;
use crate::rules::isort::{categorize, ImportSection, ImportType};

/// ## What it does
/// Checks for first-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles. If an import is _only_ used in typing-only contexts, it can
/// instead be imported conditionally under an `if TYPE_CHECKING:` block to
/// minimize runtime overhead.
///
/// If [`lint.flake8-type-checking.quote-annotations`] is set to `true`,
/// annotations will be wrapped in quotes if doing so would enable the
/// corresponding import to be moved into an `if TYPE_CHECKING:` block.
///
/// If a class _requires_ that type annotations be available at runtime (as is
/// the case for Pydantic, SQLAlchemy, and other libraries), consider using
/// the [`lint.flake8-type-checking.runtime-evaluated-base-classes`] and
/// [`lint.flake8-type-checking.runtime-evaluated-decorators`] settings to mark them
/// as such.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import local_module
///
///
/// def func(sized: local_module.Container) -> int:
///     return len(sized)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import local_module
///
///
/// def func(sized: local_module.Container) -> int:
///     return len(sized)
/// ```
///
/// ## Options
/// - `lint.flake8-type-checking.quote-annotations`
/// - `lint.flake8-type-checking.runtime-evaluated-base-classes`
/// - `lint.flake8-type-checking.runtime-evaluated-decorators`
/// - `lint.flake8-type-checking.strict`
/// - `lint.typing-modules`
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[derive(ViolationMetadata)]
pub(crate) struct TypingOnlyFirstPartyImport {
    qualified_name: String,
}

impl Violation for TypingOnlyFirstPartyImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move application import `{}` into a type-checking block",
            self.qualified_name
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move into type-checking block".to_string())
    }
}

/// ## What it does
/// Checks for third-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles. If an import is _only_ used in typing-only contexts, it can
/// instead be imported conditionally under an `if TYPE_CHECKING:` block to
/// minimize runtime overhead.
///
/// If [`lint.flake8-type-checking.quote-annotations`] is set to `true`,
/// annotations will be wrapped in quotes if doing so would enable the
/// corresponding import to be moved into an `if TYPE_CHECKING:` block.
///
/// If a class _requires_ that type annotations be available at runtime (as is
/// the case for Pydantic, SQLAlchemy, and other libraries), consider using
/// the [`lint.flake8-type-checking.runtime-evaluated-base-classes`] and
/// [`lint.flake8-type-checking.runtime-evaluated-decorators`] settings to mark them
/// as such.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import pandas as pd
///
///
/// def func(df: pd.DataFrame) -> int:
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
/// def func(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// ## Options
/// - `lint.flake8-type-checking.quote-annotations`
/// - `lint.flake8-type-checking.runtime-evaluated-base-classes`
/// - `lint.flake8-type-checking.runtime-evaluated-decorators`
/// - `lint.flake8-type-checking.strict`
/// - `lint.typing-modules`
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[derive(ViolationMetadata)]
pub(crate) struct TypingOnlyThirdPartyImport {
    qualified_name: String,
}

impl Violation for TypingOnlyThirdPartyImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move third-party import `{}` into a type-checking block",
            self.qualified_name
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move into type-checking block".to_string())
    }
}

/// ## What it does
/// Checks for standard library imports that are only used for type
/// annotations, but aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles. If an import is _only_ used in typing-only contexts, it can
/// instead be imported conditionally under an `if TYPE_CHECKING:` block to
/// minimize runtime overhead.
///
/// If [`lint.flake8-type-checking.quote-annotations`] is set to `true`,
/// annotations will be wrapped in quotes if doing so would enable the
/// corresponding import to be moved into an `if TYPE_CHECKING:` block.
///
/// If a class _requires_ that type annotations be available at runtime (as is
/// the case for Pydantic, SQLAlchemy, and other libraries), consider using
/// the [`lint.flake8-type-checking.runtime-evaluated-base-classes`] and
/// [`lint.flake8-type-checking.runtime-evaluated-decorators`] settings to mark them
/// as such.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// from pathlib import Path
///
///
/// def func(path: Path) -> str:
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
/// def func(path: Path) -> str:
///     return str(path)
/// ```
///
/// ## Options
/// - `lint.flake8-type-checking.quote-annotations`
/// - `lint.flake8-type-checking.runtime-evaluated-base-classes`
/// - `lint.flake8-type-checking.runtime-evaluated-decorators`
/// - `lint.flake8-type-checking.strict`
/// - `lint.typing-modules`
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[derive(ViolationMetadata)]
pub(crate) struct TypingOnlyStandardLibraryImport {
    qualified_name: String,
}

impl Violation for TypingOnlyStandardLibraryImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move standard library import `{}` into a type-checking block",
            self.qualified_name
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move into type-checking block".to_string())
    }
}

/// TC001, TC002, TC003
pub(crate) fn typing_only_runtime_import(
    checker: &Checker,
    scope: &Scope,
    runtime_imports: &[&Binding],
) {
    // Collect all typing-only imports by statement and import type.
    let mut errors_by_statement: FxHashMap<(NodeId, ImportType), Vec<ImportBinding>> =
        FxHashMap::default();
    let mut ignores_by_statement: FxHashMap<(NodeId, ImportType), Vec<ImportBinding>> =
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

        let Some(import) = binding.as_any_import() else {
            continue;
        };

        let Some(reference_id) = binding.references.first().copied() else {
            continue;
        };

        if binding.context.is_runtime()
            && binding
                .references()
                .map(|reference_id| checker.semantic().reference(reference_id))
                .all(|reference| {
                    is_typing_reference(reference, &checker.settings.flake8_type_checking)
                })
        {
            let qualified_name = import.qualified_name();

            if is_exempt(
                &qualified_name.to_string(),
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

            // Categorize the import, using coarse-grained categorization.
            let import_type = match categorize(
                &qualified_name.to_string(),
                qualified_name.is_unresolved_import(),
                &checker.settings.src,
                checker.package(),
                checker.settings.isort.detect_same_package,
                &checker.settings.isort.known_modules,
                checker.target_version(),
                checker.settings.isort.no_sections,
                &checker.settings.isort.section_order,
                &checker.settings.isort.default_section,
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

            let Some(node_id) = binding.source else {
                continue;
            };

            let import = ImportBinding {
                import,
                reference_id,
                binding,
                range: binding.range(),
                parent_range: binding.parent_range(checker.semantic()),
            };

            if checker.rule_is_ignored(rule_for(import_type), import.start())
                || import.parent_range.is_some_and(|parent_range| {
                    checker.rule_is_ignored(rule_for(import_type), parent_range.start())
                })
            {
                ignores_by_statement
                    .entry((node_id, import_type))
                    .or_default()
                    .push(import);
            } else {
                errors_by_statement
                    .entry((node_id, import_type))
                    .or_default()
                    .push(import);
            }
        }
    }

    // Generate a diagnostic for every import, but share a fix across all imports within the same
    // statement (excluding those that are ignored).
    for ((node_id, import_type), imports) in errors_by_statement {
        let fix = fix_imports(checker, node_id, &imports).ok();

        for ImportBinding {
            import,
            range,
            parent_range,
            ..
        } in imports
        {
            let mut diagnostic = Diagnostic::new(
                diagnostic_for(import_type, import.qualified_name().to_string()),
                range,
            );
            if let Some(range) = parent_range {
                diagnostic.set_parent(range.start());
            }
            if let Some(fix) = fix.as_ref() {
                diagnostic.set_fix(fix.clone());
            }
            checker.report_diagnostic(diagnostic);
        }
    }

    // Separately, generate a diagnostic for every _ignored_ import, to ensure that the
    // suppression comments aren't marked as unused.
    for ((_, import_type), imports) in ignores_by_statement {
        for ImportBinding {
            import,
            range,
            parent_range,
            ..
        } in imports
        {
            let mut diagnostic = Diagnostic::new(
                diagnostic_for(import_type, import.qualified_name().to_string()),
                range,
            );
            if let Some(range) = parent_range {
                diagnostic.set_parent(range.start());
            }
            checker.report_diagnostic(diagnostic);
        }
    }
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
    let Some(this_import) = this.as_any_import() else {
        return false;
    };
    let Some(that_import) = that.as_any_import() else {
        return false;
    };
    this_import.module_name() == that_import.module_name()
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
fn fix_imports(checker: &Checker, node_id: NodeId, imports: &[ImportBinding]) -> Result<Fix> {
    let statement = checker.semantic().statement(node_id);
    let parent = checker.semantic().parent_statement(node_id);

    let member_names: Vec<Cow<'_, str>> = imports
        .iter()
        .map(|ImportBinding { import, .. }| import)
        .map(Imported::member_name)
        .collect();

    // Find the first reference across all imports.
    let at = imports
        .iter()
        .map(|ImportBinding { reference_id, .. }| {
            checker.semantic().reference(*reference_id).start()
        })
        .min()
        .expect("Expected at least one import");

    // Step 1) Remove the import.
    let remove_import_edit = fix::edits::remove_unused_imports(
        member_names.iter().map(AsRef::as_ref),
        statement,
        parent,
        checker.locator(),
        checker.stylist(),
        checker.indexer(),
    )?;

    // Step 2) Add the import to a `TYPE_CHECKING` block.
    let (type_checking_edit, add_import_edit) = checker
        .importer()
        .typing_import_edit(
            &ImportedMembers {
                statement,
                names: member_names.iter().map(AsRef::as_ref).collect(),
            },
            at,
            checker.semantic(),
        )?
        .into_edits();

    // Step 3) Quote any runtime usages of the referenced symbol.
    let quote_reference_edits = filter_contained(
        imports
            .iter()
            .flat_map(|ImportBinding { binding, .. }| {
                binding.references.iter().filter_map(|reference_id| {
                    let reference = checker.semantic().reference(*reference_id);
                    if reference.in_runtime_context() {
                        Some(quote_annotation(
                            reference.expression_id()?,
                            checker.semantic(),
                            checker.stylist(),
                            checker.locator(),
                            checker.default_string_flags(),
                        ))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>(),
    );

    Ok(Fix::unsafe_edits(
        type_checking_edit,
        add_import_edit
            .into_iter()
            .chain(std::iter::once(remove_import_edit))
            .chain(quote_reference_edits),
    )
    .isolate(Checker::isolation(
        checker.semantic().parent_statement_id(node_id),
    )))
}
