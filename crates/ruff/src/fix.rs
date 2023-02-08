use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Hash)]
pub enum FixMode {
    Generate,
    Apply,
    Diff,
    None,
}

impl From<bool> for FixMode {
    fn from(value: bool) -> Self {
        if value {
            Self::Apply
        } else {
            Self::None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
