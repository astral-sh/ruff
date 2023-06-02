use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::binding::{
    Binding, BindingKind, FromImportation, Importation, SubmoduleImportation,
};

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
    full_name: String,
}

impl Violation for RuntimeImportInTypeCheckingBlock {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let RuntimeImportInTypeCheckingBlock { full_name } = self;
        format!(
            "Move import `{full_name}` out of type-checking block. Import is used for more than type hinting."
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
    let full_name = match &binding.kind {
        BindingKind::Importation(Importation { full_name }) => full_name,
        BindingKind::FromImportation(FromImportation { full_name }) => full_name.as_str(),
        BindingKind::SubmoduleImportation(SubmoduleImportation { full_name }) => full_name,
        _ => return,
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
                full_name: full_name.to_string(),
            },
            binding.range,
        );

        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                // Step 1) Remove the import.
                // SAFETY: All non-builtin bindings have a source.
                let source = binding.source.unwrap();
                let stmt = checker.semantic_model().stmts[source];
                let parent = checker.semantic_model().stmts.parent(stmt);
                let remove_import_edit = autofix::edits::remove_unused_imports(
                    std::iter::once(full_name),
                    stmt,
                    parent,
                    checker.locator,
                    checker.indexer,
                    checker.stylist,
                )?;

                // Step 2) Add the import to the top-level.
                let reference = checker.semantic_model().references.resolve(*reference_id);
                let add_import_edit = checker.importer.runtime_import_edit(
                    &StmtImport { stmt, full_name },
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
