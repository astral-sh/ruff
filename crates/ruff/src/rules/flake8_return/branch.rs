use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Branch {
    Elif,
    Else,
}

impl fmt::Display for Branch {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Branch::Elif => fmt.write_str("elif"),
            Branch::Else => fmt.write_str("else"),
        }
    }
}
