use ruff_python_ast::{ExprDict, SyntaxNode};
use ruff_text_size::Ranged;

use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, empty_parenthesized, parenthesized,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprDict;

impl FormatNodeRule<ruff_python_ast::kind::ExprDict> for FormatExprDict {
    fn fmt_fields(&self, item: ExprDict<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let items = item.items();

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if items.is_empty() {
            return empty_parenthesized("{", dangling, "}").fmt(f);
        }

        let pairs = format_with(|f: &mut PyFormatter| {
            f.join_comma_separated(item.end())
                .nodes(items.iter())
                .finish()
        });
        parenthesized("{", &pairs, "}")
            .with_dangling_comments(dangling)
            .fmt(f)
    }
}

impl NeedsParentheses for ExprDict<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
