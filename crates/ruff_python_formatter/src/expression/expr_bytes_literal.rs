use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprBytesLiteral;

use crate::comments::SourceComment;
use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;
use crate::string::{AnyString, FormatStringContinuation};

#[derive(Default)]
pub struct FormatExprBytesLiteral;

impl FormatNodeRule<ExprBytesLiteral> for FormatExprBytesLiteral {
    fn fmt_fields(&self, item: &ExprBytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprBytesLiteral { value, .. } = item;

        match value.as_slice() {
            [bytes_literal] => bytes_literal.format().fmt(f),
            _ => in_parentheses_only_group(&FormatStringContinuation::new(&AnyString::Bytes(item)))
                .fmt(f),
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprBytesLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.value.is_implicit_concatenated() {
            OptionalParentheses::Multiline
        } else if AnyString::Bytes(self).is_multiline(context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
