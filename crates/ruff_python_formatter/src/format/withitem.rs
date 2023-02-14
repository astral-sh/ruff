use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Withitem;
use crate::shared_traits::AsFormat;

pub struct FormatWithitem<'a> {
    item: &'a Withitem,
}

impl AsFormat<ASTFormatContext<'_>> for Withitem {
    type Format<'a> = FormatWithitem<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatWithitem { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatWithitem<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let withitem = self.item;

        write!(f, [withitem.context_expr.format()])?;
        if let Some(optional_vars) = &withitem.optional_vars {
            write!(f, [space(), text("as"), space()])?;
            write!(f, [optional_vars.format()])?;
        }

        Ok(())
    }
}
