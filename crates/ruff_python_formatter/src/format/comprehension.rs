use crate::prelude::*;
use ruff_formatter::write;

use crate::cst::Comprehension;

pub(crate) struct FormatComprehension<'a> {
    item: &'a Comprehension,
}

impl AsFormat<ASTFormatContext<'_>> for Comprehension {
    type Format<'a> = FormatComprehension<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatComprehension { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatComprehension<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let comprehension = self.item;

        write!(f, [soft_line_break_or_space()])?;
        if comprehension.is_async > 0 {
            write!(f, [text("async")])?;
            write!(f, [space()])?;
        }
        write!(f, [text("for")])?;
        write!(f, [space()])?;
        // TODO(charlie): If this is an unparenthesized tuple, we need to avoid expanding it.
        // Should this be set on the context?
        write!(f, [group(&comprehension.target.format())])?;
        write!(f, [space()])?;
        write!(f, [text("in")])?;
        write!(f, [space()])?;
        write!(f, [group(&comprehension.iter.format())])?;
        for if_clause in &comprehension.ifs {
            write!(f, [soft_line_break_or_space()])?;
            write!(f, [text("if")])?;
            write!(f, [space()])?;
            write!(f, [if_clause.format()])?;
        }

        Ok(())
    }
}
