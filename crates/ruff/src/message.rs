use std::cmp::Ordering;

pub use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Fix};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: DiagnosticKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
    pub filename: String,
    pub source: Option<Source>,
    pub noqa_row: usize,
}

impl Message {
    pub fn from_diagnostic(
        diagnostic: Diagnostic,
        filename: String,
        source: Option<Source>,
        noqa_row: usize,
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
            noqa_row,
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
        let source = locator.slice(Range::new(location, end_location));
        let num_chars_in_range = locator
            .slice(Range::new(diagnostic.location, diagnostic.end_location))
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
