use crate::prelude::*;
use ruff_formatter::write;

use crate::cst::{Operator, OperatorKind};
use crate::format::comments::{end_of_line_comments, leading_comments, trailing_comments};

pub(crate) struct FormatOperator<'a> {
    item: &'a Operator,
}

impl AsFormat<ASTFormatContext<'_>> for Operator {
    type Format<'a> = FormatOperator<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatOperator { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatOperator<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let operator = self.item;
        write!(f, [leading_comments(operator)])?;
        write!(
            f,
            [text(match operator.node {
                OperatorKind::Add => "+",
                OperatorKind::Sub => "-",
                OperatorKind::Mult => "*",
                OperatorKind::MatMult => "@",
                OperatorKind::Div => "/",
                OperatorKind::Mod => "%",
                OperatorKind::Pow => "**",
                OperatorKind::LShift => "<<",
                OperatorKind::RShift => ">>",
                OperatorKind::BitOr => "|",
                OperatorKind::BitXor => "^",
                OperatorKind::BitAnd => "&",
                OperatorKind::FloorDiv => "//",
            })]
        )?;
        write!(f, [end_of_line_comments(operator)])?;
        write!(f, [trailing_comments(operator)])?;
        Ok(())
    }
}
