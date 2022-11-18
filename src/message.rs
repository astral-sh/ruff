use std::cmp::Ordering;
use std::fmt;
use std::path::Path;

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::fs::relativize_path;
use crate::source_code_locator::SourceCodeLocator;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: CheckKind,
    pub fixed: bool,
    pub location: Location,
    pub end_location: Location,
    pub filename: String,
    pub source: Option<Source>,
}

impl Message {
    pub fn from_check(check: Check, filename: String, source: Option<Source>) -> Self {
        Self {
            kind: check.kind,
            fixed: check.fix.map(|fix| fix.applied).unwrap_or_default(),
            location: Location::new(check.location.row(), check.location.column() + 1),
            end_location: Location::new(check.end_location.row(), check.end_location.column() + 1),
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

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let label = format!(
            "{}{}{}{}{}{} {} {}",
            relativize_path(Path::new(&self.filename)).white().bold(),
            ":".cyan(),
            self.location.row(),
            ":".cyan(),
            self.location.column(),
            ":".cyan(),
            self.kind.code().as_ref().red().bold(),
            self.kind.body(),
        );
        match &self.source {
            None => write!(f, "{}", label),
            Some(source) => {
                let snippet = Snippet {
                    title: Some(Annotation {
                        label: Some(&label),
                        annotation_type: AnnotationType::Error,
                        // The ID (error number) is already encoded in the `label`.
                        id: None,
                    }),
                    footer: vec![],
                    slices: vec![Slice {
                        source: &source.contents,
                        line_start: self.location.row(),
                        annotations: vec![SourceAnnotation {
                            label: self.kind.code().as_ref(),
                            annotation_type: AnnotationType::Error,
                            range: source.range,
                        }],
                        // The origin (file name, line number, and column number) is already encoded
                        // in the `label`.
                        origin: None,
                        fold: false,
                    }],
                    opt: FormatOptions {
                        color: true,
                        ..Default::default()
                    },
                };
                // `split_once(' ')` strips "error: " from `message`.
                let message = DisplayList::from(snippet).to_string();
                let (_, message) = message.split_once(' ').unwrap();
                write!(f, "{}", message)
            }
        }
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
