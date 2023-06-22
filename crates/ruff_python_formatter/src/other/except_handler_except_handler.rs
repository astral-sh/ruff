use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExceptHandlerExceptHandler;

#[derive(Default)]
pub struct FormatExceptHandlerExceptHandler;

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

        write!(f, [text("except")])?;

        if let Some(type_) = type_ {
            write!(
                f,
                [space(), type_.format().with_options(Parenthesize::IfBreaks)]
            )?;
            if let Some(name) = name {
                write!(f, [space(), text("as"), space(), name.format()])?;
            }
        }
        write!(f, [text(":"), block_indent(&body.format())])
    }
}
