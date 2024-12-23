use ruff_python_ast::{AnyNodeRef, ExprFString, StringLike};

use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::other::f_string::FStringLayout;
use crate::prelude::*;
use crate::string::implicit::{
    FormatImplicitConcatenatedString, FormatImplicitConcatenatedStringFlat,
};
use crate::string::StringLikeExtensions;

#[derive(Default)]
pub struct FormatExprFString;

impl FormatNodeRule<ExprFString> for FormatExprFString {
    fn fmt_fields(&self, item: &ExprFString, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprFString { value, .. } = item;

        if let [f_string_part] = value.as_slice() {
            // SAFETY: A single string literal cannot be an f-string. This is guaranteed by the
            // [`ruff_python_ast::FStringValue::single`] constructor.
            let f_string = f_string_part.as_f_string().unwrap();

            f_string.format().fmt(f)
        } else {
            // Always join fstrings that aren't parenthesized and thus, are always on a single line.
            if !f.context().node_level().is_parenthesized() {
                if let Some(format_flat) =
                    FormatImplicitConcatenatedStringFlat::new(item.into(), f.context())
                {
                    return format_flat.fmt(f);
                }
            }

            in_parentheses_only_group(&FormatImplicitConcatenatedString::new(item)).fmt(f)
        }
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
        } else if StringLike::FString(self).is_multiline(context)
            || self.value.as_single().is_some_and(|f_string| {
                FStringLayout::from_f_string(f_string, context.source()).is_multiline()
            })
        {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
