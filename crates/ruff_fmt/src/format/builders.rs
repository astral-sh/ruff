use rome_formatter::prelude::*;
use rome_formatter::{write, Format};
use rome_rowan::TextSize;

use crate::context::ASTFormatContext;
use crate::cst::Stmt;
use crate::shared_traits::AsFormat;

#[derive(Copy, Clone)]
pub struct Block<'a> {
    body: &'a [Stmt],
}

impl Format<ASTFormatContext<'_>> for Block<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        for (i, stmt) in self.body.iter().enumerate() {
            if i > 0 {
                write!(f, [hard_line_break()])?;
            }
            write!(f, [stmt.format()])?;
        }
        Ok(())
    }
}

#[inline]
pub fn block(body: &[Stmt]) -> Block {
    Block { body }
}

pub fn join_names<Context>(f: &mut Formatter<Context>, names: &[String]) -> FormatResult<()> {
    let mut join = f.join_with(text(", "));
    for name in names {
        join.entry(&dynamic_text(name, TextSize::default()));
    }
    join.finish()
}
