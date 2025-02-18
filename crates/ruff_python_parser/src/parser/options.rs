use crate::Mode;

#[derive(Debug)]
pub struct ParserOptions {
    /// Specify the mode in which the code will be parsed.
    pub(crate) mode: Mode,
}

impl ParserOptions {
    pub fn from_mode(mode: Mode) -> Self {
        Self { mode }
    }
}
