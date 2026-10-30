use ruff_python_ast::ExprSet;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;
use ruff_text_size::Ranged;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSet;

impl FormatNodeRule<ExprSet> for FormatExprSet {
    fn fmt_fields(&self, item: flat::View<'_, ExprSet>, f: &mut PyFormatter) -> FormatResult<()> {
        let elts = item.elts();
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
}

impl NeedsParentheses for flat::View<'_, ExprSet> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
