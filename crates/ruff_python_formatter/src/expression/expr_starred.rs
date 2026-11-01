use ruff_formatter::write;
use ruff_python_ast::{ExprStarred, SyntaxNode};

use crate::comments::dangling_comments;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprStarred;

impl FormatNodeRule<ruff_python_ast::kind::ExprStarred> for FormatExprStarred {
    fn fmt_fields(&self, item: ExprStarred<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let value = item.value();

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [token("*"), dangling_comments(dangling), value.format()])
    }
}

impl NeedsParentheses for ExprStarred<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
