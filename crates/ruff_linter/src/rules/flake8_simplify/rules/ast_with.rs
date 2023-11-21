use ast::Expr;
use log::error;

use ruff_diagnostics::{Diagnostic, Fix};
use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Stmt, WithItem};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::fits;

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
/// The following context managers are exempt when used as standalone
/// statements:
///
///  - `anyio`.{`CancelScope`, `fail_after`, `move_on_after`}
///  - `asyncio`.{`timeout`, `timeout_at`}
///  - `trio`.{`fail_after`, `fail_at`, `move_on_after`, `move_on_at`}
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use a single `with` statement with multiple contexts instead of nested `with` \
             statements"
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Combine `with` statements".to_string())
    }
}

/// Returns a boolean indicating whether it's an async with statement, the items
/// and body.
fn next_with(body: &[Stmt]) -> Option<(bool, &[WithItem], &[Stmt])> {
    let [Stmt::With(ast::StmtWith {
        is_async,
        items,
        body,
        ..
    })] = body
    else {
        return None;
    };
    Some((*is_async, items, body))
}

/// Check if `with_items` contains a single item which should not necessarily be
/// grouped with other items.
///
/// For example:
/// ```python
/// async with asyncio.timeout(1):
///     with resource1(), resource2():
///         ...
/// ```
fn explicit_with_items(checker: &mut Checker, with_items: &[WithItem]) -> bool {
    let [with_item] = with_items else {
        return false;
    };
    let Expr::Call(expr_call) = &with_item.context_expr else {
        return false;
    };
    checker
        .semantic()
        .resolve_call_path(&expr_call.func)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["asyncio", "timeout" | "timeout_at"]
                    | ["anyio", "CancelScope" | "fail_after" | "move_on_after"]
                    | [
                        "trio",
                        "fail_after" | "fail_at" | "move_on_after" | "move_on_at"
                    ]
            )
        })
}

/// SIM117
pub(crate) fn multiple_with_statements(
    checker: &mut Checker,
    with_stmt: &ast::StmtWith,
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

    if let Some((is_async, items, _body)) = next_with(&with_stmt.body) {
        if is_async != with_stmt.is_async {
            // One of the statements is an async with, while the other is not,
            // we can't merge those statements.
            return;
        }

        if explicit_with_items(checker, &with_stmt.items) || explicit_with_items(checker, items) {
            return;
        }

        let Some(colon) = items.last().and_then(|item| {
            SimpleTokenizer::starts_at(item.end(), checker.locator().contents())
                .skip_trivia()
                .find(|token| token.kind == SimpleTokenKind::Colon)
        }) else {
            return;
        };

        let mut diagnostic = Diagnostic::new(
            MultipleWithStatements,
            TextRange::new(with_stmt.start(), colon.end()),
        );
        if !checker
            .indexer()
            .comment_ranges()
            .intersects(TextRange::new(with_stmt.start(), with_stmt.body[0].start()))
        {
            match fix_with::fix_multiple_with_statements(
                checker.locator(),
                checker.stylist(),
                with_stmt,
            ) {
                Ok(edit) => {
                    if edit.content().map_or(true, |content| {
                        fits(
                            content,
                            with_stmt.into(),
                            checker.locator(),
                            checker.settings.pycodestyle.max_line_length,
                            checker.settings.tab_size,
                        )
                    }) {
                        diagnostic.set_fix(Fix::unsafe_edit(edit));
                    }
                }
                Err(err) => error!("Failed to fix nested with: {err}"),
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
