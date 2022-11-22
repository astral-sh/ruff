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
}

impl Fix {
    pub fn deletion(start: Location, end: Location) -> Self {
        Self {
            patch: Patch {
                content: String::new(),
                location: start,
                end_location: end,
            },
        }
    }

    pub fn replacement(content: String, start: Location, end: Location) -> Self {
        Self {
            patch: Patch {
                content,
                location: start,
                end_location: end,
            },
        }
    }

    pub fn insertion(content: String, at: Location) -> Self {
        Self {
            patch: Patch {
                content,
                location: at,
                end_location: at,
            },
        }
    }

    pub fn dummy(location: Location) -> Self {
        Self {
            patch: Patch {
                content: String::new(),
                location,
                end_location: location,
            },
        }
    }
}
