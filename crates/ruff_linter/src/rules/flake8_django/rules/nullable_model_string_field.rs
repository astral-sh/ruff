use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks nullable string-based fields (like `CharField` and `TextField`)
/// in Django models.
///
/// ## Why is this bad?
/// If a string-based field is nullable, then your model will have two possible
/// representations for "no data": `None` and the empty string. This can lead to
/// confusion, as clients of the API have to check for both `None` and the
/// empty string when trying to determine if the field has data.
///
/// The Django convention is to use the empty string in lieu of `None` for
/// string-based fields.
///
/// ## Example
/// ```python
/// from django.db import models
///
///
/// class MyModel(models.Model):
///     field = models.CharField(max_length=255, null=True)
/// ```
///
/// Use instead:
/// ```python
/// from django.db import models
///
///
/// class MyModel(models.Model):
///     field = models.CharField(max_length=255, default="")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct DjangoNullableModelStringField {
    field_name: String,
}

impl Violation for DjangoNullableModelStringField {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DjangoNullableModelStringField { field_name } = self;
        format!("Avoid using `null=True` on string-based fields such as `{field_name}`")
    }
}

/// DJ001
pub(crate) fn nullable_model_string_field(checker: &Checker, body: &[Stmt]) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    for statement in body {
        let Stmt::Assign(ast::StmtAssign { value, .. }) = statement else {
            continue;
        };
        if let Some(field_name) = is_nullable_field(value, checker.semantic()) {
            checker.report_diagnostic(Diagnostic::new(
                DjangoNullableModelStringField {
                    field_name: field_name.to_string(),
                },
                value.range(),
            ));
        }
    }
}

fn is_nullable_field<'a>(value: &'a Expr, semantic: &'a SemanticModel) -> Option<&'a str> {
    let call = value.as_call_expr()?;

    let field_name = helpers::get_model_field_name(&call.func, semantic)?;
    if !matches!(
        field_name,
        "CharField" | "TextField" | "SlugField" | "EmailField" | "FilePathField" | "URLField"
    ) {
        return None;
    }

    let mut null_key = false;
    let mut blank_key = false;
    let mut unique_key = false;
    for keyword in &*call.arguments.keywords {
        let Some(argument) = &keyword.arg else {
            continue;
        };
        if !is_const_true(&keyword.value) {
            continue;
        }
        match argument.as_str() {
            "blank" => blank_key = true,
            "null" => null_key = true,
            "unique" => unique_key = true,
            _ => continue,
        }
    }
    if blank_key && unique_key {
        return None;
    }
    if !null_key {
        return None;
    }

    Some(field_name)
}
