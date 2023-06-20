use anyhow::Result;
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{NodeId, ReferenceId, Scope};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::importer::StmtImports;

/// ## What it does
/// Checks for runtime imports defined in a type-checking block.
///
/// ## Why is this bad?
/// The type-checking block is not executed at runtime, so the import will not
/// be available at runtime.
///
/// ## Example
/// ```python
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import foo
///
///
/// def bar() -> None:
///     foo.bar()  # raises NameError: name 'foo' is not defined
/// ```
///
/// Use instead:
/// ```python
/// import foo
///
///
/// def bar() -> None:
///     foo.bar()
/// ```
///
/// ## References
/// - [PEP 535](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct RuntimeImportInTypeCheckingBlock {
    qualified_name: String,
}

impl Violation for RuntimeImportInTypeCheckingBlock {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let RuntimeImportInTypeCheckingBlock { qualified_name } = self;
        format!(
            "Move import `{qualified_name}` out of type-checking block. Import is used for more than type hinting."
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Move out of type-checking block".to_string())
    }
}

/// TCH004
pub(crate) fn runtime_import_in_type_checking_block(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Collect all runtime imports by statement.
    let mut errors_by_statement: FxHashMap<NodeId, Vec<Import>> = FxHashMap::default();
    let mut ignores_by_statement: FxHashMap<NodeId, Vec<Import>> = FxHashMap::default();

    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);

        let Some(qualified_name) = binding.qualified_name() else {
            continue;
        };

        let Some(reference_id) = binding.references.first().copied() else {
            continue;
        };

        if binding.context.is_typing()
            && binding.references().any(|reference_id| {
                checker
                    .semantic()
                    .reference(reference_id)
                    .context()
                    .is_runtime()
            })
        {
            let Some(stmt_id) = binding.source else {
                continue;
            };

            let import = Import {
                qualified_name,
                reference_id,
                range: binding.range,
                parent_range: binding.parent_range(checker.semantic()),
            };

            if checker.rule_is_ignored(Rule::RuntimeImportInTypeCheckingBlock, import.range.start())
                || import.parent_range.map_or(false, |parent_range| {
                    checker.rule_is_ignored(
                        Rule::RuntimeImportInTypeCheckingBlock,
                        parent_range.start(),
                    )
                })
            {
                ignores_by_statement
                    .entry(stmt_id)
                    .or_default()
                    .push(import);
            } else {
                errors_by_statement.entry(stmt_id).or_default().push(import);
            }
        }
    }

    // Generate a diagnostic for every import, but share a fix across all imports within the same
    // statement (excluding those that are ignored).
    for (stmt_id, imports) in errors_by_statement {
        let fix = if checker.patch(Rule::RuntimeImportInTypeCheckingBlock) {
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
                RuntimeImportInTypeCheckingBlock {
                    qualified_name: qualified_name.to_string(),
                },
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
    for Import {
        qualified_name,
        range,
        parent_range,
        ..
    } in ignores_by_statement.into_values().flatten()
    {
        let mut diagnostic = Diagnostic::new(
            RuntimeImportInTypeCheckingBlock {
                qualified_name: qualified_name.to_string(),
            },
            range,
        );
        if let Some(range) = parent_range {
            diagnostic.set_parent(range.start());
        }
        diagnostics.push(diagnostic);
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

/// Generate a [`Fix`] to remove runtime imports from a type-checking block.
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

    // Step 2) Add the import to the top-level.
    let add_import_edit = checker.importer.runtime_import_edit(
        &StmtImports {
            stmt,
            qualified_names,
        },
        at,
    )?;

    Ok(
        Fix::suggested_edits(remove_import_edit, add_import_edit.into_edits())
            .isolate(checker.isolation(parent)),
    )
}
