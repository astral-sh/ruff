use crate::comments::trailing_comments;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatRuleWithOptions;
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AstNode;
use rustpython_parser::ast::ExceptHandlerExceptHandler;

#[derive(Copy, Clone, Default)]
pub enum ExceptHandlerKind {
    #[default]
    Regular,
    Starred,
}

#[derive(Default)]
pub struct FormatExceptHandlerExceptHandler {
    except_handler_kind: ExceptHandlerKind,
}

impl FormatRuleWithOptions<ExceptHandlerExceptHandler, PyFormatContext<'_>>
    for FormatExceptHandlerExceptHandler
{
    type Options = ExceptHandlerKind;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.except_handler_kind = options;
        self
    }
}

impl FormatNodeRule<ExceptHandlerExceptHandler> for FormatExceptHandlerExceptHandler {
    fn fmt_fields(
        &self,
        item: &ExceptHandlerExceptHandler,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let ExceptHandlerExceptHandler {
            range: _,
            type_,
            name,
            body,
        } = item;

        let comments_info = f.context().comments().clone();
        let dangling_comments = comments_info.dangling_comments(item.as_any_node_ref());

        write!(
            f,
            [
                text("except"),
                match self.except_handler_kind {
                    ExceptHandlerKind::Regular => None,
                    ExceptHandlerKind::Starred => Some(text("*")),
                }
            ]
        )?;

        if let Some(type_) = type_ {
            write!(
                f,
                [space(), type_.format().with_options(Parenthesize::IfBreaks)]
            )?;
            if let Some(name) = name {
                write!(f, [space(), text("as"), space(), name.format()])?;
            }
        }
        write!(
            f,
            [
                text(":"),
                trailing_comments(dangling_comments),
                block_indent(&body.format())
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _node: &ExceptHandlerExceptHandler,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // dangling comments are formatted as part of fmt_fields
        Ok(())
    }
}
