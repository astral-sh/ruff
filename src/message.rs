use std::cmp::Ordering;
use std::fmt;

use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use crate::checks::CheckKind;

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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: CheckKind,
    #[serde(with = "LocationDef")]
    pub location: Location,
    pub filename: String,
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.filename, self.location.row(), self.location.column()).cmp(&(
            &other.filename,
            other.location.row(),
            self.location.column(),
        ))
    }
}

impl PartialOrd for Message {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}{}{} {} {}",
            self.filename.white().bold(),
            ":".cyan(),
            self.location.row(),
            ":".cyan(),
            self.location.column(),
            ":".cyan(),
            self.kind.code().as_str().red().bold(),
            self.kind.body()
        )
    }
}
