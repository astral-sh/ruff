use log::error;
use rustpython_ast::{Located, Stmt, StmtKind, Withitem};

use super::fix_with;
use crate::ast::helpers::{first_colon_range, has_comments_in};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

fn find_last_with(body: &[Stmt]) -> Option<(&Vec<Withitem>, &Vec<Stmt>)> {
    let [Located { node: StmtKind::With { items, body, .. }, ..}] = body else { return None };
    find_last_with(body).or(Some((items, body)))
}

/// SIM117
pub fn multiple_with_statements(
    checker: &mut Checker,
    with_stmt: &Stmt,
    with_body: &[Stmt],
    with_parent: Option<&Stmt>,
) {
    if let Some(parent) = with_parent {
        if let StmtKind::With { body, .. } = &parent.node {
            if body.len() == 1 {
                return;
            }
        }
    }
    if let Some((items, body)) = find_last_with(with_body) {
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
        let mut diagnostic = Diagnostic::new(
            violations::MultipleWithStatements,
            colon.map_or_else(
                || Range::from_located(with_stmt),
                |colon| Range::new(with_stmt.location, colon.end_location),
            ),
        );
        if checker.patch(&Rule::MultipleWithStatements) {
            let nested_with = &with_body[0];
            if !has_comments_in(
                Range::new(with_stmt.location, nested_with.location),
                checker.locator,
            ) {
                match fix_with::fix_multiple_with_statements(
                    checker.locator,
                    checker.stylist,
                    with_stmt,
                ) {
                    Ok(fix) => {
                        if fix
                            .content
                            .lines()
                            .all(|line| line.len() <= checker.settings.line_length)
                        {
                            diagnostic.amend(fix);
                        }
                    }
                    Err(err) => error!("Failed to fix nested with: {err}"),
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
