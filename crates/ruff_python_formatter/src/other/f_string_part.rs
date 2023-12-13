use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::FStringPart;

use crate::other::f_string::FormatFString;
use crate::prelude::*;
use crate::string::StringOptions;

#[derive(Default)]
pub struct FormatFStringPart {
    options: StringOptions,
}

impl FormatRuleWithOptions<FStringPart, PyFormatContext<'_>> for FormatFStringPart {
    type Options = StringOptions;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.options = options;
        self
    }
}

impl FormatRule<FStringPart, PyFormatContext<'_>> for FormatFStringPart {
    fn fmt(&self, item: &FStringPart, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            FStringPart::Literal(string_literal) => {
                string_literal.format().with_options(self.options).fmt(f)
            }
            FStringPart::FString(f_string) => {
                FormatFString::new(f_string, self.options.quoting()).fmt(f)
            }
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
