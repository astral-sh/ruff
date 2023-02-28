use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::{BoolOp, BoolOpKind};
use crate::format::comments::{end_of_line_comments, leading_comments, trailing_comments};
use crate::shared_traits::AsFormat;

pub struct FormatBoolOp<'a> {
    item: &'a BoolOp,
}

impl AsFormat<ASTFormatContext<'_>> for BoolOp {
    type Format<'a> = FormatBoolOp<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatBoolOp { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatBoolOp<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let boolop = self.item;

        write!(f, [leading_comments(boolop)])?;
        write!(
            f,
            [text(match boolop.node {
                BoolOpKind::And => "and",
                BoolOpKind::Or => "or",
            })]
        )?;
        write!(f, [end_of_line_comments(boolop)])?;
        write!(f, [trailing_comments(boolop)])?;

        Ok(())
    }
}
