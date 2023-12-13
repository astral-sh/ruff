use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::FStringPart;

use crate::prelude::*;
use crate::string::StringContext;

#[derive(Default)]
pub struct FormatFStringPart {
    context: StringContext,
}

impl FormatRuleWithOptions<FStringPart, PyFormatContext<'_>> for FormatFStringPart {
    type Options = StringContext;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.context = options;
        self
    }
}

impl FormatRule<FStringPart, PyFormatContext<'_>> for FormatFStringPart {
    fn fmt(&self, item: &FStringPart, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            FStringPart::Literal(string_literal) => {
                string_literal.format().with_options(self.context).fmt(f)
            }
            FStringPart::FString(f_string) => f_string.format().with_options(self.context).fmt(f),
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for FStringPart {
    type Format<'a> = FormatRefWithRule<'a, FStringPart, FormatFStringPart, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatFStringPart::default())
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for FStringPart {
    type Format = FormatOwnedWithRule<FStringPart, FormatFStringPart, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatFStringPart::default())
    }
}
