use ruff_python_ast::ExprBytesLiteral;
use ruff_python_ast::{AnyNodeRef, StringLike};

use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;
use crate::string::implicit::FormatImplicitConcatenatedStringFlat;
use crate::string::{implicit::FormatImplicitConcatenatedString, StringLikeExtensions};

#[derive(Default)]
pub struct FormatExprBytesLiteral;

impl FormatNodeRule<ExprBytesLiteral> for FormatExprBytesLiteral {
    fn fmt_fields(&self, item: &ExprBytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some(bytes_literal) = item.as_single_part_bytestring() {
            bytes_literal.format().fmt(f)
        } else {
            // Always join byte literals that aren't parenthesized and thus, always on a single line.
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

impl NeedsParentheses for ExprBytesLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if self.value.is_implicit_concatenated() {
            OptionalParentheses::Multiline
        } else if StringLike::Bytes(self).is_multiline(context) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}
