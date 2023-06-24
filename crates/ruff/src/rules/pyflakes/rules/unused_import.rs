use anyhow::Result;
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Exceptions, NodeId, Scope};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::Rule;

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
/// - `pyflakes.extend-generics`
///
/// ## References
/// - [Python documentation: `import`](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation: `importlib.util.find_spec`](https://docs.python.org/3/library/importlib.html#importlib.util.find_spec)
#[violation]
pub struct UnusedImport {
    name: String,
    context: Option<UnusedImportContext>,
    multiple: bool,
}

impl Violation for UnusedImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

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
                    "`{name}` imported but unused; consider adding to `__all__` or using a redundant alias"
                )
            }
            None => format!("`{name}` imported but unused"),
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let UnusedImport { name, multiple, .. } = self;
        Some(if *multiple {
            "Remove unused import".to_string()
        } else {
            format!("Remove unused import: `{name}`")
        })
    }
}

pub(crate) fn unused_import(checker: &Checker, scope: &Scope, diagnostics: &mut Vec<Diagnostic>) {
    // Collect all unused imports by statement.
    let mut unused: FxHashMap<(NodeId, Exceptions), Vec<Import>> = FxHashMap::default();
    let mut ignored: FxHashMap<(NodeId, Exceptions), Vec<Import>> = FxHashMap::default();

    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);

        if binding.is_used()
            || binding.is_explicit_export()
            || binding.is_nonlocal()
            || binding.is_global()
        {
            continue;
        }

        let Some(qualified_name) = binding.qualified_name() else {
            continue;
        };

        let Some(stmt_id) = binding.source else {
            continue;
        };

        let import = Import {
            qualified_name,
            range: binding.range,
            parent_range: binding.parent_range(checker.semantic()),
        };

        if checker.rule_is_ignored(Rule::UnusedImport, import.range.start())
            || import.parent_range.map_or(false, |parent_range| {
                checker.rule_is_ignored(Rule::UnusedImport, parent_range.start())
            })
        {
            ignored
                .entry((stmt_id, binding.exceptions))
                .or_default()
                .push(import);
        } else {
            unused
                .entry((stmt_id, binding.exceptions))
                .or_default()
                .push(import);
        }
    }

    let in_init =
        checker.settings.ignore_init_module_imports && checker.path().ends_with("__init__.py");

    // Generate a diagnostic for every import, but share a fix across all imports within the same
    // statement (excluding those that are ignored).
    for ((stmt_id, exceptions), imports) in unused {
        let in_except_handler =
            exceptions.intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);
        let multiple = imports.len() > 1;

        let fix = if !in_init && !in_except_handler && checker.patch(Rule::UnusedImport) {
            fix_imports(checker, stmt_id, &imports).ok()
        } else {
            None
        };

        for Import {
            qualified_name,
            range,
            parent_range,
        } in imports
        {
            let mut diagnostic = Diagnostic::new(
                UnusedImport {
                    name: qualified_name.to_string(),
                    context: if in_except_handler {
                        Some(UnusedImportContext::ExceptHandler)
                    } else if in_init {
                        Some(UnusedImportContext::Init)
                    } else {
                        None
                    },
                    multiple,
                },
                range,
            );
            if let Some(range) = parent_range {
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
    for Import {
        qualified_name,
        range,
        parent_range,
    } in ignored.into_values().flatten()
    {
        let mut diagnostic = Diagnostic::new(
            UnusedImport {
                name: qualified_name.to_string(),
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
struct Import<'a> {
    /// The qualified name of the import (e.g., `typing.List` for `from typing import List`).
    qualified_name: &'a str,
    /// The trimmed range of the import (e.g., `List` in `from typing import List`).
    range: TextRange,
    /// The range of the import's parent statement.
    parent_range: Option<TextRange>,
}

/// Generate a [`Fix`] to remove unused imports from a statement.
fn fix_imports(checker: &Checker, stmt_id: NodeId, imports: &[Import]) -> Result<Fix> {
    let stmt = checker.semantic().stmts[stmt_id];
    let parent = checker.semantic().stmts.parent(stmt);
    let edit = autofix::edits::remove_unused_imports(
        imports
            .iter()
            .map(|Import { qualified_name, .. }| *qualified_name),
        stmt,
        parent,
        checker.locator,
        checker.stylist,
        checker.indexer,
    )?;
    Ok(Fix::automatic(edit).isolate(checker.isolation(parent)))
}
