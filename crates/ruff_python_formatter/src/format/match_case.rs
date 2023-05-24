use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::MatchCase;
use crate::format::builders::block;
use crate::format::comments::{end_of_line_comments, leading_comments};
use crate::shared_traits::AsFormat;

pub struct FormatMatchCase<'a> {
    item: &'a MatchCase,
}

impl AsFormat<ASTFormatContext<'_>> for MatchCase {
    type Format<'a> = FormatMatchCase<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatMatchCase { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatMatchCase<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let MatchCase {
            pattern,
            guard,
            body,
        } = self.item;

        write!(f, [leading_comments(pattern)])?;

        write!(f, [text("case")])?;
        write!(f, [space(), pattern.format()])?;
        if let Some(guard) = &guard {
            write!(f, [space(), text("if"), space(), guard.format()])?;
        }
        write!(f, [text(":")])?;

        write!(f, [end_of_line_comments(body)])?;
        write!(f, [block_indent(&block(body))])?;

        Ok(())
    }
}
