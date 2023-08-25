use memchr::memchr2;

use ruff_formatter::FormatResult;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprFString;

use crate::expression::parentheses::{should_use_best_fit, NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

use super::string::{AnyString, FormatString};

#[derive(Default)]
pub struct FormatExprFString;

impl FormatNodeRule<ExprFString> for FormatExprFString {
    fn fmt_fields(&self, item: &ExprFString, f: &mut PyFormatter) -> FormatResult<()> {
        FormatString::new(&AnyString::FString(item)).fmt(f)
    }
}

impl NeedsParentheses for ExprFString {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.implicit_concatenated {
            OptionalParentheses::Multiline
        } else if memchr2(b'\n', b'\r', context.source()[self.range].as_bytes()).is_none()
            && should_use_best_fit(self, context)
        {
            OptionalParentheses::BestFit
        } else {
            OptionalParentheses::Never
        }
    }
}
