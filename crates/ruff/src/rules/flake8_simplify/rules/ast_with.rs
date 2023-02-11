use log::error;
use rustpython_parser::ast::{Located, Stmt, StmtKind, Withitem};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers::{first_colon_range, has_comments_in};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::{AutofixKind, Availability, Violation};

use super::fix_with;

define_violation!(
    pub struct MultipleWithStatements {
        pub fixable: bool,
    }
);
impl Violation for MultipleWithStatements {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use a single `with` statement with multiple contexts instead of nested `with` \
             statements"
        )
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let MultipleWithStatements { fixable, .. } = self;
        if *fixable {
            Some(|_| format!("Combine `with` statements"))
        } else {
            None
        }
    }
}

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
        let fixable = !has_comments_in(
            Range::new(with_stmt.location, with_body[0].location),
            checker.locator,
        );
        let mut diagnostic = Diagnostic::new(
            MultipleWithStatements { fixable },
            colon.map_or_else(
                || Range::from_located(with_stmt),
                |colon| Range::new(with_stmt.location, colon.end_location),
            ),
        );
        if fixable && checker.patch(diagnostic.kind.rule()) {
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
        checker.diagnostics.push(diagnostic);
    }
}
