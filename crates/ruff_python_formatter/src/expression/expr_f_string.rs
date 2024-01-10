use ruff_python_ast::{AnyNodeRef, ExprFString};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::comments::SourceComment;
use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::other::f_string_part::FormatFStringPart;
use crate::prelude::*;
use crate::string::{AnyString, FormatStringContinuation, Quoting};

#[derive(Default)]
pub struct FormatExprFString;

impl FormatNodeRule<ExprFString> for FormatExprFString {
    fn fmt_fields(&self, item: &ExprFString, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprFString { value, .. } = item;

        match value.as_slice() {
            [f_string_part] => FormatFStringPart::new(
                f_string_part,
                f_string_quoting(item, &f.context().locator()),
            )
            .fmt(f),
            _ => {
                in_parentheses_only_group(&FormatStringContinuation::new(&AnyString::FString(item)))
                    .fmt(f)
            }
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_node_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
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
        } else if AnyString::FString(self).is_multiline(context.source()) {
            OptionalParentheses::Never
        } else {
            OptionalParentheses::BestFit
        }
    }
}

pub(crate) fn f_string_quoting(f_string: &ExprFString, locator: &Locator) -> Quoting {
    let unprefixed = locator
        .slice(f_string.range())
        .trim_start_matches(|c| c != '"' && c != '\'');
    let triple_quoted = unprefixed.starts_with(r#"""""#) || unprefixed.starts_with(r"'''");

    if f_string
        .value
        .elements()
        .filter_map(|element| element.as_expression())
        .any(|expression| {
            let string_content = locator.slice(expression.range());
            if triple_quoted {
                string_content.contains(r#"""""#) || string_content.contains("'''")
            } else {
                string_content.contains(['"', '\''])
            }
        })
    {
        Quoting::Preserve
    } else {
        Quoting::CanChange
    }
}
