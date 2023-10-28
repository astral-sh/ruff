use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Alias, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for import statements outside of the module level.
///
/// ## Why is this bad?
/// Module imports should be grouped at the top of the file at the module level
/// as required by PEP-8.
///
/// https://peps.python.org/pep-0008/#imports
#[violation]
pub struct ImportOutsideToplevel {
    names: String,
}

impl Violation for ImportOutsideToplevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { names } = self;
        format!("Import outside toplevel ({names})")
    }
}

/// C0415
pub(crate) fn import_outside_toplevel(checker: &mut Checker, stmt: &Stmt, names: &[Alias]) {
    if !checker.semantic().current_scope().kind.is_module() {
        let names: String = names
            .iter()
            .map(|name| name.name.clone().to_string())
            .collect::<Vec<String>>()
            .join(", ");

        checker.diagnostics.push(Diagnostic::new(
            ImportOutsideToplevel { names },
            stmt.range(),
        ));
    }
}
