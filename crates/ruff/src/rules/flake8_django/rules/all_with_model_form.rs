use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt, StmtKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::rules::flake8_django::rules::helpers::is_model_form;
use crate::violation::Violation;
use crate::{checkers::ast::Checker, registry::Diagnostic, Range};

define_violation!(
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
    /// class PostForm(ModelForm):
    ///     class Meta:
    ///         model = Post
    ///         fields = ["title", "content"]
    /// ```
    pub struct AllWithModelForm;
);
impl Violation for AllWithModelForm {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use `__all__` with `ModelForm`, use `fields` instead")
    }
}

/// DJ007
pub fn all_with_model_form(checker: &Checker, bases: &[Expr], body: &[Stmt]) -> Option<Diagnostic> {
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
                                AllWithModelForm,
                                Range::from_located(element),
                            ));
                        }
                    }
                    Constant::Bytes(b) => {
                        if b == "__all__".as_bytes() {
                            return Some(Diagnostic::new(
                                AllWithModelForm,
                                Range::from_located(element),
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
