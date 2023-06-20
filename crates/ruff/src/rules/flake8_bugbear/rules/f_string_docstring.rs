use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;

use crate::checkers::ast::Checker;

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
    let Expr::JoinedStr ( _) = value.as_ref() else {
        return;
    };
    checker
        .diagnostics
        .push(Diagnostic::new(FStringDocstring, stmt.identifier()));
}
