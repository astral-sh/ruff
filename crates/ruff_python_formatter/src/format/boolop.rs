use rome_formatter::prelude::*;
use rome_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Boolop;
use crate::shared_traits::AsFormat;

pub struct FormatBoolop<'a> {
    item: &'a Boolop,
}

impl AsFormat<ASTFormatContext<'_>> for Boolop {
    type Format<'a> = FormatBoolop<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatBoolop { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatBoolop<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let boolop = self.item;
        write!(
            f,
            [text(match boolop {
                Boolop::And => "and",
                Boolop::Or => "or",
            })]
        )?;
        Ok(())
    }
}
