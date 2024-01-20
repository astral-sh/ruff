use ast::whitespace::indentation;
use ruff_python_ast::{self as ast, ExceptHandler, MatchCase, Stmt};

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::rules::pyupgrade::fixes::adjust_indentation;

use crate::checkers::ast::Checker;

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
#[violation]
pub struct UselessElseOnLoop;

impl Violation for UselessElseOnLoop {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`else` clause on loop without a `break` statement; remove the `else` and de-indent all the \
             code inside it"
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove redundant `else` clause".to_string())
    }
}

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

/// PLW0120
pub(crate) fn useless_else_on_loop(
    checker: &mut Checker,
    stmt: &Stmt,
    body: &[Stmt],
    orelse: &[Stmt],
) {
    if orelse.is_empty() || loop_exits_early(body) {
        return;
    }

    let else_range = identifier::else_(stmt, checker.locator().contents()).unwrap();

    let mut diagnostic = Diagnostic::new(UselessElseOnLoop, else_range);

    if checker.settings.preview.is_enabled() {
        let start = orelse.first().unwrap();
        let end = orelse.last().unwrap();
        let start_indentation = indentation(checker.locator(), start);
        if start_indentation.is_none() {
            // Inline `else` block (e.g., `else: x = 1`).
            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_replacement(
                    String::new(),
                    TextRange::new(else_range.start(), start.start()),
                ),
                Applicability::Safe,
            ));
        } else {
            let desired_indentation = indentation(checker.locator(), stmt).unwrap_or("");
            let else_line_range = checker.locator().full_line_range(else_range.start());

            let indented = adjust_indentation(
                TextRange::new(else_line_range.end(), end.end()),
                desired_indentation,
                checker.locator(),
                checker.stylist(),
            )
            .unwrap();

            // we'll either delete the whole "else" line, or preserve the comment if there is one
            let else_deletion_range = if let Some(comment_token) =
                SimpleTokenizer::starts_at(else_range.start(), checker.locator().contents())
                    .find(|token| token.kind == SimpleTokenKind::Comment)
            {
                TextRange::new(else_range.start(), comment_token.start())
            } else {
                else_line_range
            };

            diagnostic.set_fix(Fix::applicable_edits(
                Edit::range_replacement(indented, TextRange::new(else_line_range.end(), end.end())),
                [Edit::range_deletion(else_deletion_range)],
                Applicability::Safe,
            ));
        }
    }

    checker.diagnostics.push(diagnostic);
}
