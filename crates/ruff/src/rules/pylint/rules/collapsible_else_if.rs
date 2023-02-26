use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    pub struct CollapsibleElseIf;
);

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
                .slice(&Range {
                    location: first.location,
                    end_location: first.end_location.unwrap(),
                })
                .starts_with("if")
            {
                return Some(Diagnostic::new(
                    CollapsibleElseIf,
                    Range::from_located(first),
                ));
            }
        }
    }
    None
}
