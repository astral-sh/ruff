use std::cmp::Ordering;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::comment_indentation_after;
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{Stmt, StmtExpr, StmtFor, StmtIf, StmtTry, StmtWhile};
use ruff_python_parser::{TokenKind, Tokens};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `else` clauses that only contains `pass` and `...` statements.
///
/// ## Why is this bad?
/// Such an else clause does nothing and can be removed.
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
        "Remove `else` clause".to_string()
    }
}

/// RUF047
pub(crate) fn needless_else(checker: &mut Checker, stmt: AnyNodeWithOrElse) {
    let source = checker.source();
    let tokens = checker.tokens();

    let else_body = stmt.else_body();

    if !body_is_no_op(else_body) {
        return;
    }

    let Some(else_range) = stmt.else_range(tokens) else {
        return;
    };

    if else_branch_should_not_be_reported(stmt, else_range, checker) {
        return;
    }

    let else_line_start = source.line_start(else_range.start());
    let else_full_end = source.full_line_end(else_range.end());
    let remove_range = TextRange::new(else_line_start, else_full_end);

    let edit = Edit::range_deletion(remove_range);
    let fix = Fix::safe_edit(edit);

    let diagnostic = Diagnostic::new(NeedlessElse, else_range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

/// Whether `body` contains only one `pass` or `...` statement.
fn body_is_no_op(body: &[Stmt]) -> bool {
    match body {
        [Stmt::Pass(_)] => true,
        [Stmt::Expr(StmtExpr { value, .. })] => value.is_ellipsis_literal_expr(),
        _ => false,
    }
}

fn else_branch_should_not_be_reported(
    stmt: AnyNodeWithOrElse,
    else_range: TextRange,
    checker: &Checker,
) -> bool {
    let Some(preceding_stmt) = stmt.body_before_else().last() else {
        return true;
    };
    let Some(else_last_stmt) = stmt.else_body().last() else {
        return true;
    };

    if else_branch_has_preceding_comment(preceding_stmt, else_range, checker) {
        return true;
    }

    let else_full_end = checker.source().full_line_end(else_range.end());
    let commentable_range = TextRange::new(else_range.start(), else_full_end);

    if checker.comment_ranges().intersects(commentable_range) {
        return true;
    }

    if else_branch_has_trailing_comment(else_last_stmt, else_full_end, checker) {
        return true;
    }

    false
}

fn else_branch_has_preceding_comment(
    preceding_stmt: &Stmt,
    else_range: TextRange,
    checker: &Checker,
) -> bool {
    let (tokens, source) = (checker.tokens(), checker.source());

    let before_else_full_end = source.full_line_end(preceding_stmt.end());
    let else_start = else_range.start();

    let preceding_indentation = indentation(source, &preceding_stmt)
        .unwrap_or_default()
        .text_len();

    for token in tokens.in_range(TextRange::new(before_else_full_end, else_start)) {
        if token.kind() != TokenKind::Comment {
            continue;
        }

        let comment_indentation =
            comment_indentation_after(preceding_stmt.into(), token.range(), source);

        match comment_indentation.cmp(&preceding_indentation) {
            Ordering::Greater => continue,
            Ordering::Equal => return true,
            Ordering::Less => return true,
        }
    }

    false
}

fn else_branch_has_trailing_comment(
    preceding_stmt: &Stmt,
    else_full_end: TextSize,
    checker: &Checker,
) -> bool {
    let (tokens, source) = (checker.tokens(), checker.source());

    let preceding_indentation = indentation(source, &preceding_stmt)
        .unwrap_or_default()
        .text_len();

    for token in tokens.after(else_full_end) {
        match token.kind() {
            TokenKind::Comment => {
                let comment_indentation =
                    comment_indentation_after(preceding_stmt.into(), token.range(), source);

                match comment_indentation.cmp(&preceding_indentation) {
                    Ordering::Greater => return true,
                    Ordering::Equal => return true,
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

#[derive(Copy, Clone, Debug)]
pub(crate) enum AnyNodeWithOrElse<'a> {
    While(&'a StmtWhile),
    For(&'a StmtFor),
    Try(&'a StmtTry),
    If(&'a StmtIf),
}

impl<'a> AnyNodeWithOrElse<'a> {
    /// Returns the range from the `else` keyword to the last statement in its block.
    fn else_range(self, tokens: &Tokens) -> Option<TextRange> {
        match self {
            Self::For(_) | Self::While(_) | Self::Try(_) => {
                let before_else = self.body_before_else();

                let else_body = self.else_body();
                let end = else_body.last()?.end();

                let start = tokens
                    .in_range(TextRange::new(before_else.last()?.end(), end))
                    .iter()
                    .find(|token| token.kind() == TokenKind::Else)?
                    .start();

                Some(TextRange::new(start, end))
            }

            Self::If(StmtIf {
                elif_else_clauses, ..
            }) => elif_else_clauses
                .last()
                .filter(|clause| clause.test.is_none())
                .map(Ranged::range),
        }
    }

    /// Returns the suite before the else block.
    fn body_before_else(self) -> &'a [Stmt] {
        match self {
            Self::Try(StmtTry { body, handlers, .. }) => handlers
                .last()
                .and_then(|handler| handler.as_except_handler())
                .map(|handler| &handler.body)
                .unwrap_or(body),

            Self::While(StmtWhile { body, .. }) | Self::For(StmtFor { body, .. }) => body,

            Self::If(StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => elif_else_clauses
                .iter()
                .rev()
                .find(|clause| clause.test.is_some())
                .map(|clause| &*clause.body)
                .unwrap_or(body),
        }
    }

    /// Returns the `else` suite.
    /// Defaults to an empty suite if the statement has no `else` block.
    fn else_body(self) -> &'a [Stmt] {
        match self {
            Self::While(StmtWhile { orelse, .. })
            | Self::For(StmtFor { orelse, .. })
            | Self::Try(StmtTry { orelse, .. }) => orelse,

            Self::If(StmtIf {
                elif_else_clauses, ..
            }) => elif_else_clauses
                .last()
                .filter(|clause| clause.test.is_none())
                .map(|clause| &*clause.body)
                .unwrap_or_default(),
        }
    }
}

impl<'a> From<&'a StmtFor> for AnyNodeWithOrElse<'a> {
    fn from(value: &'a StmtFor) -> Self {
        Self::For(value)
    }
}

impl<'a> From<&'a StmtWhile> for AnyNodeWithOrElse<'a> {
    fn from(value: &'a StmtWhile) -> Self {
        Self::While(value)
    }
}

impl<'a> From<&'a StmtIf> for AnyNodeWithOrElse<'a> {
    fn from(value: &'a StmtIf) -> Self {
        Self::If(value)
    }
}

impl<'a> From<&'a StmtTry> for AnyNodeWithOrElse<'a> {
    fn from(value: &'a StmtTry) -> Self {
        Self::Try(value)
    }
}
