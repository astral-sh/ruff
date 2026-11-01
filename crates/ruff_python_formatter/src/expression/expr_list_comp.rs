use ruff_formatter::{FormatResult, format_args, write};
use ruff_python_ast::{ExprListComp, SyntaxNode};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprListComp;

impl FormatNodeRule<ruff_python_ast::kind::ExprListComp> for FormatExprListComp {
    fn fmt_fields(&self, item: ExprListComp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
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
                "[",
                &group(&format_args![
                    group(&elt.format()),
                    soft_line_break_or_space(),
                    joined
                ]),
                "]"
            )
            .with_dangling_comments(dangling)]
        )
    }
}

impl NeedsParentheses for ExprListComp<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
