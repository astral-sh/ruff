use crate::comments::Comments;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, parenthesized, NeedsParentheses, Parentheses,
    Parenthesize,
};
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::format_args;
use rustpython_parser::ast::ExprSet;

#[derive(Default)]
pub struct FormatExprSet;

impl FormatNodeRule<ExprSet> for FormatExprSet {
    fn fmt_fields(&self, item: &ExprSet, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprSet { range: _, elts } = item;
        // That would be a dict expression
        assert!(!elts.is_empty());
        // Avoid second mutable borrow of f
        let joined = format_with(|f| {
            f.join_with(format_args!(text(","), soft_line_break_or_space()))
                .entries(elts.iter().formatted())
                .finish()
        });

        parenthesized("{", &format_args![joined, if_group_breaks(&text(","))], "}").fmt(f)
    }
}

impl NeedsParentheses for ExprSet {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
