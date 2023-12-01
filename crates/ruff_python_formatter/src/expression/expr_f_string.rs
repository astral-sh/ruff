use memchr::memchr2;

use crate::comments::SourceComment;
use ruff_formatter::FormatResult;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprFString;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

use super::string::{AnyString, FormatString};

#[derive(Default)]
pub struct FormatExprFString;

impl FormatNodeRule<ExprFString> for FormatExprFString {
    fn fmt_fields(&self, item: &ExprFString, f: &mut PyFormatter) -> FormatResult<()> {
        FormatString::new(&AnyString::FString(item)).fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_node_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprFString {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.value.is_implicit_concatenated() {
            OptionalParentheses::Multiline
        } else if memchr2(b'\n', b'\r', context.source()[self.range].as_bytes()).is_none() {
            OptionalParentheses::BestFit
        } else {
            OptionalParentheses::Never
        }
    }
}
