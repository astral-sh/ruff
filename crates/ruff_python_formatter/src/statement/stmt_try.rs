use crate::comments::{dangling_node_comments, leading_node_comments, trailing_node_comments};
use crate::prelude::*;
use crate::statement::FormatRefWithRule;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::{ExceptHandler, ExceptHandlerExceptHandler, StmtTry};

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
            write!(f, [space(), type_.format()])?;
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

        let joined_handlers = format_with(|f| {
            f.join_with(self::format_args!(text(","), soft_line_break_or_space()))
                .entries(handlers.iter().formatted())
                .finish()
        });
        write!(
            f,
            [
                text("try:"),
                hard_line_break(),
                block_indent(&body.format()),
                joined_handlers,
            ]
        )?;
        if !orelse.is_empty() {
            write!(f, [text("else:"), block_indent(&orelse.format())])?;
        }
        if !finalbody.is_empty() {
            write!(f, [text("finally:"), block_indent(&finalbody.format())])?;
        }
        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &StmtTry, _f: &mut PyFormatter) -> FormatResult<()> {
        // TODO(konstin): Needs node formatting or this leads to unstable formatting
        Ok(())
    }
}
