use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::{AnyNodeRef, ExprStringLiteral, StringLike};

use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::other::string_literal::StringLiteralKind;
use crate::prelude::*;
use crate::string::implicit::FormatImplicitConcatenatedStringFlat;
use crate::string::{implicit::FormatImplicitConcatenatedString, StringLikeExtensions};

#[derive(Default)]
pub struct FormatExprStringLiteral {
    kind: StringLiteralKind,
}

impl FormatRuleWithOptions<ExprStringLiteral, PyFormatContext<'_>> for FormatExprStringLiteral {
    type Options = StringLiteralKind;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.kind = options;
        self
    }
}

impl FormatNodeRule<ExprStringLiteral> for FormatExprStringLiteral {
    fn fmt_fields(&self, item: &ExprStringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprStringLiteral { value, .. } = item;

        match value.as_slice() {
            [string_literal] => string_literal.format().with_options(self.kind).fmt(f),
            _ => {
                // This is just a sanity check because [`DocstringStmt::try_from_statement`]
                // ensures that the docstring is a *single* string literal.
                assert!(!self.kind.is_docstring());

                // Always join strings that aren't parenthesized and thus, always on a single line.
                if !f.context().node_level().is_parenthesized() {
                    if let Some(format_flat) =
                        FormatImplicitConcatenatedStringFlat::new(item.into(), f.context())
                    {
                        return format_flat.fmt(f);
                    }
                }

                in_parentheses_only_group(&FormatImplicitConcatenatedString::new(item))
            }
            .fmt(f),
        }
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
        } else if StringLike::String(self).is_multiline(context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
