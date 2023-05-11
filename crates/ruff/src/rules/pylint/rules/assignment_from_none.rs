use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Kind {
    Empty,
    NonEmpty,
    Unknown,
}

/// ## What it does
/// Checks for assigning result of a function call, where the function returns None 
/// Used when an assignment is done on a function call but the inferred function returns nothing but None argument.
///
/// ## Why is this bad?
/// This unnecessarily abstracts a potential bug by "hard-coding" a return of None
///
/// ## Example
/// ```python
/// def func():
/// return None
///
/// def foo():
///     return func()
/// ```
#[violation]
pub struct AssignmentFromNone {
    kind: Kind,
}

impl Violation for AssignmentFromNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssignmentFromNone { kind } = self;
        match kind {
            Kind::Empty => format!("Asserting on an empty string literal will never pass"),
            Kind::NonEmpty => format!("Asserting on a non-empty string literal will always pass"),
            Kind::Unknown => format!("Asserting on a string literal may have unintended results"),
        }
    }
}
/// PLE1128
pub fn assignment_from_none(checker: &mut Checker, test: &Expr) {
    println!("Running!");
    println!("{:?}", &test);
    match &test.node {
        ExprKind::Constant { value, .. } => {checker.diagnostics.push(Diagnostic::new(AssignmentFromNone{kind: Kind::Empty}, test.range()));}
        _ => {}
    }
}