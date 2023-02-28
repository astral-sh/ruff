use ruff_formatter::prelude::*;
use ruff_formatter::write;

use crate::context::ASTFormatContext;
use crate::cst::{UnaryOp, UnaryOpKind};
use crate::shared_traits::AsFormat;

pub struct FormatUnaryOp<'a> {
    item: &'a UnaryOp,
}

impl AsFormat<ASTFormatContext<'_>> for UnaryOp {
    type Format<'a> = FormatUnaryOp<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatUnaryOp { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatUnaryOp<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let unary_op = self.item;
        write!(
            f,
            [
                text(match unary_op.node {
                    UnaryOpKind::Invert => "~",
                    UnaryOpKind::Not => "not",
                    UnaryOpKind::UAdd => "+",
                    UnaryOpKind::USub => "-",
                }),
                matches!(unary_op.node, UnaryOpKind::Not).then_some(space())
            ]
        )?;
        Ok(())
    }
}
