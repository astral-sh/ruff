use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Keyword;
use crate::format::comments::{end_of_line_comments, leading_comments, trailing_comments};
use crate::shared_traits::AsFormat;

pub struct FormatKeyword<'a> {
    item: &'a Keyword,
}

impl AsFormat<ASTFormatContext<'_>> for Keyword {
    type Format<'a> = FormatKeyword<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatKeyword { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatKeyword<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let keyword = self.item;

        write!(f, [leading_comments(keyword)])?;
        if let Some(arg) = &keyword.arg {
            write!(f, [dynamic_text(arg, None)])?;
            write!(f, [text("=")])?;
            write!(f, [keyword.value.format()])?;
        } else {
            write!(f, [text("**")])?;
            write!(f, [keyword.value.format()])?;
        }
        write!(f, [end_of_line_comments(keyword)])?;
        write!(f, [trailing_comments(keyword)])?;

        Ok(())
    }
}
