use rustpython_parser::ast::Location;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Fix {
    pub content: String,
    pub location: Location,
    pub end_location: Location,
}

impl Fix {
    pub const fn deletion(start: Location, end: Location) -> Self {
        Self {
            content: String::new(),
            location: start,
            end_location: end,
        }
    }

    pub fn replacement(content: String, start: Location, end: Location) -> Self {
        debug_assert!(!content.is_empty(), "Prefer `Fix::deletion`");

        Self {
            content,
            location: start,
            end_location: end,
        }
    }

    pub fn insertion(content: String, at: Location) -> Self {
        debug_assert!(!content.is_empty(), "Insert content is empty");

        Self {
            content,
            location: at,
            end_location: at,
        }
    }
}
