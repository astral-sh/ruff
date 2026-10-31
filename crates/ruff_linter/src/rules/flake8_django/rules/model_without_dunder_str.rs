use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::helpers::is_const_true;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{Modules, SemanticModel, analyze};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

use crate::rules::flake8_django::helpers;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.246", category = Category::Style)]
pub(crate) struct DjangoModelWithoutDunderStr;

impl Violation for DjangoModelWithoutDunderStr {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Model does not define `__str__` method".to_string()
    }
}

/// DJ008
pub(crate) fn model_without_dunder_str(
    checker: &Checker,
    class_def: flat::View<'_, ast::StmtClassDef>,
) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if !is_non_abstract_model(class_def, checker.semantic()) {
        return;
    }

    if has_dunder_method(class_def, checker.semantic()) {
        return;
    }

    checker.report_diagnostic(DjangoModelWithoutDunderStr, class_def.identifier());
}

/// Returns `true` if the class has `__str__` method.
fn has_dunder_method(
    class_def: flat::View<'_, ast::StmtClassDef>,
    semantic: &SemanticModel,
) -> bool {
    analyze::class::any_super_class(class_def, semantic, |class_def| {
        class_def.body().iter().any(|val| match val.classify() {
            flat::StmtKind::FunctionDef(stmt_function_def) => {
                let name = stmt_function_def.name();

                name == "__str__"
            }
            _ => false,
        })
    })
}

/// Returns `true` if the class is a non-abstract Django model.
fn is_non_abstract_model(
    class_def: flat::View<'_, ast::StmtClassDef>,
    semantic: &SemanticModel,
) -> bool {
    if class_def.bases().is_empty() || is_model_abstract(class_def) {
        false
    } else {
        helpers::is_model(class_def, semantic)
    }
}

/// Check if class is abstract, in terms of Django model inheritance.
fn is_model_abstract(class_def: flat::View<'_, ast::StmtClassDef>) -> bool {
    for element in class_def.body() {
        let flat::StmtKind::ClassDef(class_def_node) = element.classify() else {
            continue;
        };
        let name = class_def_node.name();
        let body = class_def_node.body();
        if name != "Meta" {
            continue;
        }
        for element in body {
            match element.classify() {
                flat::StmtKind::Assign(stmt_assign)
                    if stmt_assign.targets().iter().any(|target| {
                        is_abstract_true_assignment(target, Some(stmt_assign.value()))
                    }) =>
                {
                    return true;
                }
                flat::StmtKind::AnnAssign(stmt_ann_assign)
                    if is_abstract_true_assignment(
                        stmt_ann_assign.target(),
                        stmt_ann_assign.value(),
                    ) =>
                {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

fn is_abstract_true_assignment(
    target: flat::View<'_, Expr>,
    value: Option<flat::View<'_, Expr>>,
) -> bool {
    let flat::ExprKind::Name(name_node) = target.classify() else {
        return false;
    };
    let id = name_node.id();

    if id != "abstract" {
        return false;
    }

    let Some(value) = value else {
        return false;
    };

    if !is_const_true(value) {
        return false;
    }

    true
}
