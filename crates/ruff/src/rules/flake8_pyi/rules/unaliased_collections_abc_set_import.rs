use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Imported;
use ruff_python_semantic::{Binding, BindingKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::renamer::Renamer;

/// ## What it does
/// Checks for `from collections.abc import Set` imports that do not alias
/// `Set` to `AbstractSet`.
///
/// ## Why is this bad?
/// The `Set` type in `collections.abc` is an abstract base class for set-like types.
/// It is easily confused with, and not equivalent to, the `set` builtin.
///
/// To avoid confusion, `Set` should be aliased to `AbstractSet` when imported. This
/// makes it clear that the imported type is an abstract base class, and not the
/// `set` builtin.
///
/// ## Example
/// ```python
/// from collections.abc import Set
/// ```
///
/// Use instead:
/// ```python
/// from collections.abc import Set as AbstractSet
/// ```
#[violation]
pub struct UnaliasedCollectionsAbcSetImport;

impl Violation for UnaliasedCollectionsAbcSetImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use `from collections.abc import Set as AbstractSet` to avoid confusion with the `set` builtin"
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some(format!("Alias `Set` to `AbstractSet`"))
    }
}

/// PYI025
pub(crate) fn unaliased_collections_abc_set_import(
    checker: &Checker,
    binding: &Binding,
) -> Option<Diagnostic> {
    let BindingKind::FromImport(import) = &binding.kind else {
        return None;
    };
    if !matches!(import.call_path(), ["collections", "abc", "Set"]) {
        return None;
    }

    let name = binding.name(checker.locator());
    if name == "AbstractSet" {
        return None;
    }

    let mut diagnostic = Diagnostic::new(UnaliasedCollectionsAbcSetImport, binding.range());
    if checker.patch(diagnostic.kind.rule()) {
        if checker.semantic().is_available("AbstractSet") {
            diagnostic.try_set_fix(|| {
                let scope = &checker.semantic().scopes[binding.scope];
                let (edit, rest) = Renamer::rename(name, "AbstractSet", scope, checker.semantic())?;
                Ok(Fix::suggested_edits(edit, rest))
            });
        }
    }
    Some(diagnostic)
}
