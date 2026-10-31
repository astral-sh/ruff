use ruff_formatter::write;
use ruff_python_ast::ExprStarred;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::comments::dangling_comments;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprStarred;

impl FormatNodeRule<ExprStarred> for FormatExprStarred {
    fn fmt_fields(
        &self,
        item: flat::View<'_, ExprStarred>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let value = item.value();

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [token("*"), dangling_comments(dangling), value.format()])
    }
}

impl NeedsParentheses for flat::View<'_, ExprStarred> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
