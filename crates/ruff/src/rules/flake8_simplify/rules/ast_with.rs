use log::error;
use rustpython_parser::ast::{Located, Stmt, StmtKind, Withitem};
use unicode_width::UnicodeWidthStr;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AutofixKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{first_colon_range, has_comments_in};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

use super::fix_with;

/// ## What it does
/// Checks for the unnecessary nesting of multiple consecutive context
/// managers.
///
/// ## Why is this bad?
/// In Python 3, a single `with` block can include multiple context
/// managers.
///
/// Combining multiple context managers into a single `with` statement
/// will minimize the indentation depth of the code, making it more
/// readable.
///
/// ## Example
/// ```python
/// with A() as a:
///     with B() as b:
///         pass
/// ```
///
/// Use instead:
/// ```python
/// with A() as a, B() as b:
///     pass
/// ```
///
/// ## References
/// - [Python: "The with statement"](https://docs.python.org/3/reference/compound_stmts.html#the-with-statement)
#[violation]
pub struct MultipleWithStatements {
    pub fixable: bool,
}

impl Violation for MultipleWithStatements {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use a single `with` statement with multiple contexts instead of nested `with` \
             statements"
        )
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|_| format!("Combine `with` statements"))
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
                || Range::from(with_stmt),
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
                        .content()
                        .unwrap_or_default()
                        .universal_newlines()
                        .all(|line| line.width() <= checker.settings.line_length)
                    {
                        diagnostic.set_fix(fix);
                    }
                }
                Err(err) => error!("Failed to fix nested with: {err}"),
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
