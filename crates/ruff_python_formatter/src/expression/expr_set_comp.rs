use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprSetComp;

use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSetComp;

impl FormatNodeRule<ExprSetComp> for FormatExprSetComp {
    fn fmt_fields(&self, item: &ExprSetComp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprSetComp {
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
                "{",
                &group_with_flat_width_limit(
                    &inner_content,
                    f.options().set_comprehension_width_limit().into(),
                    true,
                ),
                "}"
            )
            .with_dangling_comments(dangling)]
        )
    }
}

impl NeedsParentheses for ExprSetComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
