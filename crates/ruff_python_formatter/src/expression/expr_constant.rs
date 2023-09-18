use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Constant, ExprConstant};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::comments::SourceComment;
use crate::expression::number::{FormatComplex, FormatFloat, FormatInt};
use crate::expression::parentheses::{should_use_best_fit, NeedsParentheses, OptionalParentheses};
use crate::expression::string::{
    AnyString, FormatString, StringLayout, StringPrefix, StringQuotes,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprConstant {
    layout: ExprConstantLayout,
}

#[derive(Copy, Clone, Debug, Default)]
pub enum ExprConstantLayout {
    #[default]
    Default,

    String(StringLayout),
}

impl FormatRuleWithOptions<ExprConstant, PyFormatContext<'_>> for FormatExprConstant {
    type Options = ExprConstantLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
    }
}

impl FormatNodeRule<ExprConstant> for FormatExprConstant {
    fn fmt_fields(&self, item: &ExprConstant, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprConstant { range: _, value } = item;

        match value {
            Constant::Ellipsis => token("...").fmt(f),
            Constant::None => token("None").fmt(f),
            Constant::Bool(value) => match value {
                true => token("True").fmt(f),
                false => token("False").fmt(f),
            },
            Constant::Int(_) => FormatInt::new(item).fmt(f),
            Constant::Float(_) => FormatFloat::new(item).fmt(f),
            Constant::Complex { .. } => FormatComplex::new(item).fmt(f),
            Constant::Str(_) | Constant::Bytes(_) => {
                let string_layout = match self.layout {
                    ExprConstantLayout::Default => StringLayout::Default,
                    ExprConstantLayout::String(layout) => layout,
                };
                FormatString::new(&AnyString::Constant(item))
                    .with_layout(string_layout)
                    .fmt(f)
            }
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

impl NeedsParentheses for ExprConstant {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.value.is_implicit_concatenated() {
            OptionalParentheses::Multiline
        } else if is_multiline_string(self, context.source())
            || self.value.is_none()
            || self.value.is_bool()
            || self.value.is_ellipsis()
        {
            OptionalParentheses::Never
        } else if should_use_best_fit(self, context) {
            OptionalParentheses::BestFit
        } else {
            OptionalParentheses::Never
        }
    }
}

pub(super) fn is_multiline_string(constant: &ExprConstant, source: &str) -> bool {
    if constant.value.is_str() || constant.value.is_bytes() {
        let contents = &source[constant.range()];
        let prefix = StringPrefix::parse(contents);
        let quotes =
            StringQuotes::parse(&contents[TextRange::new(prefix.text_len(), contents.text_len())]);

        quotes.is_some_and(StringQuotes::is_triple)
            && memchr::memchr2(b'\n', b'\r', contents.as_bytes()).is_some()
    } else {
        false
    }
}
