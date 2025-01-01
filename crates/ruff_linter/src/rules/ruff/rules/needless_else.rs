use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Stmt, StmtExpr, StmtFor, StmtIf, StmtTry, StmtWhile};
use ruff_python_parser::{TokenKind, Tokens};
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
pub(crate) fn needless_else(checker: &mut Checker, stmt: AnyNodeWithOrElse) {
    let source = checker.source();

    let else_body = stmt.else_body();

    if !body_is_useless(else_body) {
        return;
    }

    let Some(else_range) = stmt.else_range(checker.tokens()) else {
        return;
    };

    let before_else = stmt.body_before_else();
    let Some(preceding_stmt) = before_else.last() else {
        return;
    };

    let before_else_full_end = source.full_line_end(preceding_stmt.end());
    let else_full_end = source.full_line_end(else_range.end());

    // Preserve else blocks that contain a comment.
    if checker
        .comment_ranges()
        .intersects(TextRange::new(before_else_full_end, else_full_end))
    {
        return;
    }

    let remove_range = TextRange::new(before_else_full_end, else_full_end);

    let edit = Edit::range_deletion(remove_range);
    let fix = Fix::safe_edit(edit);

    let diagnostic = Diagnostic::new(NeedlessElse, else_range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

/// Whether `body` contains only `pass` or `...` statements.
fn body_is_useless(body: &[Stmt]) -> bool {
    match body {
        [Stmt::Pass(_)] => true,
        [Stmt::Expr(StmtExpr { value, .. })] => value.is_ellipsis_literal_expr(),
        _ => false,
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum AnyNodeWithOrElse<'a> {
    While(&'a StmtWhile),
    For(&'a StmtFor),
    Try(&'a StmtTry),
    If(&'a StmtIf),
}

impl<'a> AnyNodeWithOrElse<'a> {
    /// Returns the range from the `else` keyword to the last statement
    /// in it's block.
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

    // Returns the `else` suite. Defaults to an empty suite if the statement
    // has no `else` block.
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
