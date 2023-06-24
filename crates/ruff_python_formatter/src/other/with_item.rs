use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::WithItem;

#[derive(Default)]
pub struct FormatWithItem;

impl FormatNodeRule<WithItem> for FormatWithItem {
    fn fmt_fields(&self, item: &WithItem, f: &mut PyFormatter) -> FormatResult<()> {
        let WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = item;
        // parentheses added by parent StmtWith
        write!(f, [context_expr.format().with_options(Parenthesize::Never)])?;
        if let Some(optional_vars) = optional_vars {
            write!(f, [space(), text("as"), space(), optional_vars.format()])?;
        }
        Ok(())
    }
}
