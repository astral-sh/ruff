use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Arguments;

define_violation!(
    pub struct QuotedAnnotations;
);
impl AlwaysAutofixableViolation for QuotedAnnotations {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace old formatting imports with their new versions")
    }

    fn autofix_title(&self) -> String {
        "Updated the import".to_string()
    }
}

/// UP038
pub fn quoted_annotations(
    checker: &mut Checker,
    args: &Box<Arguments>,
    type_comment: &Option<String>,
) {
    println!("{:?}", args);
    println!("{:?}", type_comment);
}
