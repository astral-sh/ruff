use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::{analyze, Modules, SemanticModel};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks that a `__str__` method is defined in Django models.
///
/// ## Why is this bad?
/// Django models should define a `__str__` method to return a string representation
/// of the model instance, as Django calls this method to display the object in
/// the Django Admin and elsewhere.
///
/// Models without a `__str__` method will display a non-meaningful representation
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
pub(crate) fn model_without_dunder_str(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if !is_non_abstract_model(class_def, checker.semantic()) {
        return;
    }

    if has_dunder_method(class_def, checker.semantic()) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        DjangoModelWithoutDunderStr,
        class_def.identifier(),
    ));
}

/// Returns `true` if the class has `__str__` method.
fn has_dunder_method(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    analyze::class::any_super_class(class_def, semantic, &|class_def| {
        class_def.body.iter().any(|val| match val {
            Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => name == "__str__",
            _ => false,
        })
    })
}

/// Returns `true` if the class is a non-abstract Django model.
fn is_non_abstract_model(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    if class_def.bases().is_empty() || is_model_abstract(class_def) {
        false
    } else {
        helpers::is_model(class_def, semantic)
    }
}

/// Check if class is abstract, in terms of Django model inheritance.
fn is_model_abstract(class_def: &ast::StmtClassDef) -> bool {
    for element in &class_def.body {
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
