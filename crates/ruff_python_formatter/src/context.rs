use std::rc::Rc;

use ruff_formatter::{FormatContext, SimpleFormatOptions};
use ruff_python_ast::source_code::Locator;

pub struct ASTFormatContext<'a> {
    options: SimpleFormatOptions,
    contents: Rc<str>,
    locator: Locator<'a>,
}

impl<'a> ASTFormatContext<'a> {
    pub fn new(options: SimpleFormatOptions, locator: Locator<'a>) -> Self {
        Self {
            options,
            contents: Rc::from(locator.contents()),
            locator,
        }
    }
}

impl FormatContext for ASTFormatContext<'_> {
    type Options = SimpleFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }
}

impl<'a> ASTFormatContext<'a> {
    pub fn contents(&'a self) -> Rc<str> {
        self.contents.clone()
    }

    pub fn locator(&'a self) -> &'a Locator {
        &self.locator
    }
}
