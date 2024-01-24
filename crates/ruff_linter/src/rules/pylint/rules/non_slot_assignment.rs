use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignments to attributes that are not defined in `__slots__`.
///
/// ## Why is this bad?
/// When using `__slots__`, only the specified attributes are allowed.
/// Attempting to assign to an attribute that is not defined in `__slots__`
/// will result in an `AttributeError` at runtime.
///
/// ## Known problems
/// This rule can't detect `__slots__` implementations in superclasses, and
/// so limits its analysis to classes that inherit from (at most) `object`.
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
///     __slots__ = ("name", "surname")
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
pub struct NonSlotAssignment {
    name: String,
}

impl Violation for NonSlotAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonSlotAssignment { name } = self;
        format!("Attribute `{name}` is not defined in class's `__slots__`")
    }
}

/// E0237
pub(crate) fn non_slot_assignment(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    // If the class inherits from another class (aside from `object`), then it's possible that
    // the parent class defines the relevant `__slots__`.
    if !class_def.bases().iter().all(|base| {
        checker
            .semantic()
            .resolve_call_path(base)
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "object"]))
    }) {
        return;
    }

    for attribute in is_attributes_not_in_slots(&class_def.body) {
        checker.diagnostics.push(Diagnostic::new(
            NonSlotAssignment {
                name: attribute.name.to_string(),
            },
            attribute.range(),
        ));
    }
}

#[derive(Debug)]
struct AttributeAssignment<'a> {
    /// The name of the attribute that is assigned to.
    name: &'a str,
    /// The range of the attribute that is assigned to.
    range: TextRange,
}

impl Ranged for AttributeAssignment<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Return a list of attributes that are assigned to but not included in `__slots__`.
fn is_attributes_not_in_slots(body: &[Stmt]) -> Vec<AttributeAssignment> {
    // First, collect all the attributes that are assigned to `__slots__`.
    let mut slots = FxHashSet::default();
    for statement in body {
        match statement {
            // Ex) `__slots__ = ("name",)`
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                let [Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() else {
                    continue;
                };

                if id == "__slots__" {
                    slots.extend(slots_attributes(value));
                }
            }

            // Ex) `__slots__: Tuple[str, ...] = ("name",)`
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value: Some(value),
                ..
            }) => {
                let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                if id == "__slots__" {
                    slots.extend(slots_attributes(value));
                }
            }

            // Ex) `__slots__ += ("name",)`
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                if id == "__slots__" {
                    slots.extend(slots_attributes(value));
                }
            }
            _ => {}
        }
    }

    if slots.is_empty() {
        return vec![];
    }

    // Second, find any assignments that aren't included in `__slots__`.
    let mut assignments = vec![];
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) = statement else {
            continue;
        };

        if name == "__init__" {
            for statement in body {
                match statement {
                    // Ex) `self.name = name`
                    Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                        let [Expr::Attribute(attribute)] = targets.as_slice() else {
                            continue;
                        };
                        let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() else {
                            continue;
                        };
                        if id == "self" && !slots.contains(attribute.attr.as_str()) {
                            assignments.push(AttributeAssignment {
                                name: &attribute.attr,
                                range: attribute.range(),
                            });
                        }
                    }

                    // Ex) `self.name: str = name`
                    Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                        let Expr::Attribute(attribute) = target.as_ref() else {
                            continue;
                        };
                        let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() else {
                            continue;
                        };
                        if id == "self" && !slots.contains(attribute.attr.as_str()) {
                            assignments.push(AttributeAssignment {
                                name: &attribute.attr,
                                range: attribute.range(),
                            });
                        }
                    }

                    // Ex) `self.name += name`
                    Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                        let Expr::Attribute(attribute) = target.as_ref() else {
                            continue;
                        };
                        let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() else {
                            continue;
                        };
                        if id == "self" && !slots.contains(attribute.attr.as_str()) {
                            assignments.push(AttributeAssignment {
                                name: &attribute.attr,
                                range: attribute.range(),
                            });
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    assignments
}

/// Return an iterator over the attributes enumerated in the given `__slots__` value.
fn slots_attributes(expr: &Expr) -> impl Iterator<Item = &str> {
    // Ex) `__slots__ = ("name",)`
    let elts_iter = match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::List(ast::ExprList { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => Some(elts.iter().filter_map(|elt| match elt {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => Some(value.to_str()),
            _ => None,
        })),
        _ => None,
    };

    // Ex) `__slots__ = {"name": ...}`
    let keys_iter = match expr {
        Expr::Dict(ast::ExprDict { keys, .. }) => Some(keys.iter().filter_map(|key| match key {
            Some(Expr::StringLiteral(ast::ExprStringLiteral { value, .. })) => Some(value.to_str()),
            _ => None,
        })),
        _ => None,
    };

    elts_iter
        .into_iter()
        .flatten()
        .chain(keys_iter.into_iter().flatten())
}
