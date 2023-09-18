use ruff_python_ast::{self as ast, Arguments, Constant, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

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
pub(crate) fn all_with_model_form(
    checker: &Checker,
    arguments: Option<&Arguments>,
    body: &[Stmt],
) -> Option<Diagnostic> {
    if !arguments.is_some_and(|arguments| {
        arguments
            .args
            .iter()
            .any(|base| is_model_form(base, checker.semantic()))
    }) {
        return None;
    }

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
                if id != "fields" {
                    continue;
                }
                let Expr::Constant(ast::ExprConstant { value, .. }) = value.as_ref() else {
                    continue;
                };
                match value {
                    Constant::Str(ast::StringConstant { value, .. }) => {
                        if value == "__all__" {
                            return Some(Diagnostic::new(DjangoAllWithModelForm, element.range()));
                        }
                    }
                    Constant::Bytes(ast::BytesConstant { value, .. }) => {
                        if value == "__all__".as_bytes() {
                            return Some(Diagnostic::new(DjangoAllWithModelForm, element.range()));
                        }
                    }
                    _ => (),
                };
            }
        }
    }
    None
}
