use ruff_python_ast::FString;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::string::{Quoting, StringPart};

pub(crate) struct FormatFString<'a> {
    value: &'a FString,
    quoting: Quoting,
}

impl<'a> FormatFString<'a> {
    pub(crate) fn new(value: &'a FString, quoting: Quoting) -> Self {
        Self { value, quoting }
    }
}

impl Format<PyFormatContext<'_>> for FormatFString<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();

        let result = StringPart::from_source(self.value.range(), &locator)
            .normalize(
                self.quoting,
                &locator,
                f.options().quote_style(),
                f.context().docstring(),
            )
            .fmt(f);

        // TODO(dhruvmanila): With PEP 701, comments can be inside f-strings.
        // This is to mark all of those comments as formatted but we need to
        // figure out how to handle them. Note that this needs to be done only
        // after the f-string is formatted, so only for all the non-formatted
        // comments.
        let comments = f.context().comments();
        self.value.elements.iter().for_each(|value| {
            comments.mark_verbatim_node_comments_formatted(value.into());
        });

        result
    }
}
