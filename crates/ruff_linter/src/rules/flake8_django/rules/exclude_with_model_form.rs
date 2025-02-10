use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_django::rules::helpers::is_model_form;

/// ## What it does
/// Checks for the use of `exclude` in Django `ModelForm` classes.
///
/// ## Why is this bad?
/// If a `ModelForm` includes the `exclude` attribute, any new field that
/// is added to the model will automatically be exposed for modification.
///
/// ## Example
/// ```python
/// from django.forms import ModelForm
///
///
/// class PostForm(ModelForm):
///     class Meta:
///         model = Post
///         exclude = ["author"]
/// ```
///
/// Use instead:
/// ```python
/// from django.forms import ModelForm
///
///
/// class PostForm(ModelForm):
///     class Meta:
///         model = Post
///         fields = ["title", "content"]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct DjangoExcludeWithModelForm;

impl Violation for DjangoExcludeWithModelForm {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not use `exclude` with `ModelForm`, use `fields` instead".to_string()
    }
}

/// DJ006
pub(crate) fn exclude_with_model_form(checker: &Checker, class_def: &ast::StmtClassDef) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if !is_model_form(class_def, checker.semantic()) {
        return;
    }

    for element in &class_def.body {
        let Stmt::ClassDef(ast::StmtClassDef { name, body, .. }) = element else {
            continue;
        };
        if name != "Meta" {
            continue;
        }
        for element in body {
            let Stmt::Assign(ast::StmtAssign { targets, .. }) = element else {
                continue;
            };
            for target in targets {
                let Expr::Name(ast::ExprName { id, .. }) = target else {
                    continue;
                };
                if id == "exclude" {
                    checker.report_diagnostic(Diagnostic::new(
                        DjangoExcludeWithModelForm,
                        target.range(),
                    ));
                    return;
                }
            }
        }
    }
}
