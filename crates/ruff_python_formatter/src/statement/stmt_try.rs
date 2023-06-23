use crate::comments;
use crate::comments::leading_alternate_branch_comments;
use crate::comments::SourceComment;
use crate::prelude::*;
use crate::statement::FormatRefWithRule;
use crate::statement::Stmt;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AstNode;
use rustpython_parser::ast::{ExceptHandler, Ranged, StmtTry, Suite};

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
        match item {
            ExceptHandler::ExceptHandler(x) => x.format().fmt(f),
        }
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

        let mut previous_node = body.last();

        let mut first_handler = true;

        for handler in handlers {
            let handler_comments = if first_handler {
                first_handler = false;
                comments_info.leading_comments(handler)
            } else {
                let handler_comments_start = dangling_comments
                    .partition_point(|comment| comment.slice().end() <= handler.end());
                let (handler_comments, rest) = dangling_comments.split_at(handler_comments_start);
                dangling_comments = rest;
                handler_comments
            };

            write!(
                f,
                [
                    leading_alternate_branch_comments(handler_comments, previous_node),
                    &handler.format()
                ]
            )?;
            previous_node = match handler {
                ExceptHandler::ExceptHandler(handler) => handler.body.last(),
            };
        }

        (previous_node, dangling_comments) =
            format_case("else", orelse, previous_node, dangling_comments, f)?;

        format_case("finally", finalbody, previous_node, dangling_comments, f)?;

        write!(f, [comments::dangling_comments(dangling_comments)])
    }

    fn fmt_dangling_comments(&self, _node: &StmtTry, _f: &mut PyFormatter) -> FormatResult<()> {
        // dangling comments are formatted as part of fmt_fields
        Ok(())
    }
}

fn format_case<'a>(
    name: &'static str,
    body: &Suite,
    previous_node: Option<&Stmt>,
    dangling_comments: &'a [SourceComment],
    f: &mut PyFormatter,
) -> FormatResult<(Option<&'a Stmt>, &'a [SourceComment])> {
    Ok(if let Some(last) = body.last() {
        let case_comments_start =
            dangling_comments.partition_point(|comment| comment.slice().end() <= last.end());
        let (case_comments, rest) = dangling_comments.split_at(case_comments_start);
        write!(
            f,
            [leading_alternate_branch_comments(
                case_comments,
                previous_node
            )]
        )?;

        write!(f, [text(name), text(":"), block_indent(&body.format())])?;
        (None, rest)
    } else {
        (None, dangling_comments)
    })
}
