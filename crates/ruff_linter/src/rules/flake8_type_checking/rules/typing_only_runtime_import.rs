use std::borrow::Cow;

use anyhow::Result;
use rustc_hash::FxHashMap;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{PythonVersion, Stmt};
use ruff_python_semantic::{Binding, Imported, NodeId, Scope};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::{Checker, DiagnosticGuard};
use crate::codes::{Category, Rule};
use crate::fix;
use crate::importer::ImportedMembers;
use crate::rules::flake8_tidy_imports::rules::BannedModuleImportPolicies;
use crate::rules::flake8_type_checking::helpers::{
    TypingReference, filter_contained, quote_annotation,
};
use crate::rules::flake8_type_checking::imports::ImportBinding;
use crate::rules::isort::{ImportSection, ImportType, categorize};
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for first-party imports that are only used for type annotations, but
/// aren't imported lazily or defined in a type-checking block.
///
/// ## Why is this bad?
/// Imports that are only used for type annotations add a performance overhead
/// at runtime. For first-party imports, they can also contribute to import
/// cycles. If an import is _only_ used in typing-only contexts, it can instead
/// be imported conditionally under an `if TYPE_CHECKING:` block to minimize
/// runtime overhead.
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
/// If [`lint.future-annotations`] is set to `true`, `from __future__ import
/// annotations` will be added if doing so would enable an import to be
/// moved into an `if TYPE_CHECKING:` block. This takes precedence over the
/// [`lint.flake8-type-checking.quote-annotations`] setting described above if
/// both settings are enabled.
///
/// On Python 3.15 and later, lazy imports are also exempt, including imports
/// made lazy by a literal `__lazy_modules__` declaration. The fix prefers adding
/// `lazy` to single-name import statements where the syntax is legal and
/// [`lint.flake8-tidy-imports.ban-lazy`] allows it. This defers the import while
/// keeping the name available for runtime annotation inspection.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// from . import local_module
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
///     from . import local_module
///
///
/// def func(sized: local_module.Container) -> int:
///     return len(sized)
/// ```
///
/// On Python 3.15 and later, use instead:
/// ```python
/// lazy from . import local_module
///
///
/// def func(sized: local_module.Container) -> int:
///     return len(sized)
/// ```
///
/// ## Fix safety
/// This rule's fixes are unsafe because changing when a module is imported can
/// affect runtime behavior, including import-time side effects.
///
/// ## Options
/// - `lint.flake8-tidy-imports.ban-lazy`
/// - `lint.flake8-type-checking.quote-annotations`
/// - `lint.flake8-type-checking.runtime-evaluated-base-classes`
/// - `lint.flake8-type-checking.runtime-evaluated-decorators`
/// - `lint.flake8-type-checking.strict`
/// - `lint.typing-modules`
/// - `lint.future-annotations`
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0", category = Category::Pedantic)]
pub(crate) struct TypingOnlyFirstPartyImport {
    qualified_name: String,
    fix_style: ImportFixStyle,
}

impl Violation for TypingOnlyFirstPartyImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.fix_style {
            ImportFixStyle::TypeCheckingBlock => format!(
                "Move application import `{}` into a type-checking block",
                self.qualified_name
            ),
            ImportFixStyle::LazyImport => {
                format!("Make application import `{}` lazy", self.qualified_name)
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(self.fix_style.fix_title().to_string())
    }
}

/// ## What it does
/// Checks for third-party imports that are only used for type annotations, but
/// aren't imported lazily or defined in a type-checking block.
///
/// ## Why is this bad?
/// Imports that are only used for type annotations add a performance overhead
/// at runtime. If an import is _only_ used in typing-only contexts, it can
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
/// If [`lint.future-annotations`] is set to `true`, `from __future__ import
/// annotations` will be added if doing so would enable an import to be
/// moved into an `if TYPE_CHECKING:` block. This takes precedence over the
/// [`lint.flake8-type-checking.quote-annotations`] setting described above if
/// both settings are enabled.
///
/// On Python 3.15 and later, lazy imports are also exempt, including imports
/// made lazy by a literal `__lazy_modules__` declaration. The fix prefers adding
/// `lazy` to single-name import statements where the syntax is legal and
/// [`lint.flake8-tidy-imports.ban-lazy`] allows it. This defers the import while
/// keeping the name available for runtime annotation inspection.
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
/// On Python 3.15 and later, use instead:
/// ```python
/// lazy import pandas as pd
///
///
/// def func(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// ## Fix safety
/// This rule's fixes are unsafe because changing when a module is imported can
/// affect runtime behavior, including import-time side effects.
///
/// ## Options
/// - `lint.flake8-tidy-imports.ban-lazy`
/// - `lint.flake8-type-checking.quote-annotations`
/// - `lint.flake8-type-checking.runtime-evaluated-base-classes`
/// - `lint.flake8-type-checking.runtime-evaluated-decorators`
/// - `lint.flake8-type-checking.strict`
/// - `lint.typing-modules`
/// - `lint.future-annotations`
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0", category = Category::Pedantic)]
pub(crate) struct TypingOnlyThirdPartyImport {
    qualified_name: String,
    fix_style: ImportFixStyle,
}

impl Violation for TypingOnlyThirdPartyImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.fix_style {
            ImportFixStyle::TypeCheckingBlock => format!(
                "Move third-party import `{}` into a type-checking block",
                self.qualified_name
            ),
            ImportFixStyle::LazyImport => {
                format!("Make third-party import `{}` lazy", self.qualified_name)
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(self.fix_style.fix_title().to_string())
    }
}

/// ## What it does
/// Checks for standard library imports that are only used for type
/// annotations, but aren't imported lazily or defined in a type-checking block.
///
/// ## Why is this bad?
/// Imports that are only used for type annotations add a performance overhead
/// at runtime. If an import is _only_ used in typing-only contexts, it can
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
/// If [`lint.future-annotations`] is set to `true`, `from __future__ import
/// annotations` will be added if doing so would enable an import to be
/// moved into an `if TYPE_CHECKING:` block. This takes precedence over the
/// [`lint.flake8-type-checking.quote-annotations`] setting described above if
/// both settings are enabled.
///
/// On Python 3.15 and later, lazy imports are also exempt, including imports
/// made lazy by a literal `__lazy_modules__` declaration. The fix prefers adding
/// `lazy` to single-name import statements where the syntax is legal and
/// [`lint.flake8-tidy-imports.ban-lazy`] allows it. This defers the import while
/// keeping the name available for runtime annotation inspection.
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
/// On Python 3.15 and later, use instead:
/// ```python
/// lazy from pathlib import Path
///
///
/// def func(path: Path) -> str:
///     return str(path)
/// ```
///
/// ## Fix safety
/// This rule's fixes are unsafe because changing when a module is imported can
/// affect runtime behavior, including import-time side effects.
///
/// ## Options
/// - `lint.flake8-tidy-imports.ban-lazy`
/// - `lint.flake8-type-checking.quote-annotations`
/// - `lint.flake8-type-checking.runtime-evaluated-base-classes`
/// - `lint.flake8-type-checking.runtime-evaluated-decorators`
/// - `lint.flake8-type-checking.strict`
/// - `lint.typing-modules`
/// - `lint.future-annotations`
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0", category = Category::Pedantic)]
pub(crate) struct TypingOnlyStandardLibraryImport {
    qualified_name: String,
    fix_style: ImportFixStyle,
}

impl Violation for TypingOnlyStandardLibraryImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.fix_style {
            ImportFixStyle::TypeCheckingBlock => format!(
                "Move standard library import `{}` into a type-checking block",
                self.qualified_name
            ),
            ImportFixStyle::LazyImport => {
                format!(
                    "Make standard library import `{}` lazy",
                    self.qualified_name
                )
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(self.fix_style.fix_title().to_string())
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
        if !checker.settings().flake8_type_checking.strict
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

        if !binding.context.is_runtime()
            || (checker.target_version() >= PythonVersion::PY315 && binding.is_lazy())
        {
            continue;
        }

        let typing_reference =
            TypingReference::from_references(binding, checker.semantic(), checker.settings());

        let needs_future_import = match typing_reference {
            TypingReference::Runtime => continue,
            // We can only get the `Future` variant if `future_annotations` is
            // enabled, so we can unconditionally set this here.
            TypingReference::Future => true,
            TypingReference::TypingOnly | TypingReference::Quote => false,
        };

        let qualified_name = import.qualified_name();

        if is_exempt(
            &qualified_name.to_string(),
            &checker
                .settings()
                .flake8_type_checking
                .exempt_modules
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ) {
            continue;
        }

        let source_name = import.source_name().join(".");

        // Categorize the import, using coarse-grained categorization.
        let import_type = match categorize(
            &source_name,
            qualified_name.is_unresolved_import(),
            &checker.settings().src,
            checker.package(),
            checker.settings().isort.detect_same_package,
            &checker.settings().isort.known_modules,
            checker.target_version(),
            checker.settings().isort.no_sections,
            &checker.settings().isort.section_order,
            &checker.settings().isort.default_section,
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

        if !checker.is_rule_enabled(rule_for(import_type)) {
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
            needs_future_import,
            runtime_reference: None,
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

    // Generate a diagnostic for every import, but share a fix across all imports within the same
    // statement (excluding those that are ignored).
    #[expect(
        clippy::iter_over_hash_type,
        reason = "each statement group produces diagnostics and a fix independently"
    )]
    for ((node_id, import_type), imports) in errors_by_statement {
        let fix_style = ImportFixStyle::for_import(checker, scope, node_id);
        let fix = fix_imports(checker, node_id, &imports, fix_style).ok();

        for ImportBinding {
            import,
            range,
            parent_range,
            ..
        } in imports
        {
            let mut diagnostic = diagnostic_for(
                checker,
                import_type,
                import.qualified_name().to_string(),
                fix_style,
                range,
            );
            if let Some(range) = parent_range {
                diagnostic.set_parent(range.start());
            }
            if let Some(fix) = fix.as_ref() {
                diagnostic.set_fix(fix.clone());
            }
        }
    }

    // Separately, generate a diagnostic for every _ignored_ import, to ensure that the
    // suppression comments aren't marked as unused.
    #[expect(
        clippy::iter_over_hash_type,
        reason = "each ignored statement group produces diagnostics independently"
    )]
    for ((node_id, import_type), imports) in ignores_by_statement {
        let fix_style = ImportFixStyle::for_import(checker, scope, node_id);
        for ImportBinding {
            import,
            range,
            parent_range,
            ..
        } in imports
        {
            let mut diagnostic = diagnostic_for(
                checker,
                import_type,
                import.qualified_name().to_string(),
                fix_style,
                range,
            );
            if let Some(range) = parent_range {
                diagnostic.set_parent(range.start());
            }
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
fn diagnostic_for<'a, 'b>(
    checker: &'a Checker<'b>,
    import_type: ImportType,
    qualified_name: String,
    fix_style: ImportFixStyle,
    range: TextRange,
) -> DiagnosticGuard<'a, 'b> {
    match import_type {
        ImportType::StandardLibrary => checker.report_diagnostic(
            TypingOnlyStandardLibraryImport {
                qualified_name,
                fix_style,
            },
            range,
        ),
        ImportType::ThirdParty => checker.report_diagnostic(
            TypingOnlyThirdPartyImport {
                qualified_name,
                fix_style,
            },
            range,
        ),
        ImportType::FirstParty => checker.report_diagnostic(
            TypingOnlyFirstPartyImport {
                qualified_name,
                fix_style,
            },
            range,
        ),
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

/// Generate a [`Fix`] to defer imports used only for typing.
fn fix_imports(
    checker: &Checker,
    node_id: NodeId,
    imports: &[ImportBinding],
    fix_style: ImportFixStyle,
) -> Result<Fix> {
    let statement = checker.semantic().statement(node_id);
    if matches!(fix_style, ImportFixStyle::LazyImport) {
        return Ok(Fix::unsafe_edit(Edit::insertion(
            "lazy ".to_string(),
            statement.start(),
        )));
    }
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

    let add_future_import = imports.iter().any(|binding| binding.needs_future_import);

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

    // Step 3) Either add a `__future__` import or quote any runtime usages of the referenced
    // symbol.
    let fix = if add_future_import {
        let future_import = checker.importer().add_future_import();

        // The order here is very important. We first need to add the `__future__` import, if
        // needed, since it's a syntax error to come later. Then `type_checking_edit` imports
        // `TYPE_CHECKING`, if available. Then we can add and/or remove existing imports.
        Fix::unsafe_edits(
            future_import,
            std::iter::once(type_checking_edit)
                .chain(add_import_edit)
                .chain(std::iter::once(remove_import_edit)),
        )
    } else {
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
        Fix::unsafe_edits(
            type_checking_edit,
            add_import_edit
                .into_iter()
                .chain(std::iter::once(remove_import_edit))
                .chain(quote_reference_edits),
        )
    };

    Ok(fix.isolate(Checker::isolation(
        checker.semantic().parent_statement_id(node_id),
    )))
}

#[derive(Debug, Clone, Copy)]
enum ImportFixStyle {
    TypeCheckingBlock,
    LazyImport,
}

impl ImportFixStyle {
    fn for_import(checker: &Checker, scope: &Scope, node_id: NodeId) -> Self {
        let semantic = checker.semantic();
        if checker.target_version() < PythonVersion::PY315
            || !scope.kind.is_module()
            || semantic.statements(node_id).skip(1).any(Stmt::is_try_stmt)
        {
            return Self::TypeCheckingBlock;
        }

        let statement = semantic.statement(node_id);
        let names = match statement {
            Stmt::Import(import) => &import.names,
            Stmt::ImportFrom(import) => &import.names,
            _ => return Self::TypeCheckingBlock,
        };
        // Other members may be used at runtime, suppressed, or governed by another lazy-import policy.
        if names.len() != 1 {
            return Self::TypeCheckingBlock;
        }

        let ban_lazy = &checker.settings().flake8_tidy_imports.ban_lazy;
        for (policy, node) in &BannedModuleImportPolicies::new(statement, checker) {
            if ban_lazy.includes_all() && statement.is_import_from_stmt() && node.is_alias() {
                continue;
            }
            if ban_lazy.find(&policy).is_some() {
                return Self::TypeCheckingBlock;
            }
        }

        Self::LazyImport
    }

    fn fix_title(self) -> &'static str {
        match self {
            Self::TypeCheckingBlock => "Move into type-checking block",
            Self::LazyImport => "Convert to a lazy import",
        }
    }
}
