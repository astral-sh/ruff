use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{not_yet_implemented_custom_text, AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::text;
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprAttribute;

#[derive(Default)]
pub struct FormatExprAttribute;

impl FormatNodeRule<ExprAttribute> for FormatExprAttribute {
    fn fmt_fields(&self, item: &ExprAttribute, f: &mut PyFormatter) -> FormatResult<()> {
        // We need to write the value - which is also a dummy - already because power op spacing
        // depends on it
        write!(
            f,
            [
                item.value.format(),
                text("."),
                not_yet_implemented_custom_text(item, "NOT_IMPLEMENTED_attr")
            ]
        )
    }
}

impl NeedsParentheses for ExprAttribute {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        default_expression_needs_parentheses(self.into(), parenthesize, source)
    }
}
