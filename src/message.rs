use std::cmp::Ordering;
use std::path::Path;
use std::{fmt, fs};

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
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

impl Message {
    pub fn to_annotated_source(&self) -> String {
        let source = fs::read_to_string(&self.filename).unwrap();
        let error_lines = source
            .lines()
            .skip(self.location.row() - 1)
            .take(self.end_location.row() - self.location.row() + 1)
            .collect::<Vec<_>>();
        let range_end = if self.location.row() == self.end_location.row() {
            self.end_location.column()
        } else {
            error_lines
                .iter()
                .enumerate()
                .map(|(row, line)| {
                    if row == error_lines.len() - 1 {
                        self.end_location.column()
                    } else {
                        line.len() + 1
                    }
                })
                .sum()
        };
        let body = self.kind.body();
        let code = self.kind.code().as_ref();
        let rel_path = relativize_path(Path::new(&self.filename));
        let source = error_lines.join("\n");
        let snippet = Snippet {
            title: Some(Annotation {
                label: Some(&body),
                id: Some(code),
                annotation_type: AnnotationType::Error,
            }),
            footer: vec![],
            slices: vec![Slice {
                source: &source,
                line_start: self.location.row(),
                origin: Some(&rel_path),
                fold: false,
                annotations: vec![SourceAnnotation {
                    label: "",
                    annotation_type: AnnotationType::Error,
                    range: (self.location.column() - 1, range_end - 1),
                }],
            }],
            opt: FormatOptions {
                color: true,
                ..Default::default()
            },
        };
        DisplayList::from(snippet).to_string()
    }
}
