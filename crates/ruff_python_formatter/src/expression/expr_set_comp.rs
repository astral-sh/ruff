use ruff_formatter::{Buffer, FormatResult, format_args, write};
use ruff_python_ast::ExprSetComp;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSetComp;

impl FormatNodeRule<ExprSetComp> for FormatExprSetComp {
    fn fmt_fields(
        &self,
        item: flat::View<'_, ExprSetComp>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let elt = item.elt();
        let generators = item.generators();

        let joined = format_with(|f| {
            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(
            f,
            [parenthesized(
                "{",
                &group(&format_args!(
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    joined
                )),
                "}"
            )
            .with_dangling_comments(dangling)]
        )
    }
}

impl NeedsParentheses for flat::View<'_, ExprSetComp> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
