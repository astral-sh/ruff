use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Arguments;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct DuplicateArgumentName {
        name: String
    }
);

impl Violation for DuplicateArgumentName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateArgumentName { name } = self;
        format!("Duplicate argument '{name}' in function definition")
    }
}

/// PLE0108
pub fn duplicate_argument_name(checker: &mut Checker, args: &Arguments) {
    println!("{:?}", args);
}
