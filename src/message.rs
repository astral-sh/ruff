use std::fmt;

use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(remote = "Location")]
struct LocationDef {
    #[serde(getter = "Location::row")]
    row: usize,
    #[serde(getter = "Location::column")]
    column: usize,
}

impl From<LocationDef> for Location {
    fn from(def: LocationDef) -> Location {
        Location::new(def.row, def.column)
    }
}

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

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub kind: CheckKind,
    #[serde(with = "LocationDef")]
    pub location: Location,
    pub filename: String,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}{}\t{}\t{}",
            self.filename.white().bold(),
            ":".cyan(),
            self.location.column(),
            ":".cyan(),
            self.location.row(),
            self.kind.code().red().bold(),
            self.kind.body()
        )
    }
}
