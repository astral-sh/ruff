use rome_formatter::{FormatContext, SimpleFormatOptions, TransformSourceMap};

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

    fn source_map(&self) -> Option<&TransformSourceMap> {
        None
    }
}

impl<'a> ASTFormatContext<'a> {
    pub fn locator(&'a self) -> &'a Locator {
        &self.locator
    }
}
