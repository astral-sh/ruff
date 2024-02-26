use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{identifier::Identifier, Expr, ExprAttribute, Stmt, StmtClassDef};
use std::collections::HashSet;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes that include too many class attributes.
///
/// By default, this rule allows up to seven attributes, as configured by the
/// [`lint.pylint.max-attributes`] option.
///
/// ## Why is this bad?
/// Classes with many attributes are harder to use and maintain.
///
/// Consider reducing class attributes to get a simpler and easier to use class.
///
/// ## Options
/// - `lint.pylint.max-attributes`
#[violation]
pub struct TooManyInstanceAttributes {
    current_amount: usize,
    max_amount: usize,
}

impl Violation for TooManyInstanceAttributes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyInstanceAttributes {
            current_amount,
            max_amount,
        } = self;
        format!("Too many instance attributes ({current_amount}/{max_amount})")
    }
}

fn collect_attributes<'a>(attributes: &mut HashSet<&'a str>, expr: &'a ExprAttribute) {
    if let Some(ident) = expr.value.as_name_expr() {
        if ident.id == "self" {
            attributes.insert(expr.attr.as_str());
        }
    }
}

fn extract_assigned_attributes<'a>(attributes: &mut HashSet<&'a str>, target: &'a Expr) {
    match target {
        Expr::Attribute(expr) => {
            collect_attributes(attributes, expr);
        }
        Expr::Tuple(tuple) => {
            for expr in &tuple.elts {
                if let Some(expr) = expr.as_attribute_expr() {
                    collect_attributes(attributes, expr);
                }
            }
        }
        _ => (),
    }
}

/// PLR0902
pub(crate) fn too_many_instance_attributes(
    checker: &Checker,
    class_def: &StmtClassDef,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut attributes = HashSet::new();
    for body in &class_def.body {
        match body {
            // collect attributes defined in class methods
            Stmt::FunctionDef(func_def) => {
                for stmt in &func_def.body {
                    match stmt {
                        Stmt::Assign(assign) => {
                            for target in &assign.targets {
                                extract_assigned_attributes(&mut attributes, target);
                            }
                        }
                        Stmt::AnnAssign(assign) => {
                            extract_assigned_attributes(&mut attributes, &assign.target);
                        }
                        _ => (),
                    }
                }
            }
            // collect attributes defined in class properties
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    extract_assigned_attributes(&mut attributes, target);
                }
            }
            Stmt::AnnAssign(assign) => {
                extract_assigned_attributes(&mut attributes, &assign.target);
            }
            _ => (),
        }
    }
    let num_attributes = attributes.len();
    if num_attributes > checker.settings.pylint.max_attributes {
        diagnostics.push(Diagnostic::new(
            TooManyInstanceAttributes {
                current_amount: num_attributes,
                max_amount: checker.settings.pylint.max_attributes,
            },
            class_def.identifier(),
        ));
    }
}
