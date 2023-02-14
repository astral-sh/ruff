use rome_formatter::prelude::*;
use rome_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::Operator;
use crate::shared_traits::AsFormat;

pub struct FormatOperator<'a> {
    item: &'a Operator,
}

impl AsFormat<ASTFormatContext<'_>> for Operator {
    type Format<'a> = FormatOperator<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatOperator { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatOperator<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let operator = self.item;

        write!(
            f,
            [text(match operator {
                Operator::Add => "+",
                Operator::Sub => "-",
                Operator::Mult => "*",
                Operator::MatMult => "@",
                Operator::Div => "/",
                Operator::Mod => "%",
                Operator::Pow => "**",
                Operator::LShift => "<<",
                Operator::RShift => ">>",
                Operator::BitOr => "|",
                Operator::BitXor => "^",
                Operator::BitAnd => "&",
                Operator::FloorDiv => "//",
            })]
        )?;

        Ok(())
    }
}
