use std::borrow::Cow;

use anyhow::Result;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{AnyImport, Imported, NodeId, ResolvedReferenceId, Scope};
use ruff_text_size::{Ranged, TextRange};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::importer::ImportedMembers;

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
    let mut errors_by_statement: FxHashMap<NodeId, Vec<ImportBinding>> = FxHashMap::default();
    let mut ignores_by_statement: FxHashMap<NodeId, Vec<ImportBinding>> = FxHashMap::default();

    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);

        let Some(import) = binding.as_any_import() else {
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
            let Some(node_id) = binding.source else {
                continue;
            };

            let import = ImportBinding {
                import,
                reference_id,
                range: binding.range(),
                parent_range: binding.parent_range(checker.semantic()),
            };

            if checker.rule_is_ignored(Rule::RuntimeImportInTypeCheckingBlock, import.start())
                || import.parent_range.is_some_and(|parent_range| {
                    checker.rule_is_ignored(
                        Rule::RuntimeImportInTypeCheckingBlock,
                        parent_range.start(),
                    )
                })
            {
                ignores_by_statement
                    .entry(node_id)
                    .or_default()
                    .push(import);
            } else {
                errors_by_statement.entry(node_id).or_default().push(import);
            }
        }
    }

    // Generate a diagnostic for every import, but share a fix across all imports within the same
    // statement (excluding those that are ignored).
    for (node_id, imports) in errors_by_statement {
        let fix = if checker.patch(Rule::RuntimeImportInTypeCheckingBlock) {
            fix_imports(checker, node_id, &imports).ok()
        } else {
            None
        };

        for ImportBinding {
            import,
            range,
            parent_range,
            ..
        } in imports
        {
            let mut diagnostic = Diagnostic::new(
                RuntimeImportInTypeCheckingBlock {
                    qualified_name: import.qualified_name(),
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
    for ImportBinding {
        import,
        range,
        parent_range,
        ..
    } in ignores_by_statement.into_values().flatten()
    {
        let mut diagnostic = Diagnostic::new(
            RuntimeImportInTypeCheckingBlock {
                qualified_name: import.qualified_name(),
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
struct ImportBinding<'a> {
    /// The qualified name of the import (e.g., `typing.List` for `from typing import List`).
    import: AnyImport<'a>,
    /// The first reference to the imported symbol.
    reference_id: ResolvedReferenceId,
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

/// Generate a [`Fix`] to remove runtime imports from a type-checking block.
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
    let remove_import_edit = autofix::edits::remove_unused_imports(
        member_names.iter().map(AsRef::as_ref),
        statement,
        parent,
        checker.locator(),
        checker.stylist(),
        checker.indexer(),
    )?;

    // Step 2) Add the import to the top-level.
    let add_import_edit = checker.importer().runtime_import_edit(
        &ImportedMembers {
            statement,
            names: member_names.iter().map(AsRef::as_ref).collect(),
        },
        at,
    )?;

    Ok(
        Fix::suggested_edits(remove_import_edit, add_import_edit.into_edits()).isolate(
            Checker::isolation(checker.semantic().parent_statement_id(node_id)),
        ),
    )
}
