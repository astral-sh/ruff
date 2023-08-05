use ruff_python_ast::WithItem;

use ruff_formatter::{write, Buffer, FormatResult};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatWithItem;

impl FormatNodeRule<WithItem> for FormatWithItem {
    fn fmt_fields(&self, item: &WithItem, f: &mut PyFormatter) -> FormatResult<()> {
        let WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = item;

        let comments = f.context().comments().clone();
        let trailing_as_comments = comments.dangling_comments(item);

        maybe_parenthesize_expression(context_expr, item, Parenthesize::IfRequired).fmt(f)?;

        if let Some(optional_vars) = optional_vars {
            write!(
                f,
                [space(), text("as"), trailing_comments(trailing_as_comments)]
            )?;
            let leading_var_comments = comments.leading_comments(optional_vars.as_ref());
            if leading_var_comments.is_empty() {
                write!(f, [space(), optional_vars.format()])?;
            } else {
                write!(
                    f,
                    [
                        // Otherwise the comment would end up on the same line as the `as`
                        hard_line_break(),
                        leading_comments(leading_var_comments),
                        optional_vars.format()
                    ]
                )?;
            }
        }
        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &WithItem, _f: &mut PyFormatter) -> FormatResult<()> {
        Ok(())
    }
}
