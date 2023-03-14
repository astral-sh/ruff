use rustpython_parser::ast::StmtKind;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};

#[violation]
pub struct NoSignature;

impl Violation for NoSignature {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should not be the function's signature")
    }
}

/// D402
pub fn no_signature(checker: &mut Checker, docstring: &Docstring) {
    let (
        DefinitionKind::Function(parent)
        | DefinitionKind::NestedFunction(parent)
        | DefinitionKind::Method(parent)
    ) = docstring.kind else {
        return;
    };
    let StmtKind::FunctionDef { name, .. } = &parent.node else {
        return;
    };

    let body = docstring.body;

    let Some(first_line) = body.trim().universal_newlines().next() else {
        return;
    };
    if !first_line.contains(&format!("{name}(")) {
        return;
    };
    checker
        .diagnostics
        .push(Diagnostic::new(NoSignature, Range::from(docstring.expr)));
}
