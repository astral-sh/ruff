use rome_formatter::prelude::*;
use rome_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Comprehension;
use crate::shared_traits::AsFormat;

pub struct FormatComprehension<'a> {
    item: &'a Comprehension,
}

impl AsFormat<ASTFormatContext<'_>> for Comprehension {
    type Format<'a> = FormatComprehension<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatComprehension { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatComprehension<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let comprehension = self.item;

        write!(f, [soft_line_break_or_space()])?;
        write!(f, [text("for ")])?;
        write!(f, [comprehension.target.format()])?;
        write!(f, [text(" in ")])?;
        write!(f, [comprehension.iter.format()])?;
        for if_clause in &comprehension.ifs {
            write!(f, [soft_line_break_or_space()])?;
            write!(f, [text("if ")])?;
            write!(f, [if_clause.format()])?;
        }

        Ok(())
    }
}
