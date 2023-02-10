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

pub(crate) const fn join_names( names: &[String]) -> JoinNames {
    JoinNames { names }
}

pub(crate) struct JoinNames<'a> {
    names: &'a [String],
}

impl<Context> Format<Context> for JoinNames<'_> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let mut join = f.join_with(text(", "));
        for name in self.names {
            join.entry(&dynamic_text(name, TextSize::default()));
        }
        join.finish()
    }
}
