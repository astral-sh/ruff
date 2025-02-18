use ruff_python_ast::PySourceType;

use crate::{AsMode, Mode};

#[derive(Debug)]
pub struct ParseOptions {
    /// Specify the mode in which the code will be parsed.
    pub(crate) mode: Mode,
}

impl From<Mode> for ParseOptions {
    fn from(mode: Mode) -> Self {
        Self { mode }
    }
}

impl From<PySourceType> for ParseOptions {
    fn from(source_type: PySourceType) -> Self {
        Self {
            mode: source_type.as_mode(),
        }
    }
}
