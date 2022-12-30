use rustpython_ast::Location;
use serde::{Deserialize, Serialize};

pub mod fixer;
pub mod helpers;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Fix {
    pub location: Location,
    pub end_location: Location,
    pub content: String,
    pub message: Option<String>,
}

impl Fix {
    pub fn deletion(start: Location, end: Location) -> Self {
        Self {
            location: start,
            end_location: end,
            content: String::new(),
            message: None,
        }
    }

    pub fn replacement(content: String, start: Location, end: Location) -> Self {
        Self {
            location: start,
            end_location: end,
            content,
            message: None,
        }
    }

    pub fn insertion(content: String, at: Location) -> Self {
        Self {
            location: at,
            end_location: at,
            content,
            message: None,
        }
    }

    #[must_use]
    pub fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }
}
