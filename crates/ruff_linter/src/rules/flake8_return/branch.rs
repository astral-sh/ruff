use std::fmt;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum Branch {
    Elif,
    Else,
}

impl fmt::Display for Branch {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Elif => fmt.write_str("elif"),
            Self::Else => fmt.write_str("else"),
        }
    }
}
