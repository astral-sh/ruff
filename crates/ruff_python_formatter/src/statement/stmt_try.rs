use ruff_formatter::FormatRuleWithOptions;
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::{ExceptHandler, Ranged, StmtTry, Suite};

use crate::comments;
use crate::comments::{leading_alternate_branch_comments, trailing_comments, SourceComment};
use crate::other::except_handler_except_handler::ExceptHandlerKind;
use crate::prelude::*;
use crate::statement::{FormatRefWithRule, Stmt};
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatStmtTry;

#[derive(Copy, Clone, Default)]
pub struct FormatExceptHandler {
    except_handler_kind: ExceptHandlerKind,
}

impl FormatRuleWithOptions<ExceptHandler, PyFormatContext<'_>> for FormatExceptHandler {
    type Options = ExceptHandlerKind;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.except_handler_kind = options;
        self
    }
}

impl FormatRule<ExceptHandler, PyFormatContext<'_>> for FormatExceptHandler {
    fn fmt(&self, item: &ExceptHandler, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            ExceptHandler::ExceptHandler(except_handler) => except_handler
                .format()
                .with_options(self.except_handler_kind)
                .fmt(f),
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
            body,
            handlers,
            orelse,
            finalbody,
            is_star,
            range: _,
        } = item;

        let comments_info = f.context().comments().clone();
        let mut dangling_comments = comments_info.dangling_comments(item);

        (_, dangling_comments) = format_case("try", body, None, dangling_comments, f)?;
        let mut previous_node = body.last();

        for handler in handlers {
            let handler_comments = comments_info.leading_comments(handler);
            write!(
                f,
                [
                    leading_alternate_branch_comments(handler_comments, previous_node),
                    &handler.format().with_options(if *is_star {
                        ExceptHandlerKind::Starred
                    } else {
                        ExceptHandlerKind::Regular
                    }),
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

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // dangling comments are formatted as part of AnyStatementTry::fmt
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
        let partition_point =
            case_comments.partition_point(|comment| comment.line_position().is_own_line());
        write!(
            f,
            [
                leading_alternate_branch_comments(&case_comments[..partition_point], previous_node),
                text(name),
                text(":"),
                trailing_comments(&case_comments[partition_point..]),
                block_indent(&body.format())
            ]
        )?;
        (None, rest)
    } else {
        (None, dangling_comments)
    })
}
