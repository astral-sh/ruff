use log::error;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Ranged, Stmt, WithItem};

use ruff_diagnostics::{AutofixKind, Violation};
use ruff_diagnostics::{Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{first_colon_range, has_comments_in};
use ruff_python_whitespace::UniversalNewlines;

use crate::checkers::ast::Checker;
use crate::line_width::LineWidth;
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
/// - [Python documentation: The `with` statement](https://docs.python.org/3/reference/compound_stmts.html#the-with-statement)
#[violation]
pub struct MultipleWithStatements;

impl Violation for MultipleWithStatements {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use a single `with` statement with multiple contexts instead of nested `with` \
             statements"
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Combine `with` statements".to_string())
    }
}

/// Returns a boolean indicating whether it's an async with statement, the items
/// and body.
fn next_with(body: &[Stmt]) -> Option<(bool, &[WithItem], &[Stmt])> {
    match body {
        [Stmt::With(ast::StmtWith { items, body, .. })] => Some((false, items, body)),
        [Stmt::AsyncWith(ast::StmtAsyncWith { items, body, .. })] => Some((true, items, body)),
        _ => None,
    }
}

/// SIM117
pub(crate) fn multiple_with_statements(
    checker: &mut Checker,
    with_stmt: &Stmt,
    with_body: &[Stmt],
    with_parent: Option<&Stmt>,
) {
    // Make sure we fix from top to bottom for nested with statements, e.g. for
    // ```python
    // with A():
    //     with B():
    //         with C():
    //             print("hello")
    // ```
    // suggests
    // ```python
    // with A(), B():
    //     with C():
    //         print("hello")
    // ```
    // but not the following
    // ```python
    // with A():
    //     with B(), C():
    //         print("hello")
    // ```
    if let Some(Stmt::With(ast::StmtWith { body, .. })) = with_parent {
        if body.len() == 1 {
            return;
        }
    }

    if let Some((is_async, items, body)) = next_with(with_body) {
        if is_async != with_stmt.is_async_with_stmt() {
            // One of the statements is an async with, while the other is not,
            // we can't merge those statements.
            return;
        }

        let last_item = items.last().expect("Expected items to be non-empty");
        let colon = first_colon_range(
            TextRange::new(
                last_item
                    .optional_vars
                    .as_ref()
                    .map_or(last_item.context_expr.end(), |v| v.end()),
                body.first().expect("Expected body to be non-empty").start(),
            ),
            checker.locator,
        );

        let mut diagnostic = Diagnostic::new(
            MultipleWithStatements,
            colon.map_or_else(
                || with_stmt.range(),
                |colon| TextRange::new(with_stmt.start(), colon.end()),
            ),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if !has_comments_in(
                TextRange::new(with_stmt.start(), with_body[0].start()),
                checker.locator,
            ) {
                match fix_with::fix_multiple_with_statements(
                    checker.locator,
                    checker.stylist,
                    with_stmt,
                ) {
                    Ok(edit) => {
                        if edit
                            .content()
                            .unwrap_or_default()
                            .universal_newlines()
                            .all(|line| {
                                LineWidth::new(checker.settings.tab_size).add_str(&line)
                                    <= checker.settings.line_length
                            })
                        {
                            diagnostic.set_fix(Fix::suggested(edit));
                        }
                    }
                    Err(err) => error!("Failed to fix nested with: {err}"),
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
