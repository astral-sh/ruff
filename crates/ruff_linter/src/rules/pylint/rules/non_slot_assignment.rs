use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, Expr, ExprAttribute, ExprKind, ExprName, StmtClassDef, StmtKind, Suite,
};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "v0.1.15", category = Category::Pedantic)]
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
pub(crate) fn non_slot_assignment(checker: &Checker, class_def: StmtClassDef<'_>) {
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

    for attribute in is_attributes_not_in_slots(class_def.body()) {
        checker.report_diagnostic(
            NonSlotAssignment {
                name: attribute.name.to_string(),
            },
            attribute.range(),
        );
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
fn is_attributes_not_in_slots(body: Suite<'_>) -> Vec<AttributeAssignment<'_>> {
    // First, collect all the attributes that are assigned to `__slots__`.
    let mut slots = FxHashSet::default();
    for statement in body {
        match statement.classify() {
            // Ex) `__slots__ = ("name",)`
            StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();
                let value = stmt_assign.value();

                let Some([ast::ExprFields::Name(ast::ExprNameFields { id, .. })]) =
                    targets.as_array().map(|items| items.map(Expr::fields))
                else {
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
            StmtKind::AnnAssign(stmt_ann_assign) if let Some(value) = stmt_ann_assign.value() => {
                let target = stmt_ann_assign.target();

                let ExprKind::Name(name_node) = target.classify() else {
                    continue;
                };
                let id = name_node.id();

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
            StmtKind::AugAssign(stmt_aug_assign) => {
                let target = stmt_aug_assign.target();
                let value = stmt_aug_assign.value();

                let ExprKind::Name(name_node) = target.classify() else {
                    continue;
                };
                let id = name_node.id();

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
        let StmtKind::FunctionDef(function_def) = statement.classify() else {
            continue;
        };
        let decorator_list = function_def.decorator_list();

        for decorator in decorator_list {
            let Some(ast::ExprAttributeFields { value, attr, .. }) = decorator
                .expression()
                .as_attribute_expr()
                .map(ExprAttribute::fields)
            else {
                continue;
            };

            if attr == "setter" {
                let Some(ast::ExprNameFields { id, .. }) =
                    value.as_name_expr().map(ExprName::fields)
                else {
                    continue;
                };
                slots.insert(id.as_str());
            }
        }
    }

    // Second, find any assignments that aren't included in `__slots__`.
    let mut assignments = vec![];
    for statement in body {
        let StmtKind::FunctionDef(function_def) = statement.classify() else {
            continue;
        };
        let name = function_def.name();
        let body = function_def.body();

        if name == "__init__" {
            for statement in body {
                match statement.classify() {
                    // Ex) `self.name = name`
                    StmtKind::Assign(stmt_assign) => {
                        let targets = stmt_assign.targets();

                        let Some([ExprKind::Attribute(attribute)]) =
                            targets.as_array().map(|items| items.map(Expr::classify))
                        else {
                            continue;
                        };
                        let ExprKind::Name(name_node) = attribute.value().classify() else {
                            continue;
                        };
                        let id = name_node.id();
                        if id == "self" && !slots.contains(attribute.attr().as_str()) {
                            assignments.push(AttributeAssignment {
                                name: attribute.attr().as_str(),
                                range: attribute.range(),
                            });
                        }
                    }

                    // Ex) `self.name: str = name`
                    StmtKind::AnnAssign(stmt_ann_assign) => {
                        let target = stmt_ann_assign.target();

                        let ExprKind::Attribute(attribute) = target.classify() else {
                            continue;
                        };
                        let ExprKind::Name(name_node) = attribute.value().classify() else {
                            continue;
                        };
                        let id = name_node.id();
                        if id == "self" && !slots.contains(attribute.attr().as_str()) {
                            assignments.push(AttributeAssignment {
                                name: attribute.attr().as_str(),
                                range: attribute.range(),
                            });
                        }
                    }

                    // Ex) `self.name += name`
                    StmtKind::AugAssign(stmt_aug_assign) => {
                        let target = stmt_aug_assign.target();

                        let ExprKind::Attribute(attribute) = target.classify() else {
                            continue;
                        };
                        let ExprKind::Name(name_node) = attribute.value().classify() else {
                            continue;
                        };
                        let id = name_node.id();
                        if id == "self" && !slots.contains(attribute.attr().as_str()) {
                            assignments.push(AttributeAssignment {
                                name: attribute.attr().as_str(),
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
fn slots_attributes(expr: Expr<'_>) -> impl Iterator<Item = Option<&str>> {
    // Ex) `__slots__ = ("name",)`
    let elts_iter = match expr.fields() {
        ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. })
        | ast::ExprFields::List(ast::ExprListFields { elts, .. })
        | ast::ExprFields::Set(ast::ExprSetFields { elts, .. }) => {
            Some(elts.iter().map(|elt| match elt.classify() {
                ExprKind::StringLiteral(expr_string_literal) => {
                    let value = expr_string_literal.value();
                    Some(value.to_str())
                }
                _ => None,
            }))
        }
        _ => None,
    };

    // Ex) `__slots__ = {"name": ...}`
    let keys_iter = match expr.classify() {
        ExprKind::Dict(dict) => Some(dict.iter_keys().map(|key| match key.map(Expr::fields) {
            Some(ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields {
                value, ..
            })) => Some(value.to_str()),
            _ => None,
        })),
        _ => None,
    };

    elts_iter
        .into_iter()
        .flatten()
        .chain(keys_iter.into_iter().flatten())
}
