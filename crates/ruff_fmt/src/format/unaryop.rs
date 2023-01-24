use rome_formatter::prelude::*;
use rome_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Unaryop;
use crate::shared_traits::AsFormat;

pub struct FormatUnaryop<'a> {
    item: &'a Unaryop,
}

impl AsFormat<ASTFormatContext<'_>> for Unaryop {
    type Format<'a> = FormatUnaryop<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatUnaryop { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatUnaryop<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let unaryop = self.item;
        write!(
            f,
            [text(match unaryop {
                Unaryop::Invert => "~",
                Unaryop::Not => "not",
                Unaryop::UAdd => "+",
                Unaryop::USub => "-",
            })]
        )?;
        if matches!(unaryop, Unaryop::Not) {
            write!(f, [space()])?;
        }
        Ok(())
    }
}
