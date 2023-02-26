use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Stmt, StmtKind};

use crate::ast::helpers::identifier_range;
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    pub struct CollapsibleElseIf {}
);

impl Violation for CollapsibleElseIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider using \"elif\" instead of \"else\" then \"if\" to remove one indentation level")
    }
}

/// PLR5501
pub fn collapsible_else_if(orelse: &[Stmt], locator: &Locator) -> Option<Diagnostic> {
    if orelse.len() == 1 {
        let first = &orelse[0];
        // Only consider if the body of the else portion is exactly 1 in size
        if matches!(first.node, StmtKind::If { .. }) {
            // check the source if this is else then if or elif
            // we distinguish between if and elif...by looking at the source
            if locator
                .slice(&Range {
                    location: first.location,
                    end_location: first.end_location.unwrap(),
                })
                .starts_with("if")
            {
                // pylint uses the row/col for the if after the else
                // that could be turned into an elif
                return Some(Diagnostic::new(
                    CollapsibleElseIf {},
                    identifier_range(first, locator),
                ));
            }
        }
    }
    None
}
