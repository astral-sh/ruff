use ruff_formatter::prelude::{format_with, text};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{ExprList, Ranged};

use crate::builders::empty_parenthesized_with_dangling_comments;
use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatExprList;

impl FormatNodeRule<ExprList> for FormatExprList {
    fn fmt_fields(&self, item: &ExprList, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprList {
            range: _,
            elts,
            ctx: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling_comments(item);

        if elts.is_empty() {
            return empty_parenthesized_with_dangling_comments(text("["), dangling, text("]"))
                .fmt(f);
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

    fn fmt_dangling_comments(&self, _node: &ExprList, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprList {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
