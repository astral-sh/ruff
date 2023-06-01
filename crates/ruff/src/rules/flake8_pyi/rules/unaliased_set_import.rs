use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::StmtImportFrom;

/// ## What it does
/// Checks if collections.abc.Set is imported without being aliased
///
/// ## Why is this bad?
/// It is easily confused with `builtins.set`. In order to avoid this confusion it is best to alias
/// collections.abc.Set to AbstractSet
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
pub struct UnaliasedSetImport;

impl AlwaysAutofixableViolation for UnaliasedSetImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Always alias collections.abc.Set when importing it, so as to avoid confusion with builtins.set")
    }

    fn autofix_title(&self) -> String {
        format!("Alias `Set` to `AbstractSet`")
    }
}

///PYI025
pub(crate) fn unaliased_set_import(checker: &mut Checker, stmt: &StmtImportFrom) {
    if let Some(module_id) = &stmt.module {
        if module_id.as_str() != "collections.abc" {
            return;
        }
    }

    for name in &stmt.names {
        if name.name.as_str() == "Set" && name.asname.is_none() {
            let mut diagnostic = Diagnostic::new(UnaliasedSetImport, name.range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::insertion(
                    " as AbstractSet".to_string(),
                    name.range.end(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
