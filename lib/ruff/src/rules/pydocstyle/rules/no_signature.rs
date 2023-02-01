use rustpython_ast::StmtKind;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::Diagnostic;
use crate::violations;

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
        violations::NoSignature,
        Range::from_located(docstring.expr),
    ));
}
