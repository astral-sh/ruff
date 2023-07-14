use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprLambda;

#[derive(Default)]
pub struct FormatExprLambda;

impl FormatNodeRule<ExprLambda> for FormatExprLambda {
    fn fmt_fields(&self, _item: &ExprLambda, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "lambda NOT_YET_IMPLEMENTED_lambda: True"
            )]
        )
    }
}

impl NeedsParentheses for ExprLambda {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
