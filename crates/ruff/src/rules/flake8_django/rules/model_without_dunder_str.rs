use rustpython_parser::ast::{Constant, Expr, StmtKind};
use rustpython_parser::ast::{ExprKind, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks that `__str__` method is defined in Django models.
///
/// ## Why is this bad?
/// Django models should define `__str__` method to return a string representation
/// of the model instance, as Django calls this method to display the object in
/// the Django Admin and elsewhere.
///
/// Models without `__str__` method will display a non-meaningful representation
/// of the object in the Django Admin.
///
/// ## Example
/// ```python
/// from django.db import models
///
///
/// class MyModel(models.Model):
///     field = models.CharField(max_length=255)
/// ```
///
/// Use instead:
/// ```python
/// from django.db import models
///
///
/// class MyModel(models.Model):
///     field = models.CharField(max_length=255)
///
///     def __str__(self):
///         return f"{self.field}"
/// ```
#[violation]
pub struct DjangoModelWithoutDunderStr;

impl Violation for DjangoModelWithoutDunderStr {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Model does not define `__str__` method")
    }
}

/// DJ008
pub fn model_without_dunder_str(
    checker: &Checker,
    bases: &[Expr],
    body: &[Stmt],
    class_location: &Stmt,
) -> Option<Diagnostic> {
    if !checker_applies(checker, bases, body) {
        return None;
    }
    if !has_dunder_method(body) {
        return Some(Diagnostic::new(
            DjangoModelWithoutDunderStr,
            Range::from(class_location),
        ));
    }
    None
}

fn has_dunder_method(body: &[Stmt]) -> bool {
    body.iter().any(|val| match &val.node {
        StmtKind::FunctionDef { name, .. } => {
            if name == "__str__" {
                return true;
            }
            false
        }
        _ => false,
    })
}

fn checker_applies(checker: &Checker, bases: &[Expr], body: &[Stmt]) -> bool {
    for base in bases.iter() {
        if is_model_abstract(body) {
            continue;
        }
        if helpers::is_model(&checker.ctx, base) {
            return true;
        }
    }
    false
}

/// Check if class is abstract, in terms of Django model inheritance.
fn is_model_abstract(body: &[Stmt]) -> bool {
    for element in body.iter() {
        let StmtKind::ClassDef {name, body, ..} = &element.node else {
            continue
        };
        if name != "Meta" {
            continue;
        }
        for element in body.iter() {
            let StmtKind::Assign {targets, value, ..} = &element.node else {
                continue;
            };
            for target in targets.iter() {
                let ExprKind::Name {id , ..} = &target.node else {
                    continue;
                };
                if id != "abstract" {
                    continue;
                }
                let ExprKind::Constant{value: Constant::Bool(true), ..} = &value.node else {
                    continue;
                };
                return true;
            }
        }
    }
    false
}
