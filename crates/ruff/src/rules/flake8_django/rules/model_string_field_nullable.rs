use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Constant::Bool;
use rustpython_ast::ExprKind::{Attribute, Call, Constant, Name};
use rustpython_ast::StmtKind::Assign;
use rustpython_ast::{Expr, Stmt};
define_violation!(
    pub struct ModelStringFieldNullable(pub String);
);
impl Violation for ModelStringFieldNullable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ModelStringFieldNullable(field) = self;
        format!("Avoid using null=True on string-based fields such as {field}.")
    }
}
const NOT_NULL_TRUE_FIELDS: [&str; 6] = [
    "CharField",
    "TextField",
    "SlugField",
    "EmailField",
    "FilePathField",
    "URLField",
];
pub fn model_string_field_nullable(
    checker: &Checker,
    bases: &[Expr],
    body: &[Stmt],
) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    if !bases.iter().any(|base| helpers::is_model(checker, base)) {
        return errors;
    }
    for statement in body.iter() {
        let Assign {value, ..} = &statement.node else {
            continue
        };
        if let Some(field_name) = check_nullable_field(value) {
            errors.push(Diagnostic::new(
                ModelStringFieldNullable(field_name.to_string()),
                Range::from_located(value),
            ));
        }
    }
    errors
}

fn check_nullable_field(value: &Expr) -> Option<&str> {
    let Call {func, keywords, ..} = &value.node else {
        return None;
    };
    let valid_field_name = match &func.node {
        Attribute { attr, .. } => {
            if !NOT_NULL_TRUE_FIELDS.contains(&&**attr) {
                return None;
            }
            Some(attr)
        }
        Name { id, .. } => {
            if !NOT_NULL_TRUE_FIELDS.contains(&&**id) {
                return None;
            }
            Some(id)
        }
        _ => None,
    };
    let Some(valid_field_name) = valid_field_name else {
        return None;
    };

    let mut null_key = false;
    let mut blank_key = false;
    let mut unique_key = false;
    for keyword in keywords.iter() {
        let Constant {value: Bool(true), ..} = &keyword.node.value.node else {
            continue
        };
        let Some(argument) = &keyword.node.arg else {
            continue
        };
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
    if null_key {
        return Some(valid_field_name);
    }
    None
}
