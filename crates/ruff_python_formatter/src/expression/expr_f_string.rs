use ruff_python_ast::{AnyNodeRef, ExprFString};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::other::f_string_part::FormatFStringPart;
use crate::prelude::*;
use crate::string::{AnyString, FormatImplicitConcatenatedString, Quoting};

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
            _ => in_parentheses_only_group(&FormatImplicitConcatenatedString::new(item)).fmt(f),
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
        // TODO(dhruvmanila): Ideally what we want here is a new variant which
        // is something like:
        // - If the expression fits by just adding the parentheses, then add them and
        //   avoid breaking the f-string expression. So,
        //   ```
        //   xxxxxxxxx = (
        //       f"aaaaaaaaaaaa { xxxxxxx + yyyyyyyy } bbbbbbbbbbbbb"
        //   )
        //   ```
        // - But, if the expression is too long to fit even with parentheses, then
        //   don't add the parentheses and instead break the expression at `soft_line_break`.
        //   ```
        //   xxxxxxxxx = f"aaaaaaaaaaaa {
        //       xxxxxxxxx + yyyyyyyyyy
        //   } bbbbbbbbbbbbb"
        //   ```
        // This isn't decided yet, refer to the relevant discussion:
        // https://github.com/astral-sh/ruff/discussions/9785
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
