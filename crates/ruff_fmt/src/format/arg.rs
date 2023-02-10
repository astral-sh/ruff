use rome_formatter::prelude::*;
use rome_formatter::write;
use rome_text_size::TextSize;

use crate::context::ASTFormatContext;
use crate::cst::Arg;
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

        write!(f, [dynamic_text(&arg.node.arg, TextSize::default())])?;
        if let Some(annotation) = &arg.node.annotation {
            write!(f, [text(": ")])?;
            write!(f, [annotation.format()])?;
        }

        Ok(())
    }
}
