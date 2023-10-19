use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprBytesLiteral;

use crate::comments::SourceComment;
use crate::expression::expr_string_literal::is_multiline_string;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::expression::string::{AnyString, FormatString, StringLayout};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprBytesLiteral {
    layout: StringLayout,
}

impl FormatRuleWithOptions<ExprBytesLiteral, PyFormatContext<'_>> for FormatExprBytesLiteral {
    type Options = StringLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
    }
}

impl FormatNodeRule<ExprBytesLiteral> for FormatExprBytesLiteral {
    fn fmt_fields(&self, item: &ExprBytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatString::new(&AnyString::Bytes(item))
            .with_layout(self.layout)
            .fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

impl NeedsParentheses for ExprBytesLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.implicit_concatenated {
            OptionalParentheses::Multiline
        } else if is_multiline_string(self.into(), context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
