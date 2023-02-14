use ruff_formatter::{FormatContext, SimpleFormatOptions};

use crate::core::locator::Locator;

pub struct ASTFormatContext<'a> {
    options: SimpleFormatOptions,
    locator: Locator<'a>,
}

impl<'a> ASTFormatContext<'a> {
    pub fn new(options: SimpleFormatOptions, locator: Locator<'a>) -> Self {
        Self { options, locator }
    }
}

impl FormatContext for ASTFormatContext<'_> {
    type Options = SimpleFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }
}

impl<'a> ASTFormatContext<'a> {
    pub fn locator(&'a self) -> &'a Locator {
        &self.locator
    }
}
