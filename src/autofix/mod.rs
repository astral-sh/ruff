use rustpython_ast::Location;
use serde::{Deserialize, Serialize};

pub mod fixer;
pub mod helpers;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Fix {
    pub content: String,
    pub location: Location,
    pub end_location: Location,
}

impl Fix {
    pub fn deletion(start: Location, end: Location) -> Self {
        Self {
            content: String::new(),
            location: start,
            end_location: end,
        }
    }

    pub fn replacement(content: String, start: Location, end: Location) -> Self {
        Self {
            content,
            location: start,
            end_location: end,
        }
    }

    pub fn insertion(content: String, at: Location) -> Self {
        Self {
            content,
            location: at,
            end_location: at,
        }
    }
}
