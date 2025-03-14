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
        if let Some(f_string) = item.as_single_part_fstring() {
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
        if let Some(fstring_part) = self.as_single_part_fstring() {
            // The f-string is not implicitly concatenated
            if StringLike::FString(self).is_multiline(context)
                || FStringLayout::from_f_string(fstring_part, context.source()).is_multiline()
            {
                OptionalParentheses::Never
            } else {
                OptionalParentheses::BestFit
            }
        } else {
            // The f-string is implicitly concatenated
            OptionalParentheses::Multiline
        }
    }
}
