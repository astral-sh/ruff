use crate::expression::parentheses::{parenthesized, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::format_args;
use ruff_python_ast::node::AnyNodeRef;
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
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
