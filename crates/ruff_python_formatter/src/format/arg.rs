use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Arg;
use crate::format::comments::end_of_line_comments;
use crate::shared_traits::AsFormat;

pub struct FormatArg<'a> {
    item: &'a Arg,
}

impl AsFormat<ASTFormatContext<'_>> for Arg {
    type Format<'a> = FormatArg<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatArg { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatArg<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let arg = self.item;

        write!(f, [dynamic_text(&arg.arg, None)])?;
        if let Some(annotation) = &arg.annotation {
            write!(f, [text(": ")])?;
            write!(f, [annotation.format()])?;
        }
        write!(f, [end_of_line_comments(arg)])?;

        Ok(())
    }
}
