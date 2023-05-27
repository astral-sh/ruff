use ruff_formatter::{FormatContext, SimpleFormatOptions, SourceCode};
use ruff_python_ast::source_code::Locator;

#[derive(Clone, Debug)]
pub struct ASTFormatContext<'source> {
    options: SimpleFormatOptions,
    contents: &'source str,
}

impl<'source> ASTFormatContext<'source> {
    pub fn new(options: SimpleFormatOptions, contents: &'source str) -> Self {
        Self { options, contents }
    }

    pub fn contents(&self) -> &'source str {
        self.contents
    }

    pub fn locator(&self) -> Locator<'source> {
        Locator::new(self.contents)
    }
}

impl FormatContext for ASTFormatContext<'_> {
    type Options = SimpleFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }

    fn source_code(&self) -> SourceCode {
        SourceCode::new(self.contents)
    }
}
