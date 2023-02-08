use std::collections::BTreeMap;
use std::io;
use std::io::{BufWriter, Write};
use std::path::Path;

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use anyhow::Result;
use colored::Colorize;
use itertools::{iterate, Itertools};
use ruff::fs::relativize_path;
use ruff::logging::LogLevel;
use ruff::message::{Location, Message};
use ruff::registry::Rule;
use ruff::settings::types::SerializationFormat;
use ruff::{fix, notify_user};
use serde::Serialize;
use serde_json::json;

use crate::diagnostics::Diagnostics;

/// Enum to control whether lint violations are shown to the user.
pub enum Violations {
    Show,
    Hide,
}

#[derive(Serialize)]
struct ExpandedFix<'a> {
    content: &'a str,
    message: Option<String>,
    location: &'a Location,
    end_location: &'a Location,
}

#[derive(Serialize)]
struct ExpandedMessage<'a> {
    code: SerializeRuleAsCode<'a>,
    message: String,
    fix: Option<ExpandedFix<'a>>,
    location: Location,
    end_location: Location,
    filename: &'a str,
}

#[derive(Serialize)]
struct ExpandedStatistics<'a> {
    count: usize,
    code: &'a str,
    message: String,
}

struct SerializeRuleAsCode<'a>(&'a Rule);

impl Serialize for SerializeRuleAsCode<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.code())
    }
}

impl<'a> From<&'a Rule> for SerializeRuleAsCode<'a> {
    fn from(rule: &'a Rule) -> Self {
        Self(rule)
    }
}

pub struct Printer<'a> {
    format: &'a SerializationFormat,
    log_level: &'a LogLevel,
    autofix: &'a fix::FixMode,
    violations: &'a Violations,
}

impl<'a> Printer<'a> {
    pub const fn new(
        format: &'a SerializationFormat,
        log_level: &'a LogLevel,
        autofix: &'a fix::FixMode,
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
            notify_user!("{}", message);
        }
    }

    fn post_text<T: Write>(&self, stdout: &mut T, diagnostics: &Diagnostics) -> Result<()> {
        if self.log_level >= &LogLevel::Default {
            match self.violations {
                Violations::Show => {
                    let fixed = diagnostics.fixed;
                    let remaining = diagnostics.messages.len();
                    let total = fixed + remaining;
                    if fixed > 0 {
                        let s = if total == 1 { "" } else { "s" };
                        writeln!(
                            stdout,
                            "Found {total} error{s} ({fixed} fixed, {remaining} remaining)."
                        )?;
                    } else if remaining > 0 {
                        let s = if remaining == 1 { "" } else { "s" };
                        writeln!(stdout, "Found {remaining} error{s}.")?;
                    }

                    if !matches!(self.autofix, fix::FixMode::Apply) {
                        let num_fixable = diagnostics
                            .messages
                            .iter()
                            .filter(|message| message.kind.fixable())
                            .count();
                        if num_fixable > 0 {
                            writeln!(
                                stdout,
                                "[{}] {num_fixable} potentially fixable with the --fix option.",
                                "*".cyan(),
                            )?;
                        }
                    }
                }
                Violations::Hide => {
                    let fixed = diagnostics.fixed;
                    if fixed > 0 {
                        let s = if fixed == 1 { "" } else { "s" };
                        if matches!(self.autofix, fix::FixMode::Apply) {
                            writeln!(stdout, "Fixed {fixed} error{s}.")?;
                        } else if matches!(self.autofix, fix::FixMode::Diff) {
                            writeln!(stdout, "Would fix {fixed} error{s}.")?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn write_once(&self, diagnostics: &Diagnostics) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        if matches!(self.violations, Violations::Hide) {
            let mut stdout = BufWriter::new(io::stdout().lock());
            if matches!(
                self.format,
                SerializationFormat::Text | SerializationFormat::Grouped
            ) {
                self.post_text(&mut stdout, diagnostics)?;
            }
            return Ok(());
        }

        let mut stdout = BufWriter::new(io::stdout().lock());
        match self.format {
            SerializationFormat::Json => {
                writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string_pretty(
                        &diagnostics
                            .messages
                            .iter()
                            .map(|message| ExpandedMessage {
                                code: message.kind.rule().into(),
                                message: message.kind.body(),
                                fix: message.fix.as_ref().map(|fix| ExpandedFix {
                                    content: &fix.content,
                                    location: &fix.location,
                                    end_location: &fix.end_location,
                                    message: message.kind.commit(),
                                }),
                                location: message.location,
                                end_location: message.end_location,
                                filename: &message.filename,
                            })
                            .collect::<Vec<_>>()
                    )?
                )?;
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
                        let mut case = TestCase::new(
                            format!("org.ruff.{}", message.kind.rule().code()),
                            status,
                        );
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
                writeln!(stdout, "{}", report.to_string().unwrap())?;
            }
            SerializationFormat::Text => {
                for message in &diagnostics.messages {
                    print_message(&mut stdout, message)?;
                }

                self.post_text(&mut stdout, diagnostics)?;
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
                    writeln!(
                        stdout,
                        "{}:",
                        relativize_path(Path::new(&filename)).underline()
                    )?;

                    // Print each message.
                    for message in messages {
                        print_grouped_message(&mut stdout, message, row_length, column_length)?;
                    }
                    writeln!(stdout)?;
                }

                self.post_text(&mut stdout, diagnostics)?;
            }
            SerializationFormat::Github => {
                // Generate error workflow command in GitHub Actions format.
                // See: https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions#setting-an-error-message
                for message in &diagnostics.messages {
                    let label = format!(
                        "{}{}{}{}{}{} {} {}",
                        relativize_path(Path::new(&message.filename)),
                        ":",
                        message.location.row(),
                        ":",
                        message.location.column(),
                        ":",
                        message.kind.rule().code(),
                        message.kind.body(),
                    );
                    writeln!(
                        stdout,
                        "::error title=Ruff \
                         ({}),file={},line={},col={},endLine={},endColumn={}::{}",
                        message.kind.rule().code(),
                        message.filename,
                        message.location.row(),
                        message.location.column(),
                        message.end_location.row(),
                        message.end_location.column(),
                        label,
                    )?;
                }
            }
            SerializationFormat::Gitlab => {
                // Generate JSON with violations in GitLab CI format
                // https://docs.gitlab.com/ee/ci/testing/code_quality.html#implementing-a-custom-tool
                writeln!(stdout,
                    "{}",
                    serde_json::to_string_pretty(
                        &diagnostics
                            .messages
                            .iter()
                            .map(|message| {
                                json!({
                                    "description": format!("({}) {}", message.kind.rule().code(), message.kind.body()),
                                    "severity": "major",
                                    "fingerprint": message.kind.rule().code(),
                                    "location": {
                                        "path": message.filename,
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
                )?;
            }
            SerializationFormat::Pylint => {
                // Generate violations in Pylint format.
                // See: https://flake8.pycqa.org/en/latest/internal/formatters.html#pylint-formatter
                for message in &diagnostics.messages {
                    let label = format!(
                        "{}:{}: [{}] {}",
                        relativize_path(Path::new(&message.filename)),
                        message.location.row(),
                        message.kind.rule().code(),
                        message.kind.body(),
                    );
                    writeln!(stdout, "{label}")?;
                }
            }
        }

        stdout.flush()?;

        Ok(())
    }

    pub fn write_statistics(&self, diagnostics: &Diagnostics) -> Result<()> {
        let violations = diagnostics
            .messages
            .iter()
            .map(|message| message.kind.rule())
            .sorted()
            .dedup()
            .collect::<Vec<_>>();
        if violations.is_empty() {
            return Ok(());
        }

        let statistics = violations
            .iter()
            .map(|rule| ExpandedStatistics {
                code: rule.code(),
                count: diagnostics
                    .messages
                    .iter()
                    .filter(|message| message.kind.rule() == *rule)
                    .count(),
                message: diagnostics
                    .messages
                    .iter()
                    .find(|message| message.kind.rule() == *rule)
                    .map(|message| message.kind.body())
                    .unwrap(),
            })
            .collect::<Vec<_>>();

        let mut stdout = BufWriter::new(io::stdout().lock());
        match self.format {
            SerializationFormat::Text => {
                // Compute the maximum number of digits in the count and code, for all messages,
                // to enable pretty-printing.
                let count_width = num_digits(
                    statistics
                        .iter()
                        .map(|statistic| statistic.count)
                        .max()
                        .unwrap(),
                );
                let code_width = statistics
                    .iter()
                    .map(|statistic| statistic.code.len())
                    .max()
                    .unwrap();

                // By default, we mimic Flake8's `--statistics` format.
                for msg in statistics {
                    writeln!(
                        stdout,
                        "{:>count_width$}\t{:<code_width$}\t{}",
                        msg.count, msg.code, msg.message
                    )?;
                }
                return Ok(());
            }
            SerializationFormat::Json => {
                writeln!(stdout, "{}", serde_json::to_string_pretty(&statistics)?)?;
            }
            _ => {
                anyhow::bail!(
                    "Unsupported serialization format for statistics: {:?}",
                    self.format
                )
            }
        }

        stdout.flush()?;

        Ok(())
    }

    pub fn write_continuously(&self, diagnostics: &Diagnostics) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        if self.log_level >= &LogLevel::Default {
            let s = if diagnostics.messages.len() == 1 {
                ""
            } else {
                "s"
            };
            notify_user!(
                "Found {} error{s}. Watching for file changes.",
                diagnostics.messages.len()
            );
        }

        let mut stdout = BufWriter::new(io::stdout().lock());
        if !diagnostics.messages.is_empty() {
            if self.log_level >= &LogLevel::Default {
                writeln!(stdout)?;
            }
            for message in &diagnostics.messages {
                print_message(&mut stdout, message)?;
            }
        }
        stdout.flush()?;

        Ok(())
    }

    pub fn clear_screen() -> Result<()> {
        #[cfg(not(target_family = "wasm"))]
        clearscreen::clear()?;
        Ok(())
    }
}

fn group_messages_by_filename(messages: &[Message]) -> BTreeMap<&String, Vec<&Message>> {
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
fn print_message<T: Write>(stdout: &mut T, message: &Message) -> Result<()> {
    let label = if message.kind.fixable() {
        format!(
            "{}{}{}{}{}{} {} [{}] {}",
            relativize_path(Path::new(&message.filename)).bold(),
            ":".cyan(),
            message.location.row(),
            ":".cyan(),
            message.location.column(),
            ":".cyan(),
            message.kind.rule().code().red().bold(),
            "*".cyan(),
            message.kind.body(),
        )
    } else {
        format!(
            "{}{}{}{}{}{} {} {}",
            relativize_path(Path::new(&message.filename)).bold(),
            ":".cyan(),
            message.location.row(),
            ":".cyan(),
            message.location.column(),
            ":".cyan(),
            message.kind.rule().code().red().bold(),
            message.kind.body(),
        )
    };
    writeln!(stdout, "{label}")?;
    if let Some(source) = &message.source {
        let commit = message.kind.commit();
        let footer = if commit.is_some() {
            vec![Annotation {
                id: None,
                label: commit.as_deref(),
                annotation_type: AnnotationType::Help,
            }]
        } else {
            vec![]
        };
        let snippet = Snippet {
            title: Some(Annotation {
                label: None,
                annotation_type: AnnotationType::Error,
                // The ID (error number) is already encoded in the `label`.
                id: None,
            }),
            footer,
            slices: vec![Slice {
                source: &source.contents,
                line_start: message.location.row(),
                annotations: vec![SourceAnnotation {
                    label: message.kind.rule().code(),
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
        writeln!(stdout, "{message}\n")?;
    }
    Ok(())
}

/// Print a grouped `Message`, assumed to be printed in a group with others from
/// the same file.
fn print_grouped_message<T: Write>(
    stdout: &mut T,
    message: &Message,
    row_length: usize,
    column_length: usize,
) -> Result<()> {
    let label = if message.kind.fixable() {
        format!(
            "  {}{}{}{}{}  {}  [{}] {}",
            " ".repeat(row_length - num_digits(message.location.row())),
            message.location.row(),
            ":".cyan(),
            message.location.column(),
            " ".repeat(column_length - num_digits(message.location.column())),
            message.kind.rule().code().red().bold(),
            "*".cyan(),
            message.kind.body(),
        )
    } else {
        format!(
            "  {}{}{}{}{}  {}  {}",
            " ".repeat(row_length - num_digits(message.location.row())),
            message.location.row(),
            ":".cyan(),
            message.location.column(),
            " ".repeat(column_length - num_digits(message.location.column())),
            message.kind.rule().code().red().bold(),
            message.kind.body(),
        )
    };
    writeln!(stdout, "{label}")?;
    if let Some(source) = &message.source {
        let commit = message.kind.commit();
        let footer = if commit.is_some() {
            vec![Annotation {
                id: None,
                label: commit.as_deref(),
                annotation_type: AnnotationType::Help,
            }]
        } else {
            vec![]
        };
        let snippet = Snippet {
            title: Some(Annotation {
                label: None,
                annotation_type: AnnotationType::Error,
                // The ID (error number) is already encoded in the `label`.
                id: None,
            }),
            footer,
            slices: vec![Slice {
                source: &source.contents,
                line_start: message.location.row(),
                annotations: vec![SourceAnnotation {
                    label: message.kind.rule().code(),
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
        writeln!(stdout, "{message}")?;
    }
    Ok(())
}
