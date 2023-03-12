use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct DocstringInStub;

impl Violation for DocstringInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstrings should not be included in stubs")
    }
}

/// PYI021
pub fn docstring_in_stubs(checker: &mut Checker, docstring: Option<&Expr>) {
    if let Some(docstr) = &docstring {
        checker
            .diagnostics
            .push(Diagnostic::new(DocstringInStub, Range::from(*docstr)));
    }
}
