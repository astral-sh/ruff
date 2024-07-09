use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;

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

fn wrong_class_structure(body: &[ast::Stmt]) -> Option<usize> {
    // Variables to check if the class body is correct
    let mut idx = 0;
    let mut init_index = None;
    let mut has_other_methods = false;

    while idx < body.len() && !has_other_methods {
        let stmt = &body[idx];
        match stmt {
            ast::Stmt::Expr(stmt_expr) => {
                // Check if it's a string literal (potential docstring)
                if stmt_expr.value.is_string_literal_expr() {
                    idx += 1;
                }
            }
            // Check for assignment statement
            ast::Stmt::Assign(_) => {
                idx += 1;
            }
            ast::Stmt::FunctionDef(func) => {
                // Check if it's the __init__ method
                if func.name.as_str() == "__init__" {
                    init_index = Some(idx);
                    idx += 1;
                } else {
                    has_other_methods = true;
                }
            }
            _ => {
                // Unexpected statement type in class body
                has_other_methods = true;
            }
        }
    }

    if has_other_methods || init_index.is_none() {
        None
    } else {
        init_index
    }
}

// B903
pub(crate) fn simple_init_classes(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    let body = &class_def.body;

    // Ensure class body is not empty
    if body.is_empty() {
        return;
    }

    let init_index = wrong_class_structure(body);

    if init_index.is_none() {
        return;
    }

    // Now check that the "__init__" method is only doing assignments
    let init_function = body.get(init_index.unwrap());
}
