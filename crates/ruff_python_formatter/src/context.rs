use std::rc::Rc;

use ruff_formatter::{FormatContext, SimpleFormatOptions};
use ruff_python_ast::source_code::Locator;

pub struct ASTFormatContext {
    options: SimpleFormatOptions,
    contents: Rc<str>,
}

impl ASTFormatContext {
    pub fn new(options: SimpleFormatOptions, contents: &str) -> Self {
        Self {
            options,
            contents: Rc::from(contents),
        }
    }
}

impl FormatContext for ASTFormatContext {
    type Options = SimpleFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }
}

impl ASTFormatContext {
    pub fn contents(&self) -> Rc<str> {
        self.contents.clone()
    }

    pub fn locator(&self) -> Locator {
        Locator::new(&self.contents)
    }
}
