use rustpython_parser::ast::{self, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that a class that implements a `__eq__` also implements `__hash__`.
///
/// ## Why is this bad?
/// A class that overrides eq() and does not define hash() will have its hash()
/// implicitly set to None.
///
/// ## Example
/// ```python
/// class Person:
///     def __init__(self):
///         self.name = "monty"
///
///     def __eq__(self, other):
///         return isinstance(other, Person) and other.name == self.name
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     def __init__(self):
///         self.name = "monty"
///
///     def __eq__(self, other):
///         return isinstance(other, Person) and other.name == self.name
///
///     def __hash__(self):
///         return hash(self.name)
/// ```
#[violation]
pub struct EqWithoutHash;

impl Violation for EqWithoutHash {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Object does not implement `__hash__` method")
    }
}

/// W1641
pub(crate) fn object_without_hash_method(
    checker: &mut Checker,
    ast::StmtClassDef { name, body, .. }: &ast::StmtClassDef,
) {
    if !has_eq_without_hash(body) {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(EqWithoutHash, name.range()));
}

fn has_eq_without_hash(body: &[Stmt]) -> bool {
    let mut has_hash = false;
    let mut has_eq = false;

    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) = statement else {
            continue;
        };
        if name == "__hash__" {
            has_hash = true;
        } else if name == "__eq__" {
            has_eq = true;
        }
    }
    has_eq && !has_hash
}
