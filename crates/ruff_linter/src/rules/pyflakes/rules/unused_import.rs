use std::borrow::Cow;
use std::iter;

use anyhow::{anyhow, bail, Result};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Applicability, Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Stmt, StmtImportFrom};
use ruff_python_semantic::{AnyImport, Exceptions, Imported, NodeId, Scope};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix;
use crate::registry::Rule;
use crate::rules::{isort, isort::ImportSection, isort::ImportType};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum UnusedImportContext {
    ExceptHandler,
    Init { first_party: bool },
}

/// ## What it does
/// Checks for unused imports.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles. They also increase the cognitive load of reading the code.
///
/// If an import statement is used to check for the availability or existence
/// of a module, consider using `importlib.util.find_spec` instead.
///
/// If an import statement is used to re-export a symbol as part of a module's
/// public interface, consider using a "redundant" import alias, which
/// instructs Ruff (and other tools) to respect the re-export, and avoid
/// marking it as unused, as in:
///
/// ```python
/// from module import member as member
/// ```
///
/// ## Fix safety
///
/// When `ignore_init_module_imports` is disabled, fixes can remove for unused imports in `__init__` files.
/// These fixes are considered unsafe because they can change the public interface.
///
/// ## Example
/// ```python
/// import numpy as np  # unused import
///
///
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// Use instead:
/// ```python
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// To check the availability of a module, use `importlib.util.find_spec`:
/// ```python
/// from importlib.util import find_spec
///
/// if find_spec("numpy") is not None:
///     print("numpy is installed")
/// else:
///     print("numpy is not installed")
/// ```
///
/// ## Options
/// - `lint.ignore-init-module-imports`
///
/// ## References
/// - [Python documentation: `import`](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation: `importlib.util.find_spec`](https://docs.python.org/3/library/importlib.html#importlib.util.find_spec)
/// - [Typing documentation: interface conventions](https://typing.readthedocs.io/en/latest/source/libraries.html#library-interface-public-and-private-symbols)
#[violation]
pub struct UnusedImport {
    name: String,
    context: Option<UnusedImportContext>,
    multiple: bool,
}

impl Violation for UnusedImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedImport { name, context, .. } = self;
        match context {
            Some(UnusedImportContext::ExceptHandler) => {
                format!(
                    "`{name}` imported but unused; consider using `importlib.util.find_spec` to test for availability"
                )
            }
            Some(UnusedImportContext::Init { .. }) => {
                format!(
                    "`{name}` imported but unused; consider removing, adding to `__all__`, or using a redundant alias"
                )
            }
            None => format!("`{name}` imported but unused"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnusedImport { name, multiple, .. } = self;
        let resolution = match self.context {
            Some(UnusedImportContext::Init { first_party: true }) => "Use a redundant alias",
            _ => "Remove unused import",
        };
        Some(if *multiple {
            resolution.to_string()
        } else {
            format!("{resolution}: `{name}`")
        })
    }
}

fn is_first_party(qualified_name: &str, level: u32, checker: &Checker) -> bool {
    let category = isort::categorize(
        qualified_name,
        level,
        &checker.settings.src,
        checker.package(),
        checker.settings.isort.detect_same_package,
        &checker.settings.isort.known_modules,
        checker.settings.target_version,
        checker.settings.isort.no_sections,
        &checker.settings.isort.section_order,
        &checker.settings.isort.default_section,
    );
    matches! {
        category,
        ImportSection::Known(ImportType::FirstParty | ImportType::LocalFolder)
    }
}

/// For some unused binding in an import statement...
///
///  __init__.py ∧ 1stpty → safe,   convert to redundant-alias
///  __init__.py ∧ stdlib → unsafe, remove
///  __init__.py ∧ 3rdpty → unsafe, remove
///
/// ¬__init__.py ∧ 1stpty → safe,   remove
/// ¬__init__.py ∧ stdlib → safe,   remove
/// ¬__init__.py ∧ 3rdpty → safe,   remove
///
pub(crate) fn unused_import(checker: &Checker, scope: &Scope, diagnostics: &mut Vec<Diagnostic>) {
    // Collect all unused imports by statement.
    let mut unused: FxHashMap<(NodeId, Exceptions), Vec<ImportBinding>> = FxHashMap::default();
    let mut ignored: FxHashMap<(NodeId, Exceptions), Vec<ImportBinding>> = FxHashMap::default();

    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);

        if binding.is_used()
            || binding.is_explicit_export()
            || binding.is_nonlocal()
            || binding.is_global()
        {
            continue;
        }

        let Some(import) = binding.as_any_import() else {
            continue;
        };

        let Some(node_id) = binding.source else {
            continue;
        };

        let import = ImportBinding {
            import,
            range: binding.range(),
            parent_range: binding.parent_range(checker.semantic()),
        };

        if checker.rule_is_ignored(Rule::UnusedImport, import.start())
            || import.parent_range.is_some_and(|parent_range| {
                checker.rule_is_ignored(Rule::UnusedImport, parent_range.start())
            })
        {
            ignored
                .entry((node_id, binding.exceptions))
                .or_default()
                .push(import);
        } else {
            unused
                .entry((node_id, binding.exceptions))
                .or_default()
                .push(import);
        }
    }

    let in_init = checker.path().ends_with("__init__.py");
    let fix_init = checker.settings.preview.is_enabled();

    // Generate a diagnostic for every import, but share fixes across all imports within the same
    // statement (excluding those that are ignored).
    for ((import_statement, exceptions), bindings) in unused {
        let in_except_handler =
            exceptions.intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);
        let multiple = bindings.len() > 1;
        let level = match checker.semantic().statement(import_statement) {
            Stmt::Import(_) => 0,
            Stmt::ImportFrom(StmtImportFrom { level, .. }) => *level,
            _ => {
                continue;
            }
        };

        // pair each binding with context; divide them by how we want to fix them
        let (to_reexport, to_remove): (Vec<_>, Vec<_>) = bindings
            .into_iter()
            .map(|binding| {
                let context = if in_except_handler {
                    Some(UnusedImportContext::ExceptHandler)
                } else if in_init {
                    Some(UnusedImportContext::Init {
                        first_party: is_first_party(
                            &binding.import.qualified_name().to_string(),
                            level,
                            checker,
                        ),
                    })
                } else {
                    None
                };
                (binding, context)
            })
            .partition(|(_, context)| {
                matches!(
                    context,
                    Some(UnusedImportContext::Init { first_party: true })
                )
            });

        // generate fixes that are shared across bindings in the statement
        let (fix_remove, fix_reexport) = if (!in_init || fix_init) && !in_except_handler {
            (
                fix_by_removing_imports(
                    checker,
                    import_statement,
                    to_remove.iter().map(|(binding, _)| binding),
                    in_init,
                )
                .ok(),
                fix_by_reexporting(
                    checker,
                    import_statement,
                    to_reexport.iter().map(|(binding, _)| binding),
                )
                .ok(),
            )
        } else {
            (None, None)
        };

        for ((binding, context), fix) in iter::Iterator::chain(
            iter::zip(to_remove, iter::repeat(fix_remove)),
            iter::zip(to_reexport, iter::repeat(fix_reexport)),
        ) {
            let mut diagnostic = Diagnostic::new(
                UnusedImport {
                    name: binding.import.qualified_name().to_string(),
                    context,
                    multiple,
                },
                binding.range,
            );
            if let Some(range) = binding.parent_range {
                diagnostic.set_parent(range.start());
            }
            if !in_except_handler {
                if let Some(fix) = fix.as_ref() {
                    diagnostic.set_fix(fix.clone());
                }
            }
            diagnostics.push(diagnostic);
        }
    }

    // Separately, generate a diagnostic for every _ignored_ import, to ensure that the
    // suppression comments aren't marked as unused.
    for ImportBinding {
        import,
        range,
        parent_range,
    } in ignored.into_values().flatten()
    {
        let mut diagnostic = Diagnostic::new(
            UnusedImport {
                name: import.qualified_name().to_string(),
                context: None,
                multiple: false,
            },
            range,
        );
        if let Some(range) = parent_range {
            diagnostic.set_parent(range.start());
        }
        diagnostics.push(diagnostic);
    }
}

/// An unused import with its surrounding context.
#[derive(Debug)]
struct ImportBinding<'a> {
    /// The qualified name of the import (e.g., `typing.List` for `from typing import List`).
    import: AnyImport<'a, 'a>,
    /// The trimmed range of the import (e.g., `List` in `from typing import List`).
    range: TextRange,
    /// The range of the import's parent statement.
    parent_range: Option<TextRange>,
}

impl Ranged for ImportBinding<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Generate a [`Fix`] to remove unused imports from a statement.
fn fix_by_removing_imports<'a>(
    checker: &Checker,
    node_id: NodeId,
    imports: impl Iterator<Item = &'a ImportBinding<'a>>,
    in_init: bool,
) -> Result<Fix> {
    let statement = checker.semantic().statement(node_id);
    let parent = checker.semantic().parent_statement(node_id);

    let member_names: Vec<Cow<'_, str>> = imports
        .map(|ImportBinding { import, .. }| import)
        .map(Imported::member_name)
        .collect();
    if member_names.is_empty() {
        bail!("Expected import bindings");
    }

    let edit = fix::edits::remove_unused_imports(
        member_names.iter().map(AsRef::as_ref),
        statement,
        parent,
        checker.locator(),
        checker.stylist(),
        checker.indexer(),
    )?;

    // It's unsafe to remove things from `__init__.py` because it can break public interfaces
    let applicability = if in_init {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Ok(
        Fix::applicable_edit(edit, applicability).isolate(Checker::isolation(
            checker.semantic().parent_statement_id(node_id),
        )),
    )
}

/// Generate a [`Fix`] to make bindings in a statement explicit, by changing from `import a` to
/// `import a as a`.
fn fix_by_reexporting<'a>(
    checker: &Checker,
    node_id: NodeId,
    imports: impl Iterator<Item = &'a ImportBinding<'a>>,
) -> Result<Fix> {
    let statement = checker.semantic().statement(node_id);

    let member_names = imports
        .map(|binding| binding.import.member_name())
        .collect::<Vec<_>>();
    if member_names.is_empty() {
        bail!("Expected import bindings");
    }

    let edits = fix::edits::make_redundant_alias(member_names.iter().map(AsRef::as_ref), statement);

    // Only emit a fix if there are edits
    let mut tail = edits.into_iter();
    let head = tail.next().ok_or(anyhow!("No edits to make"))?;

    let isolation = Checker::isolation(checker.semantic().parent_statement_id(node_id));
    Ok(Fix::safe_edits(head, tail).isolate(isolation))
}
