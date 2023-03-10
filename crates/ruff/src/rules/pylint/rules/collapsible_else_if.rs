use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

#[violation]
pub struct CollapsibleElseIf;

impl Violation for CollapsibleElseIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider using `elif` instead of `else` then `if` to remove one indentation level")
    }
}

/// PLR5501
pub fn collapsible_else_if(orelse: &[Stmt], locator: &Locator) -> Option<Diagnostic> {
    if orelse.len() == 1 {
        let first = &orelse[0];
        if matches!(first.node, StmtKind::If { .. }) {
            // Determine whether this is an `elif`, or an `if` in an `else` block.
            if locator
                .slice(Range {
                    location: first.location,
                    end_location: first.end_location.unwrap(),
                })
                .starts_with("if")
            {
                return Some(Diagnostic::new(CollapsibleElseIf, Range::from(first)));
            }
        }
    }
    None
}
