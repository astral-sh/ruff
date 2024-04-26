use std::borrow::Cow;
use std::iter;

use anyhow::{bail, Result};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Applicability, Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{AnyImport, Exceptions, Imported, NodeId, Scope};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix;
use crate::registry::Rule;
use crate::rules::isort;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum UnusedImportContext {
    ExceptHandler,
    Init,
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
            Some(UnusedImportContext::Init) => {
                format!(
                    "`{name}` imported but unused; consider removing, adding to `__all__`, or using a redundant alias"
                )
            }
            None => format!("`{name}` imported but unused"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnusedImport { name, multiple, .. } = self;
        Some(if *multiple {
            "Remove unused import".to_string()
        } else {
            format!("Remove unused import: `{name}`")
        })
    }
}

/// Categories of imports that we care about for this rule.
pub enum ImportCat {
    StdLib,
    FirstParty,
    ThirdParty,
}

/// Like [`isort::categorize`] but only returns categories relevant to this rule. Returns `None`
/// when this rule does not apply (e.g. `from __future__ …`).
fn categorize(checker: &Checker, qualified_name: &str) -> Option<ImportCat> {
    use isort::{ImportSection, ImportType};
    match isort::categorize(
        qualified_name,
        None,
        &checker.settings.src,
        checker.package(),
        checker.settings.isort.detect_same_package,
        &checker.settings.isort.known_modules,
        checker.settings.target_version,
        checker.settings.isort.no_sections,
        &checker.settings.isort.section_order,
        &checker.settings.isort.default_section,
    ) {
        // this rule doesn't apply
        ImportSection::Known(ImportType::Future) => None,
        // stdlib
        ImportSection::Known(ImportType::StandardLibrary) => Some(ImportCat::StdLib),
        // first party
        ImportSection::Known(ImportType::FirstParty) => Some(ImportCat::FirstParty),
        ImportSection::Known(ImportType::LocalFolder) => Some(ImportCat::FirstParty),
        // third party
        ImportSection::Known(ImportType::ThirdParty) => Some(ImportCat::ThirdParty),
        ImportSection::UserDefined(_) => Some(ImportCat::ThirdParty),
    }
}

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

        let Some(category) = categorize(&checker, &import.qualified_name().to_string()) else {
            continue;
        };

        let import = ImportBinding {
            import,
            range: binding.range(),
            parent_range: binding.parent_range(checker.semantic()),
            category,
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

    // Generate a diagnostic for every import, but share a fix across all imports within the same
    // statement (excluding those that are ignored).
    for ((node_id, exceptions), imports) in unused {
        let in_except_handler =
            exceptions.intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);
        let multiple = imports.len() > 1;
        // FIXME: rename "imports" to "bindings"
        // FIXME: rename "node_id" to "import_statement"

        // divide bindings in the import statement by how we want to fix them
        let (to_explicit, to_remove): (Vec<_>, Vec<_>) =
            imports
                .into_iter()
                .partition(|ImportBinding { category, .. }| {
                    in_init && *category == ImportCat::FirstParty
                    // FIXME: make an "is first party" predicate
                });

        let fix = if (!in_init || fix_init) && !in_except_handler {
            fix_imports(checker, node_id, &imports, in_init).ok()
        } else {
            None
        };

        for binding in imports {
            let mut diagnostic = Diagnostic::new(
                UnusedImport {
                    name: binding.import.qualified_name().to_string(),
                    context: if in_except_handler {
                        Some(UnusedImportContext::ExceptHandler)
                    } else if in_init {
                        Some(UnusedImportContext::Init)
                    } else {
                        None
                    },
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
        category,
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
    /// The origin of the import.
    category: ImportCat,
}

impl Ranged for ImportBinding<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Generate a [`Fix`] to remove unused imports from a statement.
fn fix_by_removing_imports_from_statement(
    checker: &Checker,
    node_id: NodeId,
    imports: &[ImportBinding],
    in_init: bool,
) -> Result<Fix> {
    if 0 == imports.len() {
        bail!("Expected import bindings");
    }
    let statement = checker.semantic().statement(node_id);
    let parent = checker.semantic().parent_statement(node_id);

    let member_names: Vec<Cow<'_, str>> = imports
        .iter()
        .map(|ImportBinding { import, .. }| import)
        .map(Imported::member_name)
        .collect();

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

/// Generate a [`Fix`] to make bindings in a statement explicit, either by moving them to `__all__`
/// or changing them from `import a` to `import a as a`.
fn fix_by_making_explicit(
    checker: &Checker,
    node_id: NodeId,
    imports: &[ImportBinding],
    export_list: Option<NodeId>,
) -> Option<Fix> {
    let statement = checker.semantic().statement(node_id);
    let parent = checker.semantic().parent_statement(node_id);

    let member_names = imports
        .iter()
        .map(|binding| binding.import.member_name())
        .collect::<Vec<_>>();

    // if export_list:NodeId is given, generate edits to remove from the import AND edits to add to
    // __all__ and combine them into a single fix
    let edits = match export_list {
        Some(export_list) => {
            // TODO: generate explicits by writing an edit function using `export_list` argument
            //fix::edits::make_exports_explicit(explicits, export_list)
            todo!()
        }
        // TODO: double check that this doesn't emit an edit when the list is empty
        None => fix::edits::make_imports_explicit(
            member_names.iter().map(AsRef::as_ref),
            statement,
            checker.locator(),
        ),
    };

    // Only emit a fix if there are edits
    let [_head, _tail @ ..] = &edits[..] else {
        return None;
    };
    let mut edits_iter = edits.into_iter();
    let first = edits_iter.next()?;

    let isolation = Checker::isolation(checker.semantic().parent_statement_id(node_id));
    Some(Fix::safe_edits(first, edits_iter).isolate(isolation))
}
