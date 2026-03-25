use std::cmp::Ordering;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{comment_indentation_after, is_stub_body};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{Stmt, StmtTry};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `finally` clauses that only contain `pass` or `...` statements
///
/// ## Why is this bad?
/// An empty `finally` clause is a no-op and adds unnecessary noise.
/// If the `try` statement has `except` or `else` clauses, the `finally`
/// clause can simply be removed. If it's a bare `try/finally`, the entire
/// `try` statement can be replaced with its body
///
/// ## Example
/// ```python
/// try:
///     foo()
/// except Exception:
///     bar()
/// finally:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// try:
///     foo()
/// except Exception:
///     bar()
/// ```
///
/// ## Example
/// ```python
/// try:
///     foo()
/// finally:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// foo()
/// ```
///
/// ## See also
/// - [`needless-else`][RUF047]: Removes empty `else` clauses on `try` (and
///   other statements). Both rules can fire on the same `try` statement
/// - [`suppressible-exception`][SIM105]: Rewrites `try/except: pass` to
///   `contextlib.suppress()`. Won't apply while a `finally` clause is present,
///   so RUF072 must remove it first
/// - [`useless-try-except`][TRY203]: Flags `try/except` that only re-raises
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct UselessFinally;

impl Violation for UselessFinally {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Empty `finally` clause".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove the `finally` clause".to_string())
    }
}

/// RUF072
pub(crate) fn useless_finally(checker: &Checker, try_stmt: &StmtTry) {
    let finalbody = &try_stmt.finalbody;

    if !is_stub_body(finalbody) {
        return;
    }

    let source = checker.source();
    let tokens = checker.tokens();

    // `is_stub_body` guarantees at least one statement in finalbody
    let last_finalbody_stmt = finalbody.last().unwrap();
    let (preceding_end, preceding_stmt) = preceding_clause_info(try_stmt);

    let Some(finally_start) = tokens
        .in_range(TextRange::new(preceding_end, last_finalbody_stmt.end()))
        .iter()
        .find(|token| token.kind() == TokenKind::Finally)
        .map(Ranged::start)
    else {
        return;
    };

    let finally_range = TextRange::new(finally_start, last_finalbody_stmt.end());

    let has_comments = finally_contains_comments(
        preceding_stmt,
        preceding_end,
        last_finalbody_stmt,
        finally_range,
        checker,
    );

    let mut diagnostic = checker.report_diagnostic(UselessFinally, finally_range);

    if has_comments {
        return;
    }

    let is_bare_try_finally = try_stmt.handlers.is_empty() && try_stmt.orelse.is_empty();

    if is_bare_try_finally {
        // bare `try/finally: pass` — unwrap the try body
        let (Some(first_body_stmt), Some(last_body_stmt)) =
            (try_stmt.body.first(), try_stmt.body.last())
        else {
            return;
        };

        let try_indentation = indentation(source, try_stmt).unwrap_or_default();

        let Ok(adjusted) = crate::fix::edits::adjust_indentation(
            TextRange::new(
                source.line_start(first_body_stmt.start()),
                source.full_line_end(last_body_stmt.end()),
            ),
            try_indentation,
            checker.locator(),
            checker.indexer(),
            checker.stylist(),
        ) else {
            return;
        };

        let try_line_start = source.line_start(try_stmt.start());
        let finally_full_end = source.full_line_end(last_finalbody_stmt.end());
        let edit =
            Edit::range_replacement(adjusted, TextRange::new(try_line_start, finally_full_end));
        diagnostic.set_fix(Fix::safe_edit(edit));
    } else {
        // `try/except/finally: pass` — remove the finally clause
        let finally_line_start = source.line_start(finally_start);
        let finally_full_end = source.full_line_end(last_finalbody_stmt.end());
        let edit = Edit::range_deletion(TextRange::new(finally_line_start, finally_full_end));
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
}

/// Returns the end offset of the clause preceding `finally` and the last
/// statement in that clause's body (used for comment indentation checks)
fn preceding_clause_info(try_stmt: &StmtTry) -> (TextSize, Option<&Stmt>) {
    if let Some(last) = try_stmt.orelse.last() {
        return (last.end(), Some(last));
    }
    if let Some(handler) = try_stmt.handlers.last() {
        let last_stmt = handler.as_except_handler().and_then(|h| h.body.last());
        return (handler.end(), last_stmt);
    }
    match try_stmt.body.last() {
        Some(last) => (last.end(), Some(last)),
        None => (try_stmt.start(), None),
    }
}

fn finally_contains_comments(
    preceding_stmt: Option<&Stmt>,
    preceding_end: TextSize,
    finalbody_stmt: &Stmt,
    finally_range: TextRange,
    checker: &Checker,
) -> bool {
    let source = checker.source();
    let finally_full_end = source.full_line_end(finally_range.end());
    let commentable_range = TextRange::new(finally_range.start(), finally_full_end);

    // A comment after the `finally` keyword or after the dummy statement
    if checker.comment_ranges().intersects(commentable_range) {
        return true;
    }

    let Some(preceding_stmt) = preceding_stmt else {
        return false;
    };

    finally_has_preceding_comment(preceding_stmt, preceding_end, finally_range, checker)
        || finally_has_trailing_comment(finalbody_stmt, finally_full_end, checker)
}

/// Returns `true` if the `finally` clause header has a leading own-line comment
fn finally_has_preceding_comment(
    preceding_stmt: &Stmt,
    preceding_end: TextSize,
    finally_range: TextRange,
    checker: &Checker,
) -> bool {
    let (tokens, source) = (checker.tokens(), checker.source());
    let before_finally_full_end = source.full_line_end(preceding_end);
    let preceding_indentation = indentation(source, preceding_stmt)
        .unwrap_or_default()
        .text_len();

    for token in tokens.in_range(TextRange::new(
        before_finally_full_end,
        finally_range.start(),
    )) {
        if token.kind() != TokenKind::Comment {
            continue;
        }

        let comment_indentation =
            comment_indentation_after(preceding_stmt.into(), token.range(), source);

        match comment_indentation.cmp(&preceding_indentation) {
            Ordering::Greater | Ordering::Equal => continue,
            Ordering::Less => return true,
        }
    }

    false
}

/// Returns `true` if the `finally` branch has a trailing own-line comment
fn finally_has_trailing_comment(
    last_finally_stmt: &Stmt,
    finally_full_end: TextSize,
    checker: &Checker,
) -> bool {
    let (tokens, source) = (checker.tokens(), checker.source());
    let preceding_indentation = indentation(source, last_finally_stmt)
        .unwrap_or_default()
        .text_len();

    for token in tokens.after(finally_full_end) {
        match token.kind() {
            TokenKind::Comment => {
                let comment_indentation =
                    comment_indentation_after(last_finally_stmt.into(), token.range(), source);

                match comment_indentation.cmp(&preceding_indentation) {
                    Ordering::Greater | Ordering::Equal => return true,
                    Ordering::Less => break,
                }
            }

            TokenKind::NonLogicalNewline
            | TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent => {}

            _ => break,
        }
    }

    false
}
