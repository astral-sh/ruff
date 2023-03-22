use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::rules::flake8_django::rules::helpers::is_model_form;

/// ## What it does
/// Checks for the use of `fields = "__all__"` in Django `ModelForm`
/// classes.
///
/// ## Why is this bad?
/// If a `ModelForm` includes the `fields = "__all__"` attribute, any new
/// field that is added to the model will automatically be exposed for
/// modification.
///
/// ## Example
/// ```python
/// from django.forms import ModelForm
///
///
/// class PostForm(ModelForm):
///     class Meta:
///         model = Post
///         fields = "__all__"
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
#[violation]
pub struct DjangoAllWithModelForm;

impl Violation for DjangoAllWithModelForm {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use `__all__` with `ModelForm`, use `fields` instead")
    }
}

/// DJ007
pub fn all_with_model_form(checker: &Checker, bases: &[Expr], body: &[Stmt]) -> Option<Diagnostic> {
    if !bases.iter().any(|base| is_model_form(&checker.ctx, base)) {
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
            let StmtKind::Assign { targets, value, .. } = &element.node else {
                continue;
            };
            for target in targets.iter() {
                let ExprKind::Name { id, .. } = &target.node else {
                    continue;
                };
                if id != "fields" {
                    continue;
                }
                let ExprKind::Constant { value, .. } = &value.node else {
                    continue;
                };
                match &value {
                    Constant::Str(s) => {
                        if s == "__all__" {
                            return Some(Diagnostic::new(
                                DjangoAllWithModelForm,
                                Range::from(element),
                            ));
                        }
                    }
                    Constant::Bytes(b) => {
                        if b == "__all__".as_bytes() {
                            return Some(Diagnostic::new(
                                DjangoAllWithModelForm,
                                Range::from(element),
                            ));
                        }
                    }
                    _ => (),
                };
            }
        }
    }
    None
}
