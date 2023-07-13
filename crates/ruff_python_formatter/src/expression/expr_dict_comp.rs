use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprDictComp;

#[derive(Default)]
pub struct FormatExprDictComp;

impl FormatNodeRule<ExprDictComp> for FormatExprDictComp {
    fn fmt_fields(&self, _item: &ExprDictComp, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                "{NOT_IMPLEMENTED_dict_key: NOT_IMPLEMENTED_dict_value for key, value in NOT_IMPLEMENTED_dict}"
            )]
        )
    }
}

impl NeedsParentheses for ExprDictComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
