use ruff_python_ast::{ExprTString, flat, flat::AnyNodeRef, flat::StringLike};

use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, in_parentheses_only_group,
};
use crate::other::interpolated_string::InterpolatedStringLayout;
use crate::prelude::*;
use crate::string::StringLikeExtensions;
use crate::string::implicit::{
    FormatImplicitConcatenatedString, FormatImplicitConcatenatedStringFlat,
};

#[derive(Default)]
pub struct FormatExprTString;

impl FormatNodeRule<ExprTString> for FormatExprTString {
    fn fmt_fields(
        &self,
        item: flat::View<'_, ExprTString>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        if let Some(t_string) = item.as_single_part_tstring() {
            t_string.format().fmt(f)
        } else {
            // Always join tstrings that aren't parenthesized and thus, are always on a single line.
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

impl NeedsParentheses for flat::View<'_, ExprTString> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if let Some(tstring_part) = self.as_single_part_tstring() {
            // The t-string is not implicitly concatenated
            if StringLike::from(*self).is_multiline(context)
                || InterpolatedStringLayout::from_interpolated_string_elements(
                    tstring_part.elements(),
                    context.source(),
                )
                .is_multiline()
            {
                OptionalParentheses::Never
            } else {
                OptionalParentheses::BestFit
            }
        } else {
            // The t-string is implicitly concatenated
            OptionalParentheses::Multiline
        }
    }
}
