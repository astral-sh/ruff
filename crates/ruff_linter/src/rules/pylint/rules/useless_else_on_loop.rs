use anyhow::Result;

use ast::whitespace::indentation;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier;
use ruff_python_ast::{self as ast, ExceptHandler, MatchCase, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::adjust_indentation;
use crate::Locator;

/// ## What it does
/// Checks for `else` clauses on loops without a `break` statement.
///
/// ## Why is this bad?
/// When a loop includes an `else` statement, the code inside the `else` clause
/// will be executed if the loop terminates "normally" (i.e., without a
/// `break`).
///
/// If a loop _always_ terminates "normally" (i.e., does _not_ contain a
/// `break`), then the `else` clause is redundant, as the code inside the
/// `else` clause will always be executed.
///
/// In such cases, the code inside the `else` clause can be moved outside the
/// loop entirely, and the `else` clause can be removed.
///
/// ## Example
/// ```python
/// for item in items:
///     print(item)
/// else:
///     print("All items printed")
/// ```
///
/// Use instead:
/// ```python
/// for item in items:
///     print(item)
/// print("All items printed")
/// ```
///
/// ## References
/// - [Python documentation: `break` and `continue` Statements, and `else` Clauses on Loops](https://docs.python.org/3/tutorial/controlflow.html#break-and-continue-statements-and-else-clauses-on-loops)
#[derive(ViolationMetadata)]
pub(crate) struct UselessElseOnLoop;

impl Violation for UselessElseOnLoop {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`else` clause on loop without a `break` statement; remove the `else` and dedent its contents".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `else`".to_string())
    }
}

/// PLW0120
pub(crate) fn useless_else_on_loop(checker: &Checker, stmt: &Stmt, body: &[Stmt], orelse: &[Stmt]) {
    if orelse.is_empty() || loop_exits_early(body) {
        return;
    }

    let else_range = identifier::else_(stmt, checker.locator().contents()).expect("else clause");

    let mut diagnostic = Diagnostic::new(UselessElseOnLoop, else_range);
    diagnostic.try_set_fix(|| {
        remove_else(
            stmt,
            orelse,
            else_range,
            checker.locator(),
            checker.indexer(),
            checker.stylist(),
        )
    });
    checker.report_diagnostic(diagnostic);
}

/// Returns `true` if the given body contains a `break` statement.
fn loop_exits_early(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            loop_exits_early(body)
                || elif_else_clauses
                    .iter()
                    .any(|clause| loop_exits_early(&clause.body))
        }
        Stmt::With(ast::StmtWith { body, .. }) => loop_exits_early(body),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .any(|MatchCase { body, .. }| loop_exits_early(body)),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            loop_exits_early(body)
                || loop_exits_early(orelse)
                || loop_exits_early(finalbody)
                || handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) => loop_exits_early(body),
                })
        }
        Stmt::For(ast::StmtFor { orelse, .. }) | Stmt::While(ast::StmtWhile { orelse, .. }) => {
            loop_exits_early(orelse)
        }
        Stmt::Break(_) => true,
        _ => false,
    })
}

/// Generate a [`Fix`] to remove the `else` clause from the given statement.
fn remove_else(
    stmt: &Stmt,
    orelse: &[Stmt],
    else_range: TextRange,
    locator: &Locator,
    indexer: &Indexer,
    stylist: &Stylist,
) -> Result<Fix> {
    let Some(start) = orelse.first() else {
        return Err(anyhow::anyhow!("Empty `else` clause"));
    };
    let Some(end) = orelse.last() else {
        return Err(anyhow::anyhow!("Empty `else` clause"));
    };

    let start_indentation = indentation(locator.contents(), start);
    if start_indentation.is_none() {
        // Inline `else` block (e.g., `else: x = 1`).
        Ok(Fix::safe_edit(Edit::deletion(
            else_range.start(),
            start.start(),
        )))
    } else {
        // Identify the indentation of the loop itself (e.g., the `while` or `for`).
        let Some(desired_indentation) = indentation(locator.contents(), stmt) else {
            return Err(anyhow::anyhow!("Compound statement cannot be inlined"));
        };

        // Dedent the content from the end of the `else` to the end of the loop.
        let indented = adjust_indentation(
            TextRange::new(
                locator.full_line_end(else_range.start()),
                locator.full_line_end(end.end()),
            ),
            desired_indentation,
            locator,
            indexer,
            stylist,
        )?;

        // Replace the content from the start of the `else` to the end of the loop.
        Ok(Fix::safe_edit(Edit::replacement(
            indented,
            locator.line_start(else_range.start()),
            locator.full_line_end(end.end()),
        )))
    }
}
