use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::StmtClassDef;

use crate::checkers::ast::Checker;

/// ## What it does
/// Use collections.namedtuple (or typing.NamedTuple) for data classes that
/// only set attributes in an __init__ method, and do nothing else.
///
/// ## Why is this bad?
/// Using a data class with a simple __init__ method to set attributes is
/// verbose and unnecessary. Using collections.namedtuple or typing.NamedTuple
/// is more concise and idiomatic.
///
/// ## Example
///
/// ```python
/// class Point:
///    def __init__(self, x, y):
///       self.x = x
///       self.y = y
/// ```
///
/// Use instead:
/// ```python
/// from collections import namedtuple
///
/// Point = namedtuple('Point', ['x', 'y'])
/// ```
/// or:
/// ```python
/// from typing import NamedTuple
///
/// class Point(NamedTuple):
///    x: int
///    y: int
/// ```

#[violation]
pub struct SimpleInitClasses;

impl AlwaysFixableViolation for SimpleInitClasses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of a data class with a simple __init__ method to set attributes")
    }

    fn fix_title(&self) -> String {
        format!("Replace with collections.namedtuple or typing.NamedTuple")
    }
}

// B903
pub(crate) fn simple_init_classes(
    checker: &mut Checker,
    class_def: &StmtClassDef,
) -> Option<Diagnostic> {
}
