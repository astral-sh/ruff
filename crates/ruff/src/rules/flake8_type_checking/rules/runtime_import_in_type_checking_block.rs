use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::binding::{
    Binding, BindingKind, FromImportation, Importation, SubmoduleImportation,
};
use ruff_python_semantic::model::SemanticModel;

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
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move import `{}` out of type-checking block. Import is used for more than type \
             hinting.",
            self.full_name
        )
    }
}

/// TCH004
pub(crate) fn runtime_import_in_type_checking_block(
    binding: &Binding,
    semantic_model: &SemanticModel,
) -> Option<Diagnostic> {
    let full_name = match &binding.kind {
        BindingKind::Importation(Importation { full_name, .. }) => full_name,
        BindingKind::FromImportation(FromImportation { full_name, .. }) => full_name.as_str(),
        BindingKind::SubmoduleImportation(SubmoduleImportation { full_name, .. }) => full_name,
        _ => return None,
    };

    if binding.context.is_typing()
        && binding.references().any(|reference_id| {
            semantic_model
                .references
                .resolve(reference_id)
                .context()
                .is_runtime()
        })
    {
        Some(Diagnostic::new(
            RuntimeImportInTypeCheckingBlock {
                full_name: full_name.to_string(),
            },
            binding.range,
        ))
    } else {
        None
    }
}
