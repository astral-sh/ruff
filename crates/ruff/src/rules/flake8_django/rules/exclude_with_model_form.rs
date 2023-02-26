use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::rules::flake8_django::rules::helpers::is_model_form;
use crate::violation::Violation;
use crate::{checkers::ast::Checker, registry::Diagnostic, Range};

define_violation!(
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
    /// class PostForm(ModelForm):
    ///     class Meta:
    ///         model = Post
    ///         fields = ["title", "content"]
    /// ```
    pub struct ExcludeWithModelForm;
);
impl Violation for ExcludeWithModelForm {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use `exclude` with `ModelForm`, use `fields` instead")
    }
}

/// DJ006
pub fn exclude_with_model_form(
    checker: &Checker,
    bases: &[Expr],
    body: &[Stmt],
) -> Option<Diagnostic> {
    if !bases.iter().any(|base| is_model_form(checker, base)) {
        return None;
    }
    for element in body.iter() {
        let StmtKind::ClassDef { name, body, .. } = &element.node else {
            continue;
        };
        if name != "Meta" {
            continue;
        }
        for element in body.iter() {
            let StmtKind::Assign { targets, .. } = &element.node else {
                continue;
            };
            for target in targets.iter() {
                let ExprKind::Name { id, .. } = &target.node else {
                    continue;
                };
                if id == "exclude" {
                    return Some(Diagnostic::new(
                        ExcludeWithModelForm,
                        Range::from_located(target),
                    ));
                }
            }
        }
    }
    None
}
