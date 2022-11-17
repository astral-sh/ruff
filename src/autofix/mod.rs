use rustpython_ast::Location;
use serde::{Deserialize, Serialize};

pub mod fixer;
pub mod helpers;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Patch {
    pub content: String,
    pub location: Location,
    pub end_location: Location,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    pub patch: Patch,
    pub applied: bool,
}

impl Fix {
    pub fn deletion(start: Location, end: Location) -> Self {
        Self {
            patch: Patch {
                content: "".to_string(),
                location: start,
                end_location: end,
            },
            applied: false,
        }
    }

    pub fn replacement(content: String, start: Location, end: Location) -> Self {
        Self {
            patch: Patch {
                content,
                location: start,
                end_location: end,
            },
            applied: false,
        }
    }

    pub fn insertion(content: String, at: Location) -> Self {
        Self {
            patch: Patch {
                content,
                location: at,
                end_location: at,
            },
            applied: false,
        }
    }

    pub fn dummy(location: Location) -> Self {
        Self {
            patch: Patch {
                content: "".to_string(),
                location,
                end_location: location,
            },
            applied: false,
        }
    }
}
