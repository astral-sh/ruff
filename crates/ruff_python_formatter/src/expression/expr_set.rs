use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprSet;
use ruff_text_size::Ranged;

use crate::comments::SourceComment;
use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSet;

impl FormatNodeRule<ExprSet> for FormatExprSet {
    fn fmt_fields(&self, item: &ExprSet, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprSet { range: _, elts } = item;
        // That would be a dict expression
        assert!(!elts.is_empty());
        // Avoid second mutable borrow of f
        let joined = format_with(|f: &mut PyFormatter| {
            f.join_comma_separated(item.end())
                .nodes(elts.iter())
                .finish()
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        parenthesized("{", &joined, "}")
            .with_dangling_comments(dangling)
            .fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprSet {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
