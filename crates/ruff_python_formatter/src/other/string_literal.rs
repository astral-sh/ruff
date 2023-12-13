use ruff_formatter::FormatRuleWithOptions;
use ruff_python_ast::StringLiteral;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::string::{docstring, StringContext, StringPart};
use crate::QuoteStyle;

#[derive(Default)]
pub struct FormatStringLiteral {
    context: StringContext,
}

impl FormatRuleWithOptions<StringLiteral, PyFormatContext<'_>> for FormatStringLiteral {
    type Options = StringContext;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.context = options;
        self
    }
}

impl FormatNodeRule<StringLiteral> for FormatStringLiteral {
    fn fmt_fields(&self, item: &StringLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        let locator = f.context().locator();
        let parent_docstring_quote_style = f.context().docstring();

        let quote_style = if self.context.is_docstring() {
            // Per PEP 8 and PEP 257, always prefer double quotes for docstrings
            QuoteStyle::Double
        } else {
            f.options().quote_style()
        };

        let normalized = StringPart::from_source(item.range(), &locator).normalize(
            self.context.quoting(),
            &locator,
            quote_style,
            parent_docstring_quote_style,
        );

        if self.context.is_docstring() {
            docstring::format(&normalized, f)
        } else {
            normalized.fmt(f)
        }
    }
}
