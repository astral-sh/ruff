use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::identifier::{else_loop, else_try};
use ruff_python_ast::{ExceptHandler, Expr, Stmt, StmtExpr};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `else` clauses that only contains `pass` and `...` statements.
///
/// ## Why is this bad?
/// Such a clause is unnecessary.
///
/// ## Example
/// ```python
/// if foo:
///     bar()
/// else:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if foo:
///     bar()
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct NeedlessElse;

impl AlwaysFixableViolation for NeedlessElse {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Empty `else` clause".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove clause".to_string()
    }
}

/// RUF047
pub(crate) fn needless_else(checker: &mut Checker, stmt: &Stmt) {
    let (comment_ranges, source) = (checker.comment_ranges(), checker.source());

    let (body, report_range, remove_range, comment_range) = match stmt {
        Stmt::For(for_stmt) => {
            let (previous, else_body) = (&for_stmt.body, &for_stmt.orelse[..]);

            let Some(keyword_range) = else_loop(stmt, source) else {
                return;
            };
            let Some(last_stmt) = else_body.last() else {
                return;
            };
            let report_range = TextRange::new(keyword_range.start(), last_stmt.end());

            let Some((remove_range, comment_range)) =
                remove_and_comment_range(source, previous, else_body)
            else {
                return;
            };

            (else_body, report_range, remove_range, comment_range)
        }

        Stmt::While(while_stmt) => {
            let (previous, else_body) = (&while_stmt.body, &while_stmt.orelse[..]);

            let Some(keyword_range) = else_loop(stmt, source) else {
                return;
            };
            let Some(last_stmt) = else_body.last() else {
                return;
            };
            let report_range = TextRange::new(keyword_range.start(), last_stmt.end());

            let Some((remove_range, comment_range)) =
                remove_and_comment_range(source, previous, else_body)
            else {
                return;
            };

            (else_body, report_range, remove_range, comment_range)
        }

        Stmt::Try(try_stmt) => {
            let else_body = &try_stmt.orelse[..];

            let Some(keyword_range) = else_try(stmt, source) else {
                return;
            };
            let Some(last_stmt) = else_body.last() else {
                return;
            };
            let report_range = TextRange::new(keyword_range.start(), last_stmt.end());

            let mut previous = &try_stmt.body[..];

            if let [.., ExceptHandler::ExceptHandler(last_handler)] = &&try_stmt.handlers[..] {
                previous = &last_handler.body[..];
            };

            let Some((remove_range, comment_range)) =
                remove_and_comment_range(source, previous, else_body)
            else {
                return;
            };

            (else_body, report_range, remove_range, comment_range)
        }

        Stmt::If(if_stmt) => {
            let (previous, else_clause) = match &if_stmt.elif_else_clauses[..] {
                [.., elif, possibly_else] if possibly_else.test.is_none() => {
                    (&elif.body, possibly_else)
                }
                [possibly_else] if possibly_else.test.is_none() => (&if_stmt.body, possibly_else),
                _ => return,
            };

            let body = &else_clause.body[..];

            let Some((remove_range, comment_range)) =
                remove_and_comment_range(source, previous, body)
            else {
                return;
            };

            (body, else_clause.range, remove_range, comment_range)
        }

        _ => return,
    };

    if !body_is_empty(body) {
        return;
    }

    if comment_ranges.has_comments(&comment_range, source) {
        return;
    }

    let edit = Edit::range_deletion(remove_range);
    let fix = Fix::safe_edit(edit);

    let diagnostic = Diagnostic::new(NeedlessElse, report_range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn remove_and_comment_range(
    source: &str,
    previous_body: &[Stmt],
    else_body: &[Stmt],
) -> Option<(TextRange, TextRange)> {
    let preceding_stmt = previous_body.last()?;
    let else_last_stmt = else_body.last()?;

    let previous_block_end = source.line_end(preceding_stmt.end());
    let previous_block_end_plus_line_break = source.full_line_end(preceding_stmt.end());
    let else_block_end = source.line_end(else_last_stmt.end());

    let remove_range = TextRange::new(previous_block_end, else_block_end);
    let comment_range = TextRange::new(previous_block_end_plus_line_break, else_block_end);

    Some((remove_range, comment_range))
}

/// Whether `body` contains only `pass` or `...` statements.
fn body_is_empty(body: &[Stmt]) -> bool {
    if body.is_empty() {
        return false;
    }

    body.iter().all(|stmt| match stmt {
        Stmt::Pass(..) => true,
        Stmt::Expr(StmtExpr { value, .. }) => matches!(value.as_ref(), Expr::EllipsisLiteral(..)),
        _ => false,
    })
}
