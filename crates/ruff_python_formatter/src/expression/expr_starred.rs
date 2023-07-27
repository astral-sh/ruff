use ruff_python_ast::ExprStarred;

use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;

use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatExprStarred;

impl FormatNodeRule<ExprStarred> for FormatExprStarred {
    fn fmt_fields(&self, item: &ExprStarred, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprStarred {
            range: _,
            value,
            ctx: _,
        } = item;

        write!(f, [text("*"), value.format()])
    }

    fn fmt_dangling_comments(&self, node: &ExprStarred, f: &mut PyFormatter) -> FormatResult<()> {
        debug_assert_eq!(f.context().comments().dangling_comments(node), []);

        Ok(())
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
