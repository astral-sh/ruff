use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum CheckKind {
    ImportStarUsage,
    IfTuple,
}

impl CheckKind {
    /// A four-letter shorthand code for the check.
    pub fn code(&self) -> &'static str {
        match self {
            CheckKind::ImportStarUsage => "F403",
            CheckKind::IfTuple => "F634",
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> &'static str {
        match self {
            CheckKind::ImportStarUsage => "Unable to detect undefined names",
            CheckKind::IfTuple => "If test is a tuple, which is always `True`",
        }
    }
}

pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
}
