use ruff_formatter::prelude::*;
use ruff_formatter::write;
use ruff_text_size::TextSize;

use crate::context::ASTFormatContext;
use crate::cst::Alias;
use crate::shared_traits::AsFormat;

pub struct FormatAlias<'a> {
    item: &'a Alias,
}

impl AsFormat<ASTFormatContext<'_>> for Alias {
    type Format<'a> = FormatAlias<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatAlias { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatAlias<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let alias = self.item;

        write!(f, [dynamic_text(&alias.node.name, TextSize::default())])?;
        if let Some(asname) = &alias.node.asname {
            write!(f, [text(" as ")])?;
            write!(f, [dynamic_text(asname, TextSize::default())])?;
        }

        Ok(())
    }
}
