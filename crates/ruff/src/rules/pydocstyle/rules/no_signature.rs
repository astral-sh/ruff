use rustpython_parser::ast::{self, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_newlines::StrExt;
use ruff_python_semantic::definition::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

#[violation]
pub struct NoSignature;

impl Violation for NoSignature {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should not be the function's signature")
    }
}

/// D402
pub(crate) fn no_signature(checker: &mut Checker, docstring: &Docstring) {
    let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = docstring.definition else {
        return;
    };
    let Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) = stmt else {
        return;
    };

    let body = docstring.body();

    let Some(first_line) = body.trim().universal_newlines().next() else {
        return;
    };

    if !first_line.contains(&format!("{name}(")) {
        return;
    };

    checker
        .diagnostics
        .push(Diagnostic::new(NoSignature, docstring.range()));
}
