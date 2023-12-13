use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::FString;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::string::{StringContext, StringPart};

#[derive(Default)]
pub struct FormatFString {
    context: StringContext,
}

impl FormatRuleWithOptions<FString, PyFormatContext<'_>> for FormatFString {
    type Options = StringContext;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.context = options;
        self
    }
}

impl FormatNodeRule<FString> for FormatFString {
    fn fmt_fields(&self, item: &FString, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();
        let parent_docstring_quote_style = f.context().docstring();

        let result = StringPart::from_source(item.range(), &locator)
            .normalize(
                self.context.quoting(),
                &locator,
                f.options().quote_style(),
                parent_docstring_quote_style,
            )
            .fmt(f);

        // TODO(dhruvmanila): With PEP 701, comments can be inside f-strings.
        // This is to mark all of those comments as formatted but we need to
        // figure out how to handle them. Note that this needs to be done only
        // after the f-string is formatted, so only for all the non-formatted
        // comments.
        let comments = f.context().comments();
        item.elements.iter().for_each(|value| {
            comments.mark_verbatim_node_comments_formatted(value.into());
        });

        result
    }
}
