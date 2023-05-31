use crate::comments::Comments;
use ruff_formatter::{FormatContext, SimpleFormatOptions, SourceCode};
//use ruff_python_ast::source_code::Locator;
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub(crate) struct PyFormatContext<'a> {
    options: SimpleFormatOptions,
    contents: &'a str,
    comments: Comments<'a>,
}

impl<'a> PyFormatContext<'a> {
    pub(crate) fn new(
        options: SimpleFormatOptions,
        contents: &'a str,
        comments: Comments<'a>,
    ) -> Self {
        Self {
            options,
            contents,
            comments,
        }
    }

    //pub(crate) fn contents(&self) -> &'a str {
    //    self.contents
    //}
    //
    //pub(crate) fn locator(&self) -> Locator<'a> {
    //    Locator::new(self.contents)
    //}

    #[allow(unused)]
    pub(crate) fn comments(&self) -> &Comments<'a> {
        &self.comments
    }
}

impl FormatContext for PyFormatContext<'_> {
    type Options = SimpleFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }

    fn source_code(&self) -> SourceCode {
        SourceCode::new(self.contents)
    }
}

impl Debug for PyFormatContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyFormatContext")
            .field("options", &self.options)
            .field("comments", &self.comments.debug(self.source_code()))
            .field("source", &self.contents)
            .finish()
    }
}
