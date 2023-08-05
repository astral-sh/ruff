use super::string::{AnyString, FormatString};
use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::FormatResult;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprJoinedStr;

#[derive(Default)]
pub struct FormatExprJoinedStr;

impl FormatNodeRule<ExprJoinedStr> for FormatExprJoinedStr {
    fn fmt_fields(&self, item: &ExprJoinedStr, f: &mut PyFormatter) -> FormatResult<()> {
        FormatString::new(&AnyString::JoinedStr(item)).fmt(f)
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
