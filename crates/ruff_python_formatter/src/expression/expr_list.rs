use ruff_formatter::prelude::format_with;
use ruff_python_ast::{ExprList, SyntaxNode};
use ruff_text_size::Ranged;

use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, empty_parenthesized, parenthesized,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprList;

impl FormatNodeRule<ruff_python_ast::kind::ExprList> for FormatExprList {
    fn fmt_fields(&self, item: ExprList<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let elts = item.elts();

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if elts.is_empty() {
            return empty_parenthesized("[", dangling, "]").fmt(f);
        }

        let items = format_with(|f| {
            f.join_comma_separated(item.end())
                .nodes(elts.iter())
                .finish()
        });

        parenthesized("[", &items, "]")
            .with_dangling_comments(dangling)
            .fmt(f)
    }
}

impl NeedsParentheses for ExprList<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
