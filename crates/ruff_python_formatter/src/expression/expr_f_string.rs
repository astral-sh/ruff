use memchr::memchr2;

use ruff_formatter::{FormatResult, FormatRuleWithOptions};
use ruff_python_ast::{AnyNodeRef, ExprFString};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::comments::SourceComment;
use crate::expression::parentheses::{
    in_parentheses_only_group, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;
use crate::string::{AnyString, FormatStringContinuation, Quoting, StringContext};

#[derive(Default)]
pub struct FormatExprFString {
    context: StringContext,
}

impl FormatRuleWithOptions<ExprFString, PyFormatContext<'_>> for FormatExprFString {
    type Options = StringContext;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.context = options;
        self
    }
}

impl FormatNodeRule<ExprFString> for FormatExprFString {
    fn fmt_fields(&self, item: &ExprFString, f: &mut PyFormatter) -> FormatResult<()> {
        let context = self
            .context
            .with_quoting(f_string_quoting(item, &f.context().locator()));

        match item.value.as_slice() {
            [f_string_part] => f_string_part.format().with_options(context).fmt(f),
            _ => in_parentheses_only_group(
                &FormatStringContinuation::new(&AnyString::FString(item)).with_context(context),
            )
            .fmt(f),
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
        } else if memchr2(b'\n', b'\r', context.source()[self.range].as_bytes()).is_none() {
            OptionalParentheses::BestFit
        } else {
            OptionalParentheses::Never
        }
    }
}

fn f_string_quoting(f_string: &ExprFString, locator: &Locator) -> Quoting {
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
