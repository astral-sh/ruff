use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::StmtKind;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct NoSignature;
);
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

    let Some(first_line) = body.trim().lines().next() else {
        return;
    };
    if !first_line.contains(&format!("{name}(")) {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        NoSignature,
        Range::from_located(docstring.expr),
    ));
}
