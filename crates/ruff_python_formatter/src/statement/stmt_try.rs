use crate::comments;
use crate::comments::leading_alternate_branch_comments;
use crate::comments::SourceComment;
use crate::other::except_handler_except_handler::ExceptHandlerKind;
use crate::prelude::*;
use crate::statement::FormatRefWithRule;
use crate::statement::Stmt;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatRuleWithOptions;
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{ExceptHandler, Ranged, StmtTry, StmtTryStar, Suite};

pub(super) enum AnyStatementTry<'a> {
    Try(&'a StmtTry),
    TryStar(&'a StmtTryStar),
}
impl<'a> AnyStatementTry<'a> {
    const fn except_handler_kind(&self) -> ExceptHandlerKind {
        match self {
            AnyStatementTry::Try(_) => ExceptHandlerKind::Regular,
            AnyStatementTry::TryStar(_) => ExceptHandlerKind::Starred,
        }
    }

    fn body(&self) -> &Suite {
        match self {
            AnyStatementTry::Try(try_) => &try_.body,
            AnyStatementTry::TryStar(try_) => &try_.body,
        }
    }

    fn handlers(&self) -> &[ExceptHandler] {
        match self {
            AnyStatementTry::Try(try_) => try_.handlers.as_slice(),
            AnyStatementTry::TryStar(try_) => try_.handlers.as_slice(),
        }
    }
    fn orelse(&self) -> &Suite {
        match self {
            AnyStatementTry::Try(try_) => &try_.orelse,
            AnyStatementTry::TryStar(try_) => &try_.orelse,
        }
    }

    fn finalbody(&self) -> &Suite {
        match self {
            AnyStatementTry::Try(try_) => &try_.finalbody,
            AnyStatementTry::TryStar(try_) => &try_.finalbody,
        }
    }
}

impl Ranged for AnyStatementTry<'_> {
    fn range(&self) -> TextRange {
        match self {
            AnyStatementTry::Try(with) => with.range(),
            AnyStatementTry::TryStar(with) => with.range(),
        }
    }
}

impl<'a> From<&'a StmtTry> for AnyStatementTry<'a> {
    fn from(value: &'a StmtTry) -> Self {
        AnyStatementTry::Try(value)
    }
}

impl<'a> From<&'a StmtTryStar> for AnyStatementTry<'a> {
    fn from(value: &'a StmtTryStar) -> Self {
        AnyStatementTry::TryStar(value)
    }
}

impl<'a> From<&AnyStatementTry<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyStatementTry<'a>) -> Self {
        match value {
            AnyStatementTry::Try(with) => AnyNodeRef::StmtTry(with),
            AnyStatementTry::TryStar(with) => AnyNodeRef::StmtTryStar(with),
        }
    }
}

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
    fn fmt(
        &self,
        item: &ExceptHandler,
        f: &mut Formatter<PyFormatContext<'_>>,
    ) -> FormatResult<()> {
        match item {
            ExceptHandler::ExceptHandler(x) => {
                x.format().with_options(self.except_handler_kind).fmt(f)
            }
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
impl Format<PyFormatContext<'_>> for AnyStatementTry<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments_info = f.context().comments().clone();
        let mut dangling_comments = comments_info.dangling_comments(self);
        let body = self.body();
        let handlers = self.handlers();
        let orelse = self.orelse();
        let finalbody = self.finalbody();

        write!(f, [text("try:"), block_indent(&body.format())])?;

        let mut previous_node = body.last();

        for handler in handlers {
            let handler_comments = comments_info.leading_comments(handler);
            write!(
                f,
                [
                    leading_alternate_branch_comments(handler_comments, previous_node),
                    &handler.format().with_options(self.except_handler_kind()),
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
}

impl FormatNodeRule<StmtTry> for FormatStmtTry {
    fn fmt_fields(&self, item: &StmtTry, f: &mut PyFormatter) -> FormatResult<()> {
        AnyStatementTry::from(item).fmt(f)
    }

    fn fmt_dangling_comments(&self, _node: &StmtTry, _f: &mut PyFormatter) -> FormatResult<()> {
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
