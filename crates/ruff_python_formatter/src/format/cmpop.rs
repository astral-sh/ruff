use rome_formatter::prelude::*;
use rome_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Cmpop;
use crate::shared_traits::AsFormat;

pub struct FormatCmpop<'a> {
    item: &'a Cmpop,
}

impl AsFormat<ASTFormatContext<'_>> for Cmpop {
    type Format<'a> = FormatCmpop<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatCmpop { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatCmpop<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let unaryop = self.item;
        write!(
            f,
            [text(match unaryop {
                Cmpop::Eq => "==",
                Cmpop::NotEq => "!=",
                Cmpop::Lt => "<",
                Cmpop::LtE => "<=",
                Cmpop::Gt => ">",
                Cmpop::GtE => ">=",
                Cmpop::Is => "is",
                Cmpop::IsNot => "is not",
                Cmpop::In => "in",
                Cmpop::NotIn => "not in",
            })]
        )?;
        Ok(())
    }
}
