use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for the order of Model's inner classes, methods, and fields as per
/// the [Django Style Guide].
///
/// ## Why is this bad?
/// The [Django Style Guide] specifies that the order of Model inner classes,
/// attributes and methods should be as follows:
///
/// 1. All database fields
/// 2. Custom manager attributes
/// 3. `class Meta`
/// 4. `def __str__()`
/// 5. `def save()`
/// 6. `def get_absolute_url()`
/// 7. Any custom methods
///
/// ## Examples
/// ```python
/// from django.db import models
///
///
/// class StrBeforeFieldModel(models.Model):
///     class Meta:
///         verbose_name = "test"
///         verbose_name_plural = "tests"
///
///     def __str__(self):
///         return "foobar"
///
///     first_name = models.CharField(max_length=32)
///     last_name = models.CharField(max_length=40)
/// ```
///
/// Use instead:
/// ```python
/// from django.db import models
///
///
/// class StrBeforeFieldModel(models.Model):
///     first_name = models.CharField(max_length=32)
///     last_name = models.CharField(max_length=40)
///
///     class Meta:
///         verbose_name = "test"
///         verbose_name_plural = "tests"
///
///     def __str__(self):
///         return "foobar"
/// ```
///
/// [Django Style Guide]: https://docs.djangoproject.com/en/dev/internals/contributing/writing-code/coding-style/#model-style
#[violation]
pub struct DjangoUnorderedBodyContentInModel {
    element_type: ContentType,
    prev_element_type: ContentType,
}

impl Violation for DjangoUnorderedBodyContentInModel {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DjangoUnorderedBodyContentInModel {
            element_type,
            prev_element_type,
        } = self;
        format!("Order of model's inner classes, methods, and fields does not follow the Django Style Guide: {element_type} should come before {prev_element_type}")
    }
}

/// DJ012
pub(crate) fn unordered_body_content_in_model(
    checker: &mut Checker,
    class_def: &ast::StmtClassDef,
) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if !helpers::is_model(class_def, checker.semantic()) {
        return;
    }

    // Track all the element types we've seen so far.
    let mut element_types = Vec::new();
    let mut prev_element_type = None;
    for element in &class_def.body {
        let Some(element_type) = get_element_type(element, checker.semantic()) else {
            continue;
        };

        // Skip consecutive elements of the same type. It's less noisy to only report
        // violations at type boundaries (e.g., avoid raising a violation for _every_
        // field declaration that's out of order).
        if prev_element_type == Some(element_type) {
            continue;
        }

        prev_element_type = Some(element_type);

        if let Some(&prev_element_type) = element_types
            .iter()
            .find(|&&prev_element_type| prev_element_type > element_type)
        {
            let diagnostic = Diagnostic::new(
                DjangoUnorderedBodyContentInModel {
                    element_type,
                    prev_element_type,
                },
                element.range(),
            );
            checker.diagnostics.push(diagnostic);
        } else {
            element_types.push(element_type);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
enum ContentType {
    FieldDeclaration,
    ManagerDeclaration,
    MetaClass,
    StrMethod,
    SaveMethod,
    GetAbsoluteUrlMethod,
    CustomMethod,
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentType::FieldDeclaration => f.write_str("field declaration"),
            ContentType::ManagerDeclaration => f.write_str("manager declaration"),
            ContentType::MetaClass => f.write_str("`Meta` class"),
            ContentType::StrMethod => f.write_str("`__str__` method"),
            ContentType::SaveMethod => f.write_str("`save` method"),
            ContentType::GetAbsoluteUrlMethod => f.write_str("`get_absolute_url` method"),
            ContentType::CustomMethod => f.write_str("custom method"),
        }
    }
}

fn get_element_type(element: &Stmt, semantic: &SemanticModel) -> Option<ContentType> {
    match element {
        Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
                if helpers::is_model_field(func, semantic) {
                    return Some(ContentType::FieldDeclaration);
                }
            }
            let expr = targets.first()?;
            let Expr::Name(ast::ExprName { id, .. }) = expr else {
                return None;
            };
            if id == "objects" {
                Some(ContentType::ManagerDeclaration)
            } else {
                None
            }
        }
        Stmt::ClassDef(ast::StmtClassDef { name, .. }) => {
            if name == "Meta" {
                Some(ContentType::MetaClass)
            } else {
                None
            }
        }
        Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => match name.as_str() {
            "__str__" => Some(ContentType::StrMethod),
            "save" => Some(ContentType::SaveMethod),
            "get_absolute_url" => Some(ContentType::GetAbsoluteUrlMethod),
            _ => Some(ContentType::CustomMethod),
        },
        _ => None,
    }
}
