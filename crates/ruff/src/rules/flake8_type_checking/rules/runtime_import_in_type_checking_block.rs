use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::binding::Binding;

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::importer::StmtImport;
use crate::registry::AsRule;

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
    binding: &Binding,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(qualified_name) = binding.qualified_name() else {
        return;
    };

    let Some(reference_id) = binding.references.first() else {
        return;
    };

    if binding.context.is_typing()
        && binding.references().any(|reference_id| {
            checker
                .semantic_model()
                .references
                .resolve(reference_id)
                .context()
                .is_runtime()
        })
    {
        let mut diagnostic = Diagnostic::new(
            RuntimeImportInTypeCheckingBlock {
                qualified_name: qualified_name.to_string(),
            },
            binding.trimmed_range(checker.semantic_model(), checker.locator),
        );
        if let Some(range) = binding.parent_range(checker.semantic_model()) {
            diagnostic.set_parent(range.start());
        }

        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                // Step 1) Remove the import.
                // SAFETY: All non-builtin bindings have a source.
                let source = binding.source.unwrap();
                let stmt = checker.semantic_model().stmts[source];
                let parent = checker.semantic_model().stmts.parent(stmt);
                let remove_import_edit = autofix::edits::remove_unused_imports(
                    std::iter::once(qualified_name),
                    stmt,
                    parent,
                    checker.locator,
                    checker.indexer,
                    checker.stylist,
                )?;

                // Step 2) Add the import to the top-level.
                let reference = checker.semantic_model().references.resolve(*reference_id);
                let add_import_edit = checker.importer.runtime_import_edit(
                    &StmtImport {
                        stmt,
                        qualified_name,
                    },
                    reference.range().start(),
                )?;

                Ok(
                    Fix::suggested_edits(remove_import_edit, add_import_edit.into_edits())
                        .isolate(checker.isolation(parent)),
                )
            });
        }

        if checker.enabled(diagnostic.kind.rule()) {
            diagnostics.push(diagnostic);
        }
    }
}
