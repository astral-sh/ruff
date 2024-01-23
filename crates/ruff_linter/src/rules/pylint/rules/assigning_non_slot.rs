use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assigning to an attribute not defined in the class slots.
///
/// ## Why is this bad?
/// When using `__slots__`, only the specified attributes are allowed.
/// Any attempt to assign other attributes will result in an error.
///
/// ## Known problems
/// Does not check for `__slots__` implementations in superclasses.
///
/// ## Example
/// ```python
/// class Student:
///     __slots__ = ("name",)
///
///     def __init__(self, name, surname):
///         self.name = name
///         self.surname = surname  # [assigning-non-slot]
///         self.setup()
///
///     def setup(self):
///         pass
/// ```
///
/// Use instead:
/// ```python
/// class Student:
///     __slots__ = ("name","surname")
///
///     def __init__(self, name, surname):
///         self.name = name
///         self.surname = surname
///         self.setup()
///
///     def setup(self):
///         pass
/// ```
#[violation]
pub struct AssigningNonSlot;

impl Violation for AssigningNonSlot {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assigning an attribute that is not defined in class slots")
    }
}

// E0237
pub(crate) fn assigning_non_slot(
    checker: &mut Checker,
    ast::StmtClassDef { name, body, .. }: &ast::StmtClassDef,
) {
    if is_attributes_not_in_slots(body) {
        checker
            .diagnostics
            .push(Diagnostic::new(AssigningNonSlot, name.range()));
    }
}

fn is_attributes_not_in_slots(body: &[Stmt]) -> bool {
    let mut has_slots = false;
    let mut slots = Vec::new();
    let mut attrs = Vec::new();
    for statement in body {
        match statement {
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                let [Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() else {
                    continue;
                };

                if id == "__slots__" {
                    has_slots = true;
                    let Expr::Tuple(ast::ExprTuple { elts, .. }) = value.as_ref() else {
                        continue;
                    };
                    for elt in elts.iter() {
                        let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = elt else {
                            continue;
                        };
                        slots.push(value.to_str());
                    }
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) => {
                if name == "__init__" {
                    for stmt in body {
                        match stmt {
                            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                                let [Expr::Attribute(ast::ExprAttribute { value, attr, .. })] =
                                    targets.as_slice()
                                else {
                                    continue;
                                };
                                let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
                                    continue;
                                };
                                if id == "self" {
                                    attrs.push(attr.as_str());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    has_slots && slots.len() != attrs.len()
}
