use std::borrow::Cow;

use anyhow::Result;
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Imported, NodeId, Scope};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::fix;
use crate::importer::ImportedMembers;
use crate::rules::flake8_type_checking::helpers::{filter_contained, quote_annotation};
use crate::rules::flake8_type_checking::imports::ImportBinding;

/// ## What it does
/// Checks for runtime imports defined in a type-checking block.
///
/// ## Why is this bad?
/// The type-checking block is not executed at runtime, so the import will not
/// be available at runtime.
///
/// If [`lint.flake8-type-checking.quote-annotations`] is set to `true`,
/// annotations will be wrapped in quotes if doing so would enable the
/// corresponding import to remain in the type-checking block.
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
/// ## Options
/// - `lint.flake8-type-checking.quote-annotations`
///
/// ## References
/// - [PEP 535](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct RuntimeImportInTypeCheckingBlock {
    qualified_name: String,
    strategy: Strategy,
}

impl Violation for RuntimeImportInTypeCheckingBlock {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            qualified_name,
            strategy,
        } = self;
        match strategy {
            Strategy::MoveImport => format!(
                "Move import `{qualified_name}` out of type-checking block. Import is used for more than type hinting."
            ),
            Strategy::QuoteUsages => format!(
                "Quote references to `{qualified_name}`. Import is in a type-checking block."
            ),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Self { strategy, .. } = self;
        match strategy {
            Strategy::MoveImport => Some("Move out of type-checking block".to_string()),
            Strategy::QuoteUsages => Some("Quote references".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum Action {
    /// The import should be moved out of the type-checking block.
    Move,
    /// All usages of the import should be wrapped in quotes.
    Quote,
    /// The import should be ignored.
    Ignore,
}

/// TCH004
pub(crate) fn runtime_import_in_type_checking_block(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Collect all runtime imports by statement.
    let mut actions: FxHashMap<(NodeId, Action), Vec<ImportBinding>> = FxHashMap::default();

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
                    .in_runtime_context()
            })
        {
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

            if checker.rule_is_ignored(Rule::RuntimeImportInTypeCheckingBlock, import.start())
                || import.parent_range.is_some_and(|parent_range| {
                    checker.rule_is_ignored(
                        Rule::RuntimeImportInTypeCheckingBlock,
                        parent_range.start(),
                    )
                })
            {
                actions
                    .entry((node_id, Action::Ignore))
                    .or_default()
                    .push(import);
            } else {
                // Determine whether the member should be fixed by moving the import out of the
                // type-checking block, or by quoting its references.
                if checker.settings.flake8_type_checking.quote_annotations
                    && binding.references().all(|reference_id| {
                        let reference = checker.semantic().reference(reference_id);
                        reference.in_typing_context() || reference.in_runtime_evaluated_annotation()
                    })
                {
                    actions
                        .entry((node_id, Action::Quote))
                        .or_default()
                        .push(import);
                } else {
                    actions
                        .entry((node_id, Action::Move))
                        .or_default()
                        .push(import);
                }
            }
        }
    }

    for ((node_id, action), imports) in actions {
        match action {
            // Generate a diagnostic for every import, but share a fix across all imports within the same
            // statement (excluding those that are ignored).
            Action::Move => {
                let fix = move_imports(checker, node_id, &imports).ok();

                for ImportBinding {
                    import,
                    range,
                    parent_range,
                    ..
                } in imports
                {
                    let mut diagnostic = Diagnostic::new(
                        RuntimeImportInTypeCheckingBlock {
                            qualified_name: import.qualified_name().to_string(),
                            strategy: Strategy::MoveImport,
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

            // Generate a diagnostic for every import, but share a fix across all imports within the same
            // statement (excluding those that are ignored).
            Action::Quote => {
                let fix = quote_imports(checker, node_id, &imports).ok();

                for ImportBinding {
                    import,
                    range,
                    parent_range,
                    ..
                } in imports
                {
                    let mut diagnostic = Diagnostic::new(
                        RuntimeImportInTypeCheckingBlock {
                            qualified_name: import.qualified_name().to_string(),
                            strategy: Strategy::QuoteUsages,
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
            Action::Ignore => {
                for ImportBinding {
                    import,
                    range,
                    parent_range,
                    ..
                } in imports
                {
                    let mut diagnostic = Diagnostic::new(
                        RuntimeImportInTypeCheckingBlock {
                            qualified_name: import.qualified_name().to_string(),
                            strategy: Strategy::MoveImport,
                        },
                        range,
                    );
                    if let Some(range) = parent_range {
                        diagnostic.set_parent(range.start());
                    }
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// Generate a [`Fix`] to quote runtime usages for imports in a type-checking block.
fn quote_imports(checker: &Checker, node_id: NodeId, imports: &[ImportBinding]) -> Result<Fix> {
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
                            checker.locator(),
                            checker.stylist(),
                            checker.generator(),
                        ))
                    } else {
                        None
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );

    let mut rest = quote_reference_edits.into_iter();
    let head = rest.next().expect("Expected at least one reference");
    Ok(Fix::unsafe_edits(head, rest).isolate(Checker::isolation(
        checker.semantic().parent_statement_id(node_id),
    )))
}

/// Generate a [`Fix`] to remove runtime imports from a type-checking block.
fn move_imports(checker: &Checker, node_id: NodeId, imports: &[ImportBinding]) -> Result<Fix> {
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

    // Step 2) Add the import to the top-level.
    let add_import_edit = checker.importer().runtime_import_edit(
        &ImportedMembers {
            statement,
            names: member_names.iter().map(AsRef::as_ref).collect(),
        },
        at,
    )?;

    Ok(
        Fix::unsafe_edits(remove_import_edit, add_import_edit.into_edits()).isolate(
            Checker::isolation(checker.semantic().parent_statement_id(node_id)),
        ),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    /// The import should be moved out of the type-checking block.
    ///
    /// This is required when at least one reference to the symbol is in a runtime-required context.
    /// For example, given `from foo import Bar`, `x = Bar()` would be runtime-required.
    MoveImport,
    /// All usages of the import should be wrapped in quotes.
    ///
    /// This is acceptable when all references to the symbol are in a runtime-evaluated, but not
    /// runtime-required context. For example, given `from foo import Bar`, `x: Bar` would be
    /// runtime-evaluated, but not runtime-required.
    QuoteUsages,
}
