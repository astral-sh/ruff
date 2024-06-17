use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_semantic::{analyze, Modules, SemanticModel};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that a `__unicode__` method is not defined for Django models.
///
/// ## Why is this bad?
/// `__unicode__` was deprecated in Python 3.0, and no longer has an effect.
///
/// ## Example
/// ```python
/// from django.db import models
///
///
/// class MyModel(models.Model):
///     def __str__(self):
///         return self.name
///
///     def __unicode__(self):
///         return self.name
/// ```
///
/// Use instead:
/// ```python
/// from django.db import models
///
///
/// class MyModel(models.Model):
///     def __str__(self):
///         return self.name
/// ```
#[violation]
pub struct DjangoModelWithDunderUnicode;

impl Violation for DjangoModelWithDunderUnicode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Model defines a `__unicode__` method")
    }
}

/// DJ014
pub(crate) fn model_with_dunder_unicode(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if has_dunder_method(class_def, checker.semantic()) {
        checker.diagnostics.push(Diagnostic::new(
            DjangoModelWithDunderUnicode,
            class_def.identifier(),
        ));
    }
}

/// Returns `true` if the class has a `__unicode__` method.
fn has_dunder_method(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    analyze::class::any_super_class(class_def, semantic, &|class_def| {
        class_def.body.iter().any(|val| match val {
            Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => name == "__unicode__",
            _ => false,
        })
    })
}
