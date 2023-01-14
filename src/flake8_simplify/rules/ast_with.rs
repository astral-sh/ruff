use rustpython_ast::{Stmt, StmtKind, Withitem};

use crate::ast::helpers::first_colon_range;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

fn find_nested_with(body: &[Stmt]) -> Option<(&Vec<Withitem>, &Vec<Stmt>)> {
    if body.len() != 1 {
        return None;
    }
    let StmtKind::With { items, body, .. } = &body[0].node else {
        return None
    };
    find_nested_with(body).or(Some((items, body)))
}

/// SIM117
pub fn multiple_with_statements(
    checker: &mut Checker,
    stmt: &Stmt,
    body: &[Stmt],
    parent: Option<&Stmt>,
) {
    if let Some(parent) = parent {
        if let StmtKind::With { body, .. } = &parent.node {
            if body.len() == 1 {
                return;
            }
        }
    }
    if let Some((items, body)) = find_nested_with(body) {
        let last_item = items.last().expect("Expected items to be non-empty");
        let colon = first_colon_range(
            Range::new(
                last_item
                    .optional_vars
                    .as_ref()
                    .map_or(last_item.context_expr.end_location, |v| v.end_location)
                    .unwrap(),
                body.first()
                    .expect("Expected body to be non-empty")
                    .location,
            ),
            checker.locator,
        );
        checker.diagnostics.push(Diagnostic::new(
            violations::MultipleWithStatements,
            colon.map_or_else(
                || Range::from_located(stmt),
                |colon| Range::new(stmt.location, colon.end_location),
            ),
        ));
    }
}
