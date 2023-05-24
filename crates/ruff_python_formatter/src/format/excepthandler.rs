use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::{Excepthandler, ExcepthandlerKind};
use crate::format::builders::block;
use crate::format::comments::end_of_line_comments;
use crate::shared_traits::AsFormat;

pub struct FormatExcepthandler<'a> {
    item: &'a Excepthandler,
}

impl AsFormat<ASTFormatContext<'_>> for Excepthandler {
    type Format<'a> = FormatExcepthandler<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatExcepthandler { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatExcepthandler<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let excepthandler = self.item;
        let ExcepthandlerKind::ExceptHandler { type_, name, body } = &excepthandler.node;

        write!(f, [text("except")])?;
        if let Some(type_) = &type_ {
            write!(f, [space(), type_.format()])?;
            if let Some(name) = &name {
                write!(f, [space(), text("as"), space(), dynamic_text(name, None)])?;
            }
        }
        write!(f, [text(":")])?;
        write!(f, [end_of_line_comments(excepthandler)])?;

        write!(f, [block_indent(&block(body))])?;

        Ok(())
    }
}
