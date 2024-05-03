use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprStarred;

use crate::comments::dangling_comments;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprStarred;

impl FormatNodeRule<ExprStarred> for FormatExprStarred {
    fn fmt_fields(&self, item: &ExprStarred, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprStarred {
            range: _,
            value,
            ctx: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [token("*"), dangling_comments(dangling), value.format()])
    }
}

impl NeedsParentheses for ExprStarred {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
