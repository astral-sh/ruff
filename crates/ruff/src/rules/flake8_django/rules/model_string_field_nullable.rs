use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Constant::Bool;
use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

define_violation!(
    pub struct ModelStringFieldNullable {
        pub field_name: String,
    }
);
impl Violation for ModelStringFieldNullable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ModelStringFieldNullable { field_name } = self;
        format!("Avoid using `null=True` on string-based fields such as {field_name}")
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

/// DJ001
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
        let StmtKind::Assign {value, ..} = &statement.node else {
            continue
        };
        if let Some(field_name) = check_nullable_field(checker, value) {
            errors.push(Diagnostic::new(
                ModelStringFieldNullable {
                    field_name: field_name.to_string(),
                },
                Range::from_located(value),
            ));
        }
    }
    errors
}

fn check_nullable_field<'a>(checker: &'a Checker, value: &'a Expr) -> Option<&'a str> {
    let ExprKind::Call {func, keywords, ..} = &value.node else {
        return None;
    };

    let Some(valid_field_name) = helpers::get_model_field_name(checker, func) else {
        return None;
    };
    if !NOT_NULL_TRUE_FIELDS.contains(&valid_field_name) {
        return None;
    }

    let mut null_key = false;
    let mut blank_key = false;
    let mut unique_key = false;
    for keyword in keywords.iter() {
        let ExprKind::Constant {value: Bool(true), ..} = &keyword.node.value.node else {
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
