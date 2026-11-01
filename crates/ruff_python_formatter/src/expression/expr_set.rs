use ruff_python_ast::{ExprSet, SyntaxNode};
use ruff_text_size::Ranged;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSet;

impl FormatNodeRule<ruff_python_ast::kind::ExprSet> for FormatExprSet {
    fn fmt_fields(&self, item: ExprSet<'_>, f: &mut PyFormatter) -> FormatResult<()> {
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

impl NeedsParentheses for ExprSet<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
