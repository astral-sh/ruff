use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

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
pub(crate) fn model_without_dunder_str(
    checker: &mut Checker,
    ast::StmtClassDef {
        name,
        arguments,
        body,
        ..
    }: &ast::StmtClassDef,
) {
    if !is_non_abstract_model(arguments.as_deref(), body, checker.semantic()) {
        return;
    }
    if has_dunder_method(body) {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(DjangoModelWithoutDunderStr, name.range()));
}

fn has_dunder_method(body: &[Stmt]) -> bool {
    body.iter().any(|val| match val {
        Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => {
            if name == "__str__" {
                return true;
            }
            false
        }
        _ => false,
    })
}

fn is_non_abstract_model(
    arguments: Option<&Arguments>,
    body: &[Stmt],
    semantic: &SemanticModel,
) -> bool {
    let Some(Arguments { args: bases, .. }) = arguments else {
        return false;
    };

    if is_model_abstract(body) {
        return false;
    }

    bases.iter().any(|base| helpers::is_model(base, semantic))
}

/// Check if class is abstract, in terms of Django model inheritance.
fn is_model_abstract(body: &[Stmt]) -> bool {
    for element in body {
        let Stmt::ClassDef(ast::StmtClassDef { name, body, .. }) = element else {
            continue;
        };
        if name != "Meta" {
            continue;
        }
        for element in body {
            let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = element else {
                continue;
            };
            for target in targets {
                let Expr::Name(ast::ExprName { id, .. }) = target else {
                    continue;
                };
                if id != "abstract" {
                    continue;
                }
                if !is_const_true(value) {
                    continue;
                }
                return true;
            }
        }
    }
    false
}
