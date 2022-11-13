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
    pub source: Option<String>,
    pub range: Option<(usize, usize)>,
    pub show_source: bool,
}

impl Message {
    pub fn from_check(
        filename: String,
        check: Check,
        locator: &SourceCodeLocator,
        show_source: bool,
    ) -> Self {
        let source = locator.slice_source_code_range(&Range {
            location: Location::new(check.location.row(), 0),
            end_location: Location::new(check.end_location.row() + 1, 0),
        });
        let num_chars_in_range = locator
            .slice_source_code_range(&Range {
                location: check.location,
                end_location: check.end_location,
            })
            .len();
        Self {
            kind: check.kind,
            fixed: check.fix.map(|fix| fix.applied).unwrap_or_default(),
            location: Location::new(check.location.row(), check.location.column() + 1),
            end_location: Location::new(check.end_location.row(), check.end_location.column() + 1),
            filename,
            source: Some(source.to_string()),
            range: Some((
                check.location.column(),
                check.location.column() + num_chars_in_range,
            )),
            show_source,
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
        let slices = if self.show_source && self.source.is_some() && self.range.is_some() {
            vec![Slice {
                source: self.source.as_ref().unwrap(),
                line_start: self.location.row(),
                origin: None,
                fold: false,
                annotations: vec![SourceAnnotation {
                    label: "",
                    annotation_type: AnnotationType::Error,
                    range: self.range.unwrap(),
                }],
            }]
        } else {
            vec![]
        };
        let snippet = Snippet {
            title: Some(Annotation {
                label: Some(&label),
                id: None,
                annotation_type: AnnotationType::Error,
            }),
            footer: vec![],
            slices,
            opt: FormatOptions {
                color: true,
                ..Default::default()
            },
        };
        let mut message = DisplayList::from(snippet).to_string();
        if self.show_source {
            message.push('\n');
        }
        // `split_once(' ').unwrap().1` stirps "error: " from `message`.
        // Note `message` contains ANSI color codes.
        write!(f, "{}", message.split_once(' ').unwrap().1)
    }
}
