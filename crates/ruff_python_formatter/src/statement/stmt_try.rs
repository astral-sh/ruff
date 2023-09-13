use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::{ExceptHandler, StmtTry};
use ruff_text_size::Ranged;

use crate::comments;
use crate::comments::leading_alternate_branch_comments;
use crate::comments::SourceComment;
use crate::other::except_handler_except_handler::ExceptHandlerKind;
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader, ElseClause};
use crate::statement::{FormatRefWithRule, Stmt};

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
            orelse: _,
            finalbody: _,
            is_star,
            range: _,
        } = item;

        let comments_info = f.context().comments().clone();
        let mut dangling_comments = comments_info.dangling(item);

        (_, dangling_comments) = format_case(item, CaseKind::Try, None, dangling_comments, f)?;
        let mut previous_node = body.last();

        for handler in handlers {
            let handler_comments = comments_info.leading(handler);
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
            format_case(item, CaseKind::Else, previous_node, dangling_comments, f)?;

        format_case(item, CaseKind::Finally, previous_node, dangling_comments, f)?;

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
    try_statement: &'a StmtTry,
    kind: CaseKind,
    previous_node: Option<&'a Stmt>,
    dangling_comments: &'a [SourceComment],
    f: &mut PyFormatter,
) -> FormatResult<(Option<&'a Stmt>, &'a [SourceComment])> {
    let body = match kind {
        CaseKind::Try => &try_statement.body,
        CaseKind::Else => &try_statement.orelse,
        CaseKind::Finally => &try_statement.finalbody,
    };

    Ok(if let Some(last) = body.last() {
        let case_comments_start =
            dangling_comments.partition_point(|comment| comment.end() <= last.end());
        let (case_comments, rest) = dangling_comments.split_at(case_comments_start);
        let partition_point =
            case_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_case_comments, trailing_case_comments) =
            case_comments.split_at(partition_point);

        let header = match kind {
            CaseKind::Try => ClauseHeader::Try(try_statement),
            CaseKind::Else => ClauseHeader::OrElse(ElseClause::Try(try_statement)),
            CaseKind::Finally => ClauseHeader::TryFinally(try_statement),
        };

        write!(
            f,
            [
                clause_header(header, trailing_case_comments, &token(kind.keyword()))
                    .with_leading_comments(leading_case_comments, previous_node),
                clause_body(body, trailing_case_comments),
            ]
        )?;
        (Some(last), rest)
    } else {
        (previous_node, dangling_comments)
    })
}

#[derive(Copy, Clone)]
enum CaseKind {
    Try,
    Else,
    Finally,
}

impl CaseKind {
    fn keyword(self) -> &'static str {
        match self {
            CaseKind::Try => "try",
            CaseKind::Else => "else",
            CaseKind::Finally => "finally",
        }
    }
}
