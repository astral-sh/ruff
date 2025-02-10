use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
#[derive(ViolationMetadata)]
pub(crate) struct NonSlotAssignment {
    name: String,
}

impl Violation for NonSlotAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonSlotAssignment { name } = self;
        format!("Attribute `{name}` is not defined in class's `__slots__`")
    }
}

/// PLE0237
pub(crate) fn non_slot_assignment(checker: &Checker, class_def: &ast::StmtClassDef) {
    let semantic = checker.semantic();

    // If the class inherits from another class (aside from `object`), then it's possible that
    // the parent class defines the relevant `__slots__`.
    if !class_def
        .bases()
        .iter()
        .all(|base| semantic.match_builtin_expr(base, "object"))
    {
        return;
    }

    for attribute in is_attributes_not_in_slots(&class_def.body) {
        checker.report_diagnostic(Diagnostic::new(
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
///
/// If the `__slots__` attribute cannot be statically determined, returns an empty vector.
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
                    for attribute in slots_attributes(value) {
                        if let Some(attribute) = attribute {
                            slots.insert(attribute);
                        } else {
                            return vec![];
                        }
                    }
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
                    for attribute in slots_attributes(value) {
                        if let Some(attribute) = attribute {
                            slots.insert(attribute);
                        } else {
                            return vec![];
                        }
                    }
                }
            }

            // Ex) `__slots__ += ("name",)`
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                if id == "__slots__" {
                    for attribute in slots_attributes(value) {
                        if let Some(attribute) = attribute {
                            slots.insert(attribute);
                        } else {
                            return vec![];
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if slots.is_empty() || slots.contains("__dict__") {
        return vec![];
    }

    // And, collect all the property name with setter.
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef { decorator_list, .. }) = statement else {
            continue;
        };

        for decorator in decorator_list {
            let Some(ast::ExprAttribute { value, attr, .. }) =
                decorator.expression.as_attribute_expr()
            else {
                continue;
            };

            if attr == "setter" {
                let Some(ast::ExprName { id, .. }) = value.as_name_expr() else {
                    continue;
                };
                slots.insert(id.as_str());
            }
        }
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
///
/// If an attribute can't be statically determined, it will be `None`.
fn slots_attributes(expr: &Expr) -> impl Iterator<Item = Option<&str>> {
    // Ex) `__slots__ = ("name",)`
    let elts_iter = match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::List(ast::ExprList { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => Some(elts.iter().map(|elt| match elt {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => Some(value.to_str()),
            _ => None,
        })),
        _ => None,
    };

    // Ex) `__slots__ = {"name": ...}`
    let keys_iter = match expr {
        Expr::Dict(dict) => Some(dict.iter_keys().map(|key| match key {
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
