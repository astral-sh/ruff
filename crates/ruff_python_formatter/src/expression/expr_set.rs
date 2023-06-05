use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{
    format_with, group, if_group_breaks, soft_block_indent, soft_line_break_or_space, text,
};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use rustpython_parser::ast::ExprSet;

#[derive(Default)]
pub struct FormatExprSet;

impl FormatNodeRule<ExprSet> for FormatExprSet {
    fn fmt_fields(&self, item: &ExprSet, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprSet { range: _, elts } = item;
        // That would be a dict expression
        assert!(!elts.is_empty());
        let joined = format_with(|f| {
            let a = text(",");
            let b = soft_line_break_or_space();
            f.join_with(format_args!(a, b))
                .entries(elts.iter().map(AsFormat::format))
                .finish()
        });
        write!(
            f,
            [group(&format_args![
                text("{"),
                soft_block_indent(&format_args![joined, if_group_breaks(&text(",")),]),
                text("}")
            ])]
        )
    }
}

impl NeedsParentheses for ExprSet {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
