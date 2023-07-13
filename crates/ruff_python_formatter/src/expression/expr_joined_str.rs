use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::{not_yet_implemented_custom_text, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprJoinedStr;

#[derive(Default)]
pub struct FormatExprJoinedStr;

impl FormatNodeRule<ExprJoinedStr> for FormatExprJoinedStr {
    fn fmt_fields(&self, _item: &ExprJoinedStr, f: &mut PyFormatter) -> FormatResult<()> {
        write!(
            f,
            [not_yet_implemented_custom_text(
                r#"f"NOT_YET_IMPLEMENTED_ExprJoinedStr""#
            )]
        )
    }
}

impl NeedsParentheses for ExprJoinedStr {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
