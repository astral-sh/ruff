#[allow(unused_imports)]
use std::fmt;

#[allow(unused_imports)]
use ruff_diagnostics::{Diagnostic, Violation};
#[allow(unused_imports)]
use ruff_macros::{derive_message_formats, violation};
#[allow(unused_imports)]
use ruff_python_ast::types::Range;

#[allow(unused_imports)]
use crate::checkers::ast::Checker;

#[allow(unused_imports)]
use rustpython_parser::ast::{Stmt, StmtKind};

#[violation]
pub struct UnnecessaryPassInClass;

impl Violation for UnnecessaryPassInClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Class body should not contain `pass`")
    }
}

/// PYI012
pub fn unnecessary_pass_in_class(checker: &mut Checker, body: &[Stmt]) {
    println!("{:?}", &body);
    println!("Hi");
}
