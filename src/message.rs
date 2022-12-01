use std::cmp::Ordering;

use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checks::{Check, CheckKind};
use crate::source_code_locator::SourceCodeLocator;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: CheckKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
    pub filename: String,
    pub source: Option<Source>,
}

impl Message {
    pub fn from_check(check: Check, filename: String, source: Option<Source>) -> Self {
        Self {
            kind: check.kind,
            location: Location::new(check.location.row(), check.location.column() + 1),
            end_location: Location::new(check.end_location.row(), check.end_location.column() + 1),
            fix: check.fix,
            filename,
            source,
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub contents: String,
    pub range: (usize, usize),
}

impl Source {
    pub fn from_check(check: &Check, locator: &SourceCodeLocator) -> Self {
        let source = locator.slice_source_code_range(&Range {
            location: Location::new(check.location.row(), 0),
            end_location: Location::new(check.end_location.row() + 1, 0),
        });
        let num_chars_in_range = locator
            .slice_source_code_range(&Range {
                location: check.location,
                end_location: check.end_location,
            })
            .chars()
            .count();
        Source {
            contents: source.to_string(),
            range: (
                check.location.column(),
                check.location.column() + num_chars_in_range,
            ),
        }
    }
}
