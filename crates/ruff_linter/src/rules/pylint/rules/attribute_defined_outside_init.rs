use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for attributes that are defined outside the `__init__` method.
///
/// ## Why is this bad?
/// Attributes should be defined in `__init__` to make the object's structure
/// clear and predictable. Defining attributes outside `__init__` can make the
/// code harder to understand and maintain, as the instance attributes are not
/// immediately visible when the object is created.
///
/// ## Known problems
/// This rule can't detect attribute definitions in superclasses, and
/// so limits its analysis to classes that inherit from (at most) `object`.
/// This rule also ignores decorated classes since decorators may define
/// attributes dynamically.
///
/// ## Example
/// ```python
/// class Student:
///     def register(self):
///         self.is_registered = True  # [attribute-defined-outside-init]
/// ```
///
/// Use instead:
/// ```python
/// class Student:
///     def __init__(self):
///         self.is_registered = False
///
///     def register(self):
///         self.is_registered = True
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AttributeDefinedOutsideInit {
    name: String,
}

impl Violation for AttributeDefinedOutsideInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AttributeDefinedOutsideInit { name } = self;
        format!("Attribute `{name}` defined outside `__init__`")
    }
}

/// PLW0201
pub(crate) fn attribute_defined_outside_init(checker: &Checker, class_def: &ast::StmtClassDef) {
    let semantic = checker.semantic();

    // Skip if the class inherits from another class (aside from `object`), as the parent
    // class might define the attribute.
    if !class_def
        .bases()
        .iter()
        .all(|base| semantic.match_builtin_expr(base, "object"))
    {
        return;
    }

    // Skip if the class has decorators, as decorators might define attributes.
    if !class_def.decorator_list.is_empty() {
        return;
    }

    for attribute in find_attributes_defined_outside_init(&class_def.body) {
        checker.report_diagnostic(
            AttributeDefinedOutsideInit {
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

/// Find attributes that are defined outside the `__init__` method.
fn find_attributes_defined_outside_init(body: &[Stmt]) -> Vec<AttributeAssignment> {
    // First, collect all attributes that are defined in `__init__`.
    let mut init_attributes = FxHashSet::default();
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) = statement else {
            continue;
        };

        if name == "__init__" {
            collect_self_attributes(body, &mut init_attributes);
        }
    }

    // Also collect property setter method names.
    let mut property_setters = FxHashSet::default();
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef {
            name: _,
            decorator_list,
            ..
        }) = statement
        else {
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
                property_setters.insert(id.as_str());
            }
        }
    }

    // Also collect attributes defined at the class level (class variables).
    for statement in body {
        match statement {
            // Ex) `attr = value`
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        init_attributes.insert(id.as_str());
                    }
                }
            }
            // Ex) `attr: Type = value`
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                    init_attributes.insert(id.as_str());
                }
            }
            _ => {}
        }
    }

    // Now, find attributes that are assigned to `self` outside of `__init__`.
    let mut outside_attributes = vec![];
    for statement in body {
        let Stmt::FunctionDef(ast::StmtFunctionDef { name, body, .. }) = statement else {
            continue;
        };

        // Skip `__init__` itself since those are allowed.
        if name == "__init__" {
            continue;
        }

        // Skip property setters since those are allowed.
        if property_setters.contains(name.as_str()) {
            continue;
        }

        for statement in body {
            match statement {
                // Ex) `self.name = name`
                Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                    for target in targets {
                        if let Expr::Attribute(attribute) = target {
                            if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                                if id == "self"
                                    && !init_attributes.contains(attribute.attr.as_str())
                                {
                                    outside_attributes.push(AttributeAssignment {
                                        name: &attribute.attr,
                                        range: attribute.range(),
                                    });
                                }
                            }
                        }
                    }
                }

                // Ex) `self.name: str = name`
                Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                    if let Expr::Attribute(attribute) = target.as_ref() {
                        if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                            if id == "self" && !init_attributes.contains(attribute.attr.as_str()) {
                                outside_attributes.push(AttributeAssignment {
                                    name: &attribute.attr,
                                    range: attribute.range(),
                                });
                            }
                        }
                    }
                }

                // Ex) `self.name += name`
                Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                    if let Expr::Attribute(attribute) = target.as_ref() {
                        if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                            if id == "self" && !init_attributes.contains(attribute.attr.as_str()) {
                                outside_attributes.push(AttributeAssignment {
                                    name: &attribute.attr,
                                    range: attribute.range(),
                                });
                            }
                        }
                    }
                }

                _ => {}
            }
        }
    }

    outside_attributes
}

/// Collect all attributes that are assigned to `self` in the given statements.
fn collect_self_attributes<'a>(body: &'a [Stmt], attributes: &mut FxHashSet<&'a str>) {
    for statement in body {
        match statement {
            // Ex) `self.name = name`
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Attribute(attribute) = target {
                        if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                            if id == "self" {
                                attributes.insert(&attribute.attr);
                            }
                        }
                    }
                }
            }

            // Ex) `self.name: str = name`
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Attribute(attribute) = target.as_ref() {
                    if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                        if id == "self" {
                            attributes.insert(&attribute.attr);
                        }
                    }
                }
            }

            // Ex) `self.name += name`
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                if let Expr::Attribute(attribute) = target.as_ref() {
                    if let Expr::Name(ast::ExprName { id, .. }) = attribute.value.as_ref() {
                        if id == "self" {
                            attributes.insert(&attribute.attr);
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
