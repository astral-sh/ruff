use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::whitespace::leading_space;

/// ## What it does
/// Checks for mixed tabs and spaces in indentation.
///
/// ## Why is this bad?
/// Never mix tabs and spaces.
///
/// The most popular way of indenting Python is with spaces only. The
/// second-most popular way is with tabs only. Code indented with a
/// mixture of tabs and spaces should be converted to using spaces
/// exclusively.
///
/// ## Example
/// ```python
/// if a == 0:\n        a = 1\n\tb = 1
/// ```
///
/// Use instead:
/// ```python
/// if a == 0:\n    a = 1\n    b = 1
/// ```
#[violation]
pub struct MixedSpacesAndTabs;

impl Violation for MixedSpacesAndTabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains mixed spaces and tabs")
    }
}

/// E101
pub fn mixed_spaces_and_tabs(lineno: usize, line: &str) -> Option<Diagnostic> {
    let indent = leading_space(line);

    if indent.contains(' ') && indent.contains('\t') {
        Some(Diagnostic::new(
            MixedSpacesAndTabs,
            Range::new(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 1, indent.chars().count()),
            ),
        ))
    } else {
        None
    }
}
