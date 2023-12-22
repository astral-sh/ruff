use ruff_formatter::write;
use ruff_python_ast::WithItem;

use crate::comments::SourceComment;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{
    is_expression_parenthesized, parenthesized, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::preview::is_wrap_multiple_context_managers_in_parens_enabled;

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
        let trailing_as_comments = comments.dangling(item);

        let is_parenthesized = is_expression_parenthesized(
            context_expr.into(),
            f.context().comments().ranges(),
            f.context().source(),
        );

        // Remove the parentheses of the `with_items` if the with statement adds parentheses
        if f.context().node_level().is_parenthesized()
            && is_wrap_multiple_context_managers_in_parens_enabled(f.context())
        {
            if is_parenthesized {
                // ...except if the with item is parenthesized, then use this with item as a preferred breaking point
                // or when it has comments, then parenthesize it to prevent comments from moving.
                maybe_parenthesize_expression(
                    context_expr,
                    item,
                    Parenthesize::IfBreaksOrIfRequired,
                )
                .fmt(f)?;
            } else {
                context_expr
                    .format()
                    .with_options(Parentheses::Never)
                    .fmt(f)?;
            }
        } else {
            // Prefer keeping parentheses for already parenthesized expressions over
            // parenthesizing other nodes.
            let parenthesize = if is_parenthesized {
                Parenthesize::IfBreaks
            } else {
                Parenthesize::IfRequired
            };

            write!(
                f,
                [maybe_parenthesize_expression(
                    context_expr,
                    item,
                    parenthesize
                )]
            )?;
        }

        if let Some(optional_vars) = optional_vars {
            write!(f, [space(), token("as"), space()])?;

            if trailing_as_comments.is_empty() {
                write!(f, [optional_vars.format()])?;
            } else {
                write!(
                    f,
                    [parenthesized(
                        "(",
                        &optional_vars.format().with_options(Parentheses::Never),
                        ")",
                    )
                    .with_dangling_comments(trailing_as_comments)]
                )?;
            }
        }

        Ok(())
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}
