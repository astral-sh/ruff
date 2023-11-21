use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprStringLiteral;
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::comments::SourceComment;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::expression::string::{
    AnyString, FormatString, StringLayout, StringPrefix, StringQuotes,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprStringLiteral {
    layout: StringLayout,
}

impl FormatRuleWithOptions<ExprStringLiteral, PyFormatContext<'_>> for FormatExprStringLiteral {
    type Options = StringLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
    }
}

impl FormatNodeRule<ExprStringLiteral> for FormatExprStringLiteral {
    fn fmt_fields(&self, item: &ExprStringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        FormatString::new(&AnyString::String(item))
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

impl NeedsParentheses for ExprStringLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.value.is_implicit_concatenated() {
            OptionalParentheses::Multiline
        } else if is_multiline_string(self.into(), context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}

pub(super) fn is_multiline_string(expr: AnyNodeRef, source: &str) -> bool {
    if expr.is_expr_string_literal() || expr.is_expr_bytes_literal() {
        let contents = &source[expr.range()];
        let prefix = StringPrefix::parse(contents);
        let quotes =
            StringQuotes::parse(&contents[TextRange::new(prefix.text_len(), contents.text_len())]);

        quotes.is_some_and(StringQuotes::is_triple)
            && memchr::memchr2(b'\n', b'\r', contents.as_bytes()).is_some()
    } else {
        false
    }
}
