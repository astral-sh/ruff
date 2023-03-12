use rustpython_parser::ast::Arg;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct InvalidArgumentName {
    pub name: String,
}

impl Violation for InvalidArgumentName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidArgumentName { name } = self;
        format!("Argument name `{name}` should be lowercase")
    }
}

/// N803
pub fn invalid_argument_name(name: &str, arg: &Arg, ignore_names: &[String]) -> Option<Diagnostic> {
    if ignore_names.iter().any(|ignore_name| ignore_name == name) {
        return None;
    }
    if name.to_lowercase() != name {
        return Some(Diagnostic::new(
            InvalidArgumentName {
                name: name.to_string(),
            },
            Range::from(arg),
        ));
    }
    None
}
