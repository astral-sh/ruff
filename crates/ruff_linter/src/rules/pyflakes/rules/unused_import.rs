use std::borrow::Cow;
use std::iter;

use anyhow::{anyhow, bail, Result};
use std::collections::BTreeMap;

use ruff_diagnostics::{Applicability, Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::{Stmt, StmtImportFrom};
use ruff_python_semantic::{
    AnyImport, BindingKind, Exceptions, Imported, NodeId, Scope, SemanticModel,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix;
use crate::registry::Rule;
use crate::rules::{isort, isort::ImportSection, isort::ImportType};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum UnusedImportContext {
    ExceptHandler,
    Init {
        first_party: bool,
        dunder_all_count: usize,
        ignore_init_module_imports: bool,
    },
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
/// Alternatively, you can use `__all__` to declare a symbol as part of the module's
/// interface, as in:
///
/// ```python
/// # __init__.py
/// import some_module
///
/// __all__ = ["some_module"]
/// ```
///
/// ## Fix safety
///
/// Fixes to remove unused imports are safe, except in `__init__.py` files.
///
/// Applying fixes to `__init__.py` files is currently in preview. The fix offered depends on the
/// type of the unused import. Ruff will suggest a safe fix to export first-party imports with
/// either a redundant alias or, if already present in the file, an `__all__` entry. If multiple
/// `__all__` declarations are present, Ruff will not offer a fix. Ruff will suggest an unsafe fix
/// to remove third-party and standard library imports -- the fix is unsafe because the module's
/// interface changes.
///
/// ## Example
///
/// ```python
/// import numpy as np  # unused import
///
///
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// Use instead:
///
/// ```python
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// To check the availability of a module, use `importlib.util.find_spec`:
///
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
    /// Qualified name of the import
    name: String,
    /// Unqualified name of the import
    module: String,
    /// Name of the import binding
    binding: String,
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
        let UnusedImport {
            name,
            module,
            binding,
            multiple,
            ..
        } = self;
        match self.context {
            Some(UnusedImportContext::Init {
                first_party: true,
                dunder_all_count: 1,
                ignore_init_module_imports: true,
            }) => Some(format!("Add unused import `{binding}` to __all__")),

            Some(UnusedImportContext::Init {
                first_party: true,
                dunder_all_count: 0,
                ignore_init_module_imports: true,
            }) => Some(format!("Use an explicit re-export: `{module} as {module}`")),

            _ => Some(if *multiple {
                "Remove unused import".to_string()
            } else {
                format!("Remove unused import: `{name}`")
            }),
        }
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

/// Find the `Expr` for top level `__all__` bindings.
fn find_dunder_all_exprs<'a>(semantic: &'a SemanticModel) -> Vec<&'a ast::Expr> {
    semantic
        .global_scope()
        .get_all("__all__")
        .filter_map(|binding_id| {
            let binding = semantic.binding(binding_id);
            let stmt = match binding.kind {
                BindingKind::Export(_) => binding.statement(semantic),
                _ => None,
            }?;
            match stmt {
                Stmt::Assign(ast::StmtAssign { value, .. }) => Some(&**value),
                Stmt::AnnAssign(ast::StmtAnnAssign { value, .. }) => value.as_deref(),
                Stmt::AugAssign(ast::StmtAugAssign { value, .. }) => Some(&**value),
                _ => None,
            }
        })
        .collect()
}

/// For some unused binding in an import statement...
///
///  __init__.py ∧ 1stpty → safe,   if one __all__, add to __all__
///                         safe,   if no __all__, convert to redundant-alias
///                         n/a,    if multiple __all__, offer no fix
///  __init__.py ∧ stdlib → unsafe, remove
///  __init__.py ∧ 3rdpty → unsafe, remove
///
/// ¬__init__.py ∧ 1stpty → safe,   remove
/// ¬__init__.py ∧ stdlib → safe,   remove
/// ¬__init__.py ∧ 3rdpty → safe,   remove
///
pub(crate) fn unused_import(checker: &Checker, scope: &Scope, diagnostics: &mut Vec<Diagnostic>) {
    // Collect all unused imports by statement.
    let mut unused: BTreeMap<(NodeId, Exceptions), Vec<ImportBinding>> = BTreeMap::default();
    let mut ignored: BTreeMap<(NodeId, Exceptions), Vec<ImportBinding>> = BTreeMap::default();

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
            name: binding.name(checker.locator()),
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
    let fix_init = !checker.settings.ignore_init_module_imports;
    let preview_mode = checker.settings.preview.is_enabled();
    let dunder_all_exprs = find_dunder_all_exprs(checker.semantic());

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
                        dunder_all_count: dunder_all_exprs.len(),
                        ignore_init_module_imports: !fix_init,
                    })
                } else {
                    None
                };
                (binding, context)
            })
            .partition(|(_, context)| {
                matches!(
                    context,
                    Some(UnusedImportContext::Init {
                        first_party: true,
                        ..
                    })
                ) && preview_mode
            });

        // generate fixes that are shared across bindings in the statement
        let (fix_remove, fix_reexport) =
            if (!in_init || fix_init || preview_mode) && !in_except_handler {
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
                        to_reexport.iter().map(|(b, _)| b).collect::<Vec<_>>(),
                        &dunder_all_exprs,
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
                    module: binding.import.member_name().to_string(),
                    binding: binding.name.to_string(),
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
    for binding in ignored.into_values().flatten() {
        let mut diagnostic = Diagnostic::new(
            UnusedImport {
                name: binding.import.qualified_name().to_string(),
                module: binding.import.member_name().to_string(),
                binding: binding.name.to_string(),
                context: None,
                multiple: false,
            },
            binding.range,
        );
        if let Some(range) = binding.parent_range {
            diagnostic.set_parent(range.start());
        }
        diagnostics.push(diagnostic);
    }
}

/// An unused import with its surrounding context.
#[derive(Debug)]
struct ImportBinding<'a> {
    /// Name of the binding, which for renamed imports will differ from the qualified name.
    name: &'a str,
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

/// Generate a [`Fix`] to make bindings in a statement explicit, either by adding them to `__all__`
/// or changing them from `import a` to `import a as a`.
fn fix_by_reexporting(
    checker: &Checker,
    node_id: NodeId,
    mut imports: Vec<&ImportBinding>,
    dunder_all_exprs: &[&ast::Expr],
) -> Result<Fix> {
    let statement = checker.semantic().statement(node_id);
    if imports.is_empty() {
        bail!("Expected import bindings");
    }

    imports.sort_by_key(|b| b.name);

    let edits = match dunder_all_exprs {
        [] => fix::edits::make_redundant_alias(
            imports.iter().map(|b| b.import.member_name()),
            statement,
        ),
        [dunder_all] => fix::edits::add_to_dunder_all(
            imports.iter().map(|b| b.name),
            dunder_all,
            checker.stylist(),
        ),
        _ => bail!("Cannot offer a fix when there are multiple __all__ definitions"),
    };

    // Only emit a fix if there are edits
    let mut tail = edits.into_iter();
    let head = tail.next().ok_or(anyhow!("No edits to make"))?;

    let isolation = Checker::isolation(checker.semantic().parent_statement_id(node_id));
    Ok(Fix::safe_edits(head, tail).isolate(isolation))
}
