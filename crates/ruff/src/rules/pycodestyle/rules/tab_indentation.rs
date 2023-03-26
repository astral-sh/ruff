use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::whitespace::leading_space;

#[violation]
pub struct TabIndentation {
    value: bool,
}

impl Violation for TabIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("{}", self.value)
    }
}

/// W191
pub fn tab_indentation(lineno: usize, line: &str, in_quote: bool) -> Option<Diagnostic> {
    let indent = leading_space(line);

    if indent.contains('\t') && !in_quote {
        Some(Diagnostic::new(
            TabIndentation { value: in_quote },
            Range::new(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 1, indent.chars().count()),
            ),
        ))
    } else {
        None
    }
}
