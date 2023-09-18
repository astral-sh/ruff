use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for docstrings that are written via f-strings.
///
/// ## Why is this bad?
/// Python will interpret the f-string as a joined string, rather than as a
/// docstring. As such, the "docstring" will not be accessible via the
/// `__doc__` attribute, nor will it be picked up by any automated
/// documentation tooling.
///
/// ## Example
/// ```python
/// def foo():
///     f"""Not a docstring."""
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     """A docstring."""
/// ```
///
/// ## References
/// - [PEP 257](https://peps.python.org/pep-0257/)
/// - [Python documentation: Formatted string literals](https://docs.python.org/3/reference/lexical_analysis.html#f-strings)
#[violation]
pub struct FStringDocstring;

impl Violation for FStringDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "f-string used as docstring. Python will interpret this as a joined string, rather than a docstring."
        )
    }
}

/// B021
pub(crate) fn f_string_docstring(checker: &mut Checker, body: &[Stmt]) {
    let Some(stmt) = body.first() else {
        return;
    };
    let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt else {
        return;
    };
    if !value.is_f_string_expr() {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(FStringDocstring, stmt.identifier()));
}
