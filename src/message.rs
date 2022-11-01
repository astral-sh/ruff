use std::cmp::Ordering;
use std::fmt;
use std::path::Path;

use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use crate::checks::{Check, CheckKind};
use crate::fs::relativize_path;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: CheckKind,
    pub fixed: bool,
    pub location: Location,
    pub end_location: Location,
    pub filename: String,
}

impl Message {
    pub fn from_check(filename: String, check: Check) -> Self {
        Self {
            kind: check.kind,
            fixed: check.fix.map(|fix| fix.applied).unwrap_or_default(),
            location: Location::new(check.location.row(), check.location.column() + 1),
            end_location: Location::new(check.end_location.row(), check.end_location.column() + 1),
            filename,
        }
    }
}

impl Ord for Message {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.filename, self.location.row(), self.location.column()).cmp(&(
            &other.filename,
            other.location.row(),
            other.location.column(),
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
            relativize_path(Path::new(&self.filename)).white().bold(),
            ":".cyan(),
            self.location.row(),
            ":".cyan(),
            self.location.column(),
            ":".cyan(),
            self.kind.code().as_ref().red().bold(),
            self.kind.body()
        )
    }
}
