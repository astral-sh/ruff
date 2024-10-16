use ruff_formatter::FormatRuleWithOptions;
use ruff_formatter::GroupId;
use ruff_python_ast::ExprBytesLiteral;
use ruff_python_ast::{AnyNodeRef, StringLike};

use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;
use crate::string::{FormatImplicitConcatenatedString, StringLikeExtensions};

#[derive(Default)]
pub struct FormatExprBytesLiteral {
    layout: ExprBytesLiteralLayout,
}

#[derive(Default)]
pub struct ExprBytesLiteralLayout {
    /// ID of the group wrapping the implicit concatenated string. If `None`, the implicit
    /// is wrapped in an [`in_parentheses_only_group`].
    ///
    /// This is used when formatting implicit concatenated strings in assignment value positions
    /// where the positioning of comments depends on whether the string can be joined or not.
    pub implicit_group_id: Option<GroupId>,
}

impl FormatRuleWithOptions<ExprBytesLiteral, PyFormatContext<'_>> for FormatExprBytesLiteral {
    type Options = ExprBytesLiteralLayout;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
        self
    }
}

impl FormatNodeRule<ExprBytesLiteral> for FormatExprBytesLiteral {
    fn fmt_fields(&self, item: &ExprBytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprBytesLiteral { value, .. } = item;

        match value.as_slice() {
            [bytes_literal] => bytes_literal.format().fmt(f),
            _ => match self.layout.implicit_group_id {
                Some(group_id) => group(&FormatImplicitConcatenatedString::new(item))
                    .with_group_id(Some(group_id))
                    .fmt(f),
                None => {
                    in_parentheses_only_group(&FormatImplicitConcatenatedString::new(item)).fmt(f)
                }
            },
        }
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
        } else if StringLike::Bytes(self).is_multiline(context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
