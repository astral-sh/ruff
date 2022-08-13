use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    DuplicateArgumentName,
    ImportStarUsage,
    IfTuple,
    LineTooLong,
}

impl CheckKind {
    /// A four-letter shorthand code for the check.
    pub fn code(&self) -> &'static str {
        match self {
            CheckKind::DuplicateArgumentName => "F831",
            CheckKind::IfTuple => "F634",
            CheckKind::ImportStarUsage => "F403",
            CheckKind::LineTooLong => "E501",
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> &'static str {
        match self {
            CheckKind::DuplicateArgumentName => "Duplicate argument name in function definition",
            CheckKind::IfTuple => "If test is a tuple, which is always `True`",
            CheckKind::ImportStarUsage => "Unable to detect undefined names",
            CheckKind::LineTooLong => "Line too long (> 79 characters)",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
}
