use crate::Mode;

#[derive(Debug)]
pub struct ParseOptions {
    /// Specify the mode in which the code will be parsed.
    pub(crate) mode: Mode,
}

impl ParseOptions {
    pub fn from_mode(mode: Mode) -> Self {
        Self { mode }
    }
}
