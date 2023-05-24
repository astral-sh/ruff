use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Alias;
use crate::format::comments::end_of_line_comments;
use crate::shared_traits::AsFormat;

pub struct FormatAlias<'a> {
    item: &'a Alias,
}

impl AsFormat<ASTFormatContext> for Alias {
    type Format<'a> = FormatAlias<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatAlias { item: self }
    }
}

impl Format<ASTFormatContext> for FormatAlias<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let alias = self.item;

        write!(f, [dynamic_text(&alias.name, None)])?;
        if let Some(asname) = &alias.asname {
            write!(f, [text(" as ")])?;
            write!(f, [dynamic_text(asname, None)])?;
        }

        write!(f, [end_of_line_comments(alias)])?;

        Ok(())
    }
}
