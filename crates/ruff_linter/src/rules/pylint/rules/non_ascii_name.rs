use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of non-ASCII characters in symbol names.
///
/// ## Why is this bad?
/// Pylint discourages the use of non-ASCII characters in symbol names as
/// they can cause confusion and compatibility issues.
///
/// ## References
/// - [PEP 672](https://peps.python.org/pep-0672/)
#[violation]
pub struct NonAsciiName;

impl Violation for NonAsciiName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Symbol name contains a non-ASCII character, consider renaming it.")
    }
}

/// PLC2401
pub(crate) fn non_ascii_name(checker: &mut Checker, target: &Expr) {
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if id.is_ascii() {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(NonAsciiName, target.range()));
}
