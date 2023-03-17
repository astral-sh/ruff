use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::{CmpOp, CmpOpKind};
use crate::format::comments::{end_of_line_comments, leading_comments, trailing_comments};
use crate::shared_traits::AsFormat;

pub struct FormatCmpOp<'a> {
    item: &'a CmpOp,
}

impl AsFormat<ASTFormatContext<'_>> for CmpOp {
    type Format<'a> = FormatCmpOp<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatCmpOp { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatCmpOp<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let cmp_op = self.item;
        write!(f, [leading_comments(cmp_op)])?;
        write!(
            f,
            [text(match cmp_op.node {
                CmpOpKind::Eq => "==",
                CmpOpKind::NotEq => "!=",
                CmpOpKind::Lt => "<",
                CmpOpKind::LtE => "<=",
                CmpOpKind::Gt => ">",
                CmpOpKind::GtE => ">=",
                CmpOpKind::Is => "is",
                CmpOpKind::IsNot => "is not",
                CmpOpKind::In => "in",
                CmpOpKind::NotIn => "not in",
            })]
        )?;
        write!(f, [end_of_line_comments(cmp_op)])?;
        write!(f, [trailing_comments(cmp_op)])?;
        Ok(())
    }
}
