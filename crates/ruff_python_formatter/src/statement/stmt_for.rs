use crate::comments::{leading_alternate_branch_comments, trailing_comments};
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Expr, Ranged, Stmt, StmtAsyncFor, StmtFor, Suite};
use ruff_text_size::TextRange;

#[derive(Debug)]
struct ExprTupleWithoutParentheses<'a>(&'a Expr);

impl Format<PyFormatContext<'_>> for ExprTupleWithoutParentheses<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self.0 {
            Expr::Tuple(expr_tuple) => expr_tuple
                .format()
                .with_options(TupleParentheses::NeverPreserve)
                .fmt(f),
            other => maybe_parenthesize_expression(other, self.0, Parenthesize::IfBreaks).fmt(f),
        }
    }
}

#[derive(Default)]
pub struct FormatStmtFor;

pub(super) enum AnyStatementFor<'a> {
    For(&'a StmtFor),
    AsyncFor(&'a StmtAsyncFor),
}

impl<'a> AnyStatementFor<'a> {
    const fn is_async(&self) -> bool {
        matches!(self, AnyStatementFor::AsyncFor(_))
    }

    fn target(&self) -> &Expr {
        match self {
            AnyStatementFor::For(stmt) => &stmt.target,
            AnyStatementFor::AsyncFor(stmt) => &stmt.target,
        }
    }

    #[allow(clippy::iter_not_returning_iterator)]
    fn iter(&self) -> &Expr {
        match self {
            AnyStatementFor::For(stmt) => &stmt.iter,
            AnyStatementFor::AsyncFor(stmt) => &stmt.iter,
        }
    }

    fn body(&self) -> &Suite {
        match self {
            AnyStatementFor::For(stmt) => &stmt.body,
            AnyStatementFor::AsyncFor(stmt) => &stmt.body,
        }
    }

    fn orelse(&self) -> &Suite {
        match self {
            AnyStatementFor::For(stmt) => &stmt.orelse,
            AnyStatementFor::AsyncFor(stmt) => &stmt.orelse,
        }
    }
}

impl Ranged for AnyStatementFor<'_> {
    fn range(&self) -> TextRange {
        match self {
            AnyStatementFor::For(stmt) => stmt.range(),
            AnyStatementFor::AsyncFor(stmt) => stmt.range(),
        }
    }
}

impl<'a> From<&'a StmtFor> for AnyStatementFor<'a> {
    fn from(value: &'a StmtFor) -> Self {
        AnyStatementFor::For(value)
    }
}

impl<'a> From<&'a StmtAsyncFor> for AnyStatementFor<'a> {
    fn from(value: &'a StmtAsyncFor) -> Self {
        AnyStatementFor::AsyncFor(value)
    }
}

impl<'a> From<&AnyStatementFor<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyStatementFor<'a>) -> Self {
        match value {
            AnyStatementFor::For(stmt) => AnyNodeRef::StmtFor(stmt),
            AnyStatementFor::AsyncFor(stmt) => AnyNodeRef::StmtAsyncFor(stmt),
        }
    }
}

impl Format<PyFormatContext<'_>> for AnyStatementFor<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let target = self.target();
        let iter = self.iter();
        let body = self.body();
        let orelse = self.orelse();

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(self);
        let body_start = body.first().map_or(iter.end(), Stmt::start);
        let or_else_comments_start =
            dangling_comments.partition_point(|comment| comment.slice().end() < body_start);

        let (trailing_condition_comments, or_else_comments) =
            dangling_comments.split_at(or_else_comments_start);

        write!(
            f,
            [
                self.is_async()
                    .then_some(format_args![text("async"), space()]),
                text("for"),
                space(),
                ExprTupleWithoutParentheses(target),
                space(),
                text("in"),
                space(),
                maybe_parenthesize_expression(iter, self, Parenthesize::IfBreaks),
                text(":"),
                trailing_comments(trailing_condition_comments),
                block_indent(&body.format())
            ]
        )?;

        if orelse.is_empty() {
            debug_assert!(or_else_comments.is_empty());
        } else {
            // Split between leading comments before the `else` keyword and end of line comments at the end of
            // the `else:` line.
            let trailing_start =
                or_else_comments.partition_point(|comment| comment.line_position().is_own_line());
            let (leading, trailing) = or_else_comments.split_at(trailing_start);

            write!(
                f,
                [
                    leading_alternate_branch_comments(leading, body.last()),
                    text("else:"),
                    trailing_comments(trailing),
                    block_indent(&orelse.format())
                ]
            )?;
        }

        Ok(())
    }
}

impl FormatNodeRule<StmtFor> for FormatStmtFor {
    fn fmt_fields(&self, item: &StmtFor, f: &mut PyFormatter) -> FormatResult<()> {
        AnyStatementFor::from(item).fmt(f)
    }

    fn fmt_dangling_comments(&self, _node: &StmtFor, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
