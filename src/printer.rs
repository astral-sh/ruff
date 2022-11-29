use std::collections::BTreeMap;
use std::path::Path;

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;
use itertools::iterate;
use rustpython_parser::ast::Location;
use serde::Serialize;

use crate::checks::{CheckCode, CheckKind};
use crate::fs::relativize_path;
use crate::linter::Diagnostics;
use crate::logging::LogLevel;
use crate::message::Message;
use crate::tell_user;

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq, Debug)]
pub enum SerializationFormat {
    Text,
    Json,
    Grouped,
}

#[derive(Serialize)]
struct ExpandedMessage<'a> {
    kind: &'a CheckKind,
    code: &'a CheckCode,
    message: String,
    location: Location,
    end_location: Location,
    filename: &'a str,
}

pub struct Printer<'a> {
    format: &'a SerializationFormat,
    log_level: &'a LogLevel,
}

impl<'a> Printer<'a> {
    pub fn new(format: &'a SerializationFormat, log_level: &'a LogLevel) -> Self {
        Self { format, log_level }
    }

    pub fn write_to_user(&self, message: &str) {
        if self.log_level >= &LogLevel::Default {
            tell_user!("{}", message);
        }
    }

    fn pre_text(&self, diagnostics: &Diagnostics) {
        if self.log_level >= &LogLevel::Default {
            if diagnostics.fixed > 0 {
                println!(
                    "Found {} error(s) ({} fixed).",
                    diagnostics.messages.len(),
                    diagnostics.fixed,
                );
            } else if !diagnostics.messages.is_empty() {
                println!("Found {} error(s).", diagnostics.messages.len());
            }
        }
    }

    fn post_text(&self, num_fixable: usize) {
        if self.log_level >= &LogLevel::Default {
            if num_fixable > 0 {
                println!("{num_fixable} potentially fixable with the --fix option.");
            }
        }
    }

    pub fn write_once(&self, diagnostics: &Diagnostics) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        let num_fixable = diagnostics
            .messages
            .iter()
            .filter(|message| message.kind.fixable())
            .count();

        match self.format {
            SerializationFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &diagnostics
                            .messages
                            .iter()
                            .map(|message| ExpandedMessage {
                                kind: &message.kind,
                                code: message.kind.code(),
                                message: message.kind.body(),
                                location: message.location,
                                end_location: message.end_location,
                                filename: &message.filename,
                            })
                            .collect::<Vec<_>>()
                    )?
                );
            }
            SerializationFormat::Text => {
                self.pre_text(diagnostics);

                for message in &diagnostics.messages {
                    print_message(message);
                }

                self.post_text(num_fixable);
            }
            SerializationFormat::Grouped => {
                self.pre_text(diagnostics);
                println!();

                // Group by filename.
                let mut grouped_messages = BTreeMap::default();
                for message in &diagnostics.messages {
                    grouped_messages
                        .entry(&message.filename)
                        .or_insert_with(Vec::new)
                        .push(message);
                }

                for (filename, messages) in grouped_messages {
                    // Compute the maximum number of digits in the row and column, for messages in
                    // this file.
                    let row_length = num_digits(
                        messages
                            .iter()
                            .map(|message| message.location.row())
                            .max()
                            .unwrap(),
                    );
                    let column_length = num_digits(
                        messages
                            .iter()
                            .map(|message| message.location.column())
                            .max()
                            .unwrap(),
                    );

                    // Print the filename.
                    println!("{}:", relativize_path(Path::new(&filename)).underline());

                    // Print each message.
                    for message in messages {
                        print_grouped_message(message, row_length, column_length);
                    }
                    println!();
                }

                self.post_text(num_fixable);
            }
        }

        Ok(())
    }

    pub fn write_continuously(&self, diagnostics: &Diagnostics) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        if self.log_level >= &LogLevel::Default {
            tell_user!(
                "Found {} error(s). Watching for file changes.",
                diagnostics.messages.len()
            );
        }

        if !diagnostics.messages.is_empty() {
            if self.log_level >= &LogLevel::Default {
                println!();
            }
            for message in &diagnostics.messages {
                print_message(message);
            }
        }

        Ok(())
    }

    pub fn clear_screen(&self) -> Result<()> {
        #[cfg(not(target_family = "wasm"))]
        clearscreen::clear()?;
        Ok(())
    }
}

fn num_digits(n: usize) -> usize {
    iterate(n, |&n| n / 10)
        .take_while(|&n| n > 0)
        .count()
        .max(1)
}

/// Print a single `Message` with full details.
fn print_message(message: &Message) {
    let label = format!(
        "{}{}{}{}{}{} {} {}",
        relativize_path(Path::new(&message.filename)).bold(),
        ":".cyan(),
        message.location.row(),
        ":".cyan(),
        message.location.column(),
        ":".cyan(),
        message.kind.code().as_ref().red().bold(),
        message.kind.body(),
    );
    println!("{label}");
    if let Some(source) = &message.source {
        let snippet = Snippet {
            title: Some(Annotation {
                label: None,
                annotation_type: AnnotationType::Error,
                // The ID (error number) is already encoded in the `label`.
                id: None,
            }),
            footer: vec![],
            slices: vec![Slice {
                source: &source.contents,
                line_start: message.location.row(),
                annotations: vec![SourceAnnotation {
                    label: message.kind.code().as_ref(),
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
                ..FormatOptions::default()
            },
        };
        // Skip the first line, since we format the `label` ourselves.
        let message = DisplayList::from(snippet).to_string();
        let (_, message) = message.split_once('\n').unwrap();
        println!("{message}");
    }
}

/// Print a grouped `Message`, assumed to be printed in a group with others from
/// the same file.
fn print_grouped_message(message: &Message, row_length: usize, column_length: usize) {
    let label = format!(
        "  {}{}{}{}{}  {}  {}",
        " ".repeat(row_length - num_digits(message.location.row())),
        message.location.row(),
        ":".cyan(),
        message.location.column(),
        " ".repeat(column_length - num_digits(message.location.column())),
        message.kind.code().as_ref().red().bold(),
        message.kind.body(),
    );
    println!("{label}");
    if let Some(source) = &message.source {
        let snippet = Snippet {
            title: Some(Annotation {
                label: None,
                annotation_type: AnnotationType::Error,
                // The ID (error number) is already encoded in the `label`.
                id: None,
            }),
            footer: vec![],
            slices: vec![Slice {
                source: &source.contents,
                line_start: message.location.row(),
                annotations: vec![SourceAnnotation {
                    label: message.kind.code().as_ref(),
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
                ..FormatOptions::default()
            },
        };
        // Skip the first line, since we format the `label` ourselves.
        let message = DisplayList::from(snippet).to_string();
        let (_, message) = message.split_once('\n').unwrap();
        let message = textwrap::indent(message, "  ");
        println!("{message}");
    }
}
