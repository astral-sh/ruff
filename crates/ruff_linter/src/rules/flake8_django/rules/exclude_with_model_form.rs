use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, ExprKind, StmtKind};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_django::helpers::is_model_form;

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
#[violation_metadata(stable_since = "v0.0.253", category = Category::Security)]
pub(crate) struct DjangoExcludeWithModelForm;

impl Violation for DjangoExcludeWithModelForm {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not use `exclude` with `ModelForm`, use `fields` instead".to_string()
    }
}

/// DJ006
pub(crate) fn exclude_with_model_form(checker: &Checker, class_def: ast::StmtClassDef<'_>) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if !is_model_form(class_def, checker.semantic()) {
        return;
    }

    for element in class_def.body() {
        let StmtKind::ClassDef(class_def_node) = element.classify() else {
            continue;
        };
        let name = class_def_node.name();
        let body = class_def_node.body();
        if name != "Meta" {
            continue;
        }
        for element in body {
            let StmtKind::Assign(assign) = element.classify() else {
                continue;
            };
            let targets = assign.targets();
            for target in targets {
                let ExprKind::Name(name_node) = target.classify() else {
                    continue;
                };
                let id = name_node.id();
                if id == "exclude" {
                    checker.report_diagnostic(DjangoExcludeWithModelForm, target.range());
                    return;
                }
            }
        }
    }
}
