use crate::ast::types::Range;
use crate::registry::Diagnostic;

use crate::violations;
use rustpython_parser::ast::{Expr, ExprKind};

/// F621, F622
pub fn starred_expressions(
    elts: &[Expr],
    check_too_many_expressions: bool,
    check_two_starred_expressions: bool,
    location: Range,
) -> Option<Diagnostic> {
    let mut has_starred: bool = false;
    let mut starred_index: Option<usize> = None;
    for (index, elt) in elts.iter().enumerate() {
        if matches!(elt.node, ExprKind::Starred { .. }) {
            if has_starred && check_two_starred_expressions {
                return Some(Diagnostic::new(violations::TwoStarredExpressions, location));
            }
            has_starred = true;
            starred_index = Some(index);
        }
    }

    if check_too_many_expressions {
        if let Some(starred_index) = starred_index {
            if starred_index >= 1 << 8 || elts.len() - starred_index > 1 << 24 {
                return Some(Diagnostic::new(
                    violations::ExpressionsInStarAssignment,
                    location,
                ));
            }
        }
    }

    None
}
