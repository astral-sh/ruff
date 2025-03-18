use ruff_formatter::{write, FormatResult};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprListComp;

use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprListComp;

impl FormatNodeRule<ExprListComp> for FormatExprListComp {
    fn fmt_fields(&self, item: &ExprListComp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprListComp {
            range: _,
            elt,
            generators,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let inner_content = format_with(|f| {
            write!(f, [group(&elt.format()), soft_line_break_or_space()])?;

            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        write!(
            f,
            [parenthesized(
                "[",
                &group_with_flat_width_limit(
                    &inner_content,
                    f.options().list_comprehension_width_limit().into(),
                    true,
                ),
                "]"
            )
            .with_dangling_comments(dangling)]
        )
    }
}

impl NeedsParentheses for ExprListComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
