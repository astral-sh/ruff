use crate::comments;
use crate::comments::{
    dangling_node_comments, leading_comments, leading_node_comments, trailing_node_comments,
};
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::FormatRefWithRule;
use crate::trivia::lines_before;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AstNode;
use rustpython_parser::ast::{ExceptHandler, ExceptHandlerExceptHandler, Ranged, StmtTry};

#[derive(Default)]
pub struct FormatStmtTry;

#[derive(Copy, Clone, Default)]
pub struct FormatExceptHandler;

impl FormatRule<ExceptHandler, PyFormatContext<'_>> for FormatExceptHandler {
    fn fmt(
        &self,
        item: &ExceptHandler,
        f: &mut Formatter<PyFormatContext<'_>>,
    ) -> FormatResult<()> {
        let ExceptHandler::ExceptHandler(except_handler_except_handler) = item;
        let ExceptHandlerExceptHandler {
            range: _,
            type_,
            name,
            body,
        } = except_handler_except_handler;

        leading_node_comments(except_handler_except_handler).fmt(f)?;

        write!(f, [text("except")])?;

        if let Some(type_) = type_ {
            write!(
                f,
                [space(), type_.format().with_options(Parenthesize::IfBreaks)]
            )?;
            if let Some(name) = name {
                write!(
                    f,
                    [
                        space(),
                        text("as"),
                        space(),
                        dynamic_text(name.as_str(), None)
                    ]
                )?;
            }
        }
        write!(f, [text(":"), block_indent(&body.format())])?;

        dangling_node_comments(except_handler_except_handler).fmt(f)?;
        trailing_node_comments(except_handler_except_handler).fmt(f)
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for ExceptHandler {
    type Format<'a> = FormatRefWithRule<
        'a,
        ExceptHandler,
        FormatExceptHandler,
        PyFormatContext<'ast>,
    > where Self: 'a;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatExceptHandler::default())
    }
}

impl FormatNodeRule<StmtTry> for FormatStmtTry {
    fn fmt_fields(&self, item: &StmtTry, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtTry {
            range: _,
            body,
            handlers,
            orelse,
            finalbody,
        } = item;

        let comments_info = f.context().comments().clone();
        let mut dangling_comments = comments_info.dangling_comments(item.as_any_node_ref());

        write!(
            f,
            [
                text("try:"),
                hard_line_break(),
                block_indent(&body.format()),
            ]
        )?;

        for handler in handlers {
            let handler_comments_start =
                dangling_comments.partition_point(|comment| comment.slice().end() <= handler.end());
            let (handler_comments, rest) = dangling_comments.split_at(handler_comments_start);
            dangling_comments = rest;

            if lines_before(
                handler_comments
                    .first()
                    .map(|c| c.slice().start())
                    .unwrap_or(handler.range().start()),
                f.context().contents(),
            ) > 1
            {
                write!(f, [empty_line()])?;
            }

            write!(f, [leading_comments(handler_comments), &handler.format()])?;
        }

        if let [.., last] = &orelse[..] {
            let orelse_comments_start =
                dangling_comments.partition_point(|comment| comment.slice().end() <= last.end());
            let (orelse_comments, rest) = dangling_comments.split_at(orelse_comments_start);
            dangling_comments = rest;
            if let Some(first_orelse_comment) = orelse_comments.first() {
                if lines_before(first_orelse_comment.slice().start(), f.context().contents()) > 1 {
                    write!(f, [empty_line()])?;
                }
            }
            write!(
                f,
                [
                    leading_comments(orelse_comments),
                    text("else:"),
                    block_indent(&orelse.format())
                ]
            )?;
        }
        if let [.., last] = &finalbody[..] {
            let finally_comments_start =
                dangling_comments.partition_point(|comment| comment.slice().end() <= last.end());
            let (finally_comments, rest) = dangling_comments.split_at(finally_comments_start);
            dangling_comments = rest;
            if let Some(first_finally_comment) = finally_comments.first() {
                if lines_before(
                    first_finally_comment.slice().start(),
                    f.context().contents(),
                ) > 1
                {
                    write!(f, [empty_line()])?;
                }
            }
            write!(
                f,
                [
                    leading_comments(finally_comments),
                    text("finally:"),
                    block_indent(&finalbody.format())
                ]
            )?;
        }
        write!(f, [comments::dangling_comments(dangling_comments)])
    }

    fn fmt_dangling_comments(&self, _node: &StmtTry, _f: &mut PyFormatter) -> FormatResult<()> {
        // TODO(konstin): Needs node formatting or this leads to unstable formatting
        Ok(())
    }
}
