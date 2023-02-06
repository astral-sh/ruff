use std::cmp::Ordering;

pub use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::{Diagnostic, DiagnosticKind};
use crate::source_code::Locator;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: DiagnosticKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
    pub filename: String,
    pub source: Option<Source>,
}

impl Message {
    pub fn from_diagnostic(
        diagnostic: Diagnostic,
        filename: String,
        source: Option<Source>,
    ) -> Self {
        Self {
            kind: diagnostic.kind,
            location: Location::new(diagnostic.location.row(), diagnostic.location.column() + 1),
            end_location: Location::new(
                diagnostic.end_location.row(),
                diagnostic.end_location.column() + 1,
            ),
            fix: diagnostic.fix,
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
    pub fn from_diagnostic(diagnostic: &Diagnostic, locator: &Locator) -> Self {
        let location = Location::new(diagnostic.location.row(), 0);
        // Diagnostics can already extend one-past-the-end. If they do, though, then
        // they'll end at the start of a line. We need to avoid extending by yet another
        // line past-the-end.
        let end_location = if diagnostic.end_location.column() == 0 {
            diagnostic.end_location
        } else {
            Location::new(diagnostic.end_location.row() + 1, 0)
        };
        let source = locator.slice_source_code_range(&Range::new(location, end_location));
        let num_chars_in_range = locator
            .slice_source_code_range(&Range::new(diagnostic.location, diagnostic.end_location))
            .chars()
            .count();
        Source {
            contents: source.to_string(),
            range: (
                diagnostic.location.column(),
                diagnostic.location.column() + num_chars_in_range,
            ),
        }
    }
}
