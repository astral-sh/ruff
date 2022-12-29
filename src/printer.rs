use std::collections::BTreeMap;
use std::path::Path;

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use anyhow::Result;
use colored::Colorize;
use itertools::iterate;
use rustpython_parser::ast::Location;
use serde::Serialize;
use serde_json::json;

use crate::autofix::{fixer, Fix};
use crate::checks::CheckCode;
use crate::fs::relativize_path;
use crate::linter::Diagnostics;
use crate::logging::LogLevel;
use crate::message::Message;
use crate::settings::types::SerializationFormat;
use crate::tell_user;

/// Enum to control whether lint violations are shown to the user.
pub enum Violations {
    Show,
    Hide,
}

#[derive(Serialize)]
struct ExpandedMessage<'a> {
    code: &'a CheckCode,
    message: String,
    fix: Option<&'a Fix>,
    location: Location,
    end_location: Location,
    filename: &'a str,
}

pub struct Printer<'a> {
    format: &'a SerializationFormat,
    log_level: &'a LogLevel,
    autofix: &'a fixer::Mode,
    violations: &'a Violations,
}

impl<'a> Printer<'a> {
    pub fn new(
        format: &'a SerializationFormat,
        log_level: &'a LogLevel,
        autofix: &'a fixer::Mode,
        violations: &'a Violations,
    ) -> Self {
        Self {
            format,
            log_level,
            autofix,
            violations,
        }
    }

    pub fn write_to_user(&self, message: &str) {
        if self.log_level >= &LogLevel::Default {
            tell_user!("{}", message);
        }
    }

    fn post_text(&self, diagnostics: &Diagnostics) {
        if self.log_level >= &LogLevel::Default {
            match self.violations {
                Violations::Show => {
                    let fixed = diagnostics.fixed;
                    let remaining = diagnostics.messages.len();
                    let total = fixed + remaining;
                    if fixed > 0 {
                        println!("Found {total} error(s) ({fixed} fixed, {remaining} remaining).");
                    } else if remaining > 0 {
                        println!("Found {remaining} error(s).");
                    }

                    if !matches!(self.autofix, fixer::Mode::Apply) {
                        let num_fixable = diagnostics
                            .messages
                            .iter()
                            .filter(|message| message.kind.fixable())
                            .count();
                        if num_fixable > 0 {
                            println!("{num_fixable} potentially fixable with the --fix option.");
                        }
                    }
                }
                Violations::Hide => {
                    let fixed = diagnostics.fixed;
                    if fixed > 0 {
                        if matches!(self.autofix, fixer::Mode::Apply) {
                            println!("Fixed {fixed} error(s).");
                        } else if matches!(self.autofix, fixer::Mode::Diff) {
                            println!("Would fix {fixed} error(s).");
                        }
                    }
                }
            }
        }
    }

    pub fn write_once(&self, diagnostics: &Diagnostics) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        if matches!(self.violations, Violations::Hide) {
            if matches!(
                self.format,
                SerializationFormat::Text | SerializationFormat::Grouped
            ) {
                self.post_text(diagnostics);
            }
            return Ok(());
        }

        match self.format {
            SerializationFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &diagnostics
                            .messages
                            .iter()
                            .map(|message| ExpandedMessage {
                                code: message.kind.code(),
                                message: message.kind.body(),
                                fix: message.fix.as_ref(),
                                location: message.location,
                                end_location: message.end_location,
                                filename: &message.filename,
                            })
                            .collect::<Vec<_>>()
                    )?
                );
            }
            SerializationFormat::Junit => {
                use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite};

                let mut report = Report::new("ruff");
                for (filename, messages) in group_messages_by_filename(&diagnostics.messages) {
                    let mut test_suite = TestSuite::new(filename);
                    test_suite
                        .extra
                        .insert("package".to_string(), "org.ruff".to_string());
                    for message in messages {
                        let mut status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                        status.set_message(message.kind.body());
                        status.set_description(format!(
                            "line {}, col {}, {}",
                            message.location.row(),
                            message.location.column(),
                            message.kind.body()
                        ));
                        let mut case =
                            TestCase::new(format!("org.ruff.{}", message.kind.code()), status);
                        let file_path = Path::new(filename);
                        let file_stem = file_path.file_stem().unwrap().to_str().unwrap();
                        let classname = file_path.parent().unwrap().join(file_stem);
                        case.set_classname(classname.to_str().unwrap());
                        case.extra
                            .insert("line".to_string(), message.location.row().to_string());
                        case.extra
                            .insert("column".to_string(), message.location.column().to_string());

                        test_suite.add_test_case(case);
                    }
                    report.add_test_suite(test_suite);
                }
                println!("{}", report.to_string().unwrap());
            }
            SerializationFormat::Text => {
                for message in &diagnostics.messages {
                    print_message(message);
                }

                self.post_text(diagnostics);
            }
            SerializationFormat::Grouped => {
                for (filename, messages) in group_messages_by_filename(&diagnostics.messages) {
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

                self.post_text(diagnostics);
            }
            SerializationFormat::Github => {
                // Generate error workflow command in GitHub Actions format
                // https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions#setting-an-error-message
                diagnostics.messages.iter().for_each(|message| {
                    println!(
                        "::notice title=Ruff,file={},line={},col={},endLine={},endColumn={}::({}) \
                         {}",
                        relativize_path(Path::new(&message.filename)),
                        message.location.row(),
                        message.location.column(),
                        message.end_location.row(),
                        message.end_location.column(),
                        message.kind.code(),
                        message.kind.body(),
                    );
                });
            }
            SerializationFormat::Gitlab => {
                // Generate JSON with errors in GitLab CI format
                // https://docs.gitlab.com/ee/ci/testing/code_quality.html#implementing-a-custom-tool
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &diagnostics
                            .messages
                            .iter()
                            .map(|message| {
                                json!({
                                    "description": format!("({}) {}", message.kind.code(), message.kind.body()),
                                    "severity": "major",
                                    "fingerprint": message.kind.code(),
                                    "location": {
                                        "path": relativize_path(Path::new(&message.filename)),
                                        "lines": {
                                            "begin": message.location.row(),
                                            "end": message.end_location.row()
                                        }
                                    }
                                })
                            }
                        )
                        .collect::<Vec<_>>()
                    )?
                );
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

fn group_messages_by_filename(messages: &Vec<Message>) -> BTreeMap<&String, Vec<&Message>> {
    let mut grouped_messages = BTreeMap::default();
    for message in messages {
        grouped_messages
            .entry(&message.filename)
            .or_insert_with(Vec::new)
            .push(message);
    }
    grouped_messages
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
