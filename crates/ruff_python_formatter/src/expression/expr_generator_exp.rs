use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprGeneratorExp;

#[derive(Default)]
pub struct FormatExprGeneratorExp;

impl FormatNodeRule<ExprGeneratorExp> for FormatExprGeneratorExp {
    fn fmt_fields(&self, _item: &ExprGeneratorExp, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "(NOT_YET_IMPLEMENTED_generator_key for NOT_YET_IMPLEMENTED_generator_key in [])"
            )]
        )
    }
}

impl NeedsParentheses for ExprGeneratorExp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
