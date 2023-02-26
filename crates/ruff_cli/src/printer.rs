use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::io;
use std::io::{BufWriter, Write};
use std::path::Path;

use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use anyhow::Result;
use colored::control::SHOULD_COLORIZE;
use colored::Colorize;
use itertools::{iterate, Itertools};
use rustc_hash::FxHashMap;
use serde::Serialize;
use serde_json::json;

use bitflags::bitflags;
use ruff::fs::relativize_path;
use ruff::linter::FixTable;
use ruff::logging::LogLevel;
use ruff::message::{Location, Message};
use ruff::registry::Rule;
use ruff::settings::types::SerializationFormat;
use ruff::{fix, notify_user};

use crate::diagnostics::Diagnostics;

bitflags! {
    #[derive(Default)]
    pub struct Flags: u32 {
        const SHOW_VIOLATIONS = 0b0000_0001;
        const SHOW_FIXES = 0b0000_0010;
    }
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
    noqa_row: usize,
}

#[derive(Serialize)]
struct ExpandedStatistics {
    count: usize,
    code: String,
    message: String,
    fixable: bool,
}

struct SerializeRuleAsCode<'a>(&'a Rule);

impl Serialize for SerializeRuleAsCode<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.noqa_code().to_string())
    }
}

impl<'a> From<&'a Rule> for SerializeRuleAsCode<'a> {
    fn from(rule: &'a Rule) -> Self {
        Self(rule)
    }
}

pub struct Printer {
    format: SerializationFormat,
    log_level: LogLevel,
    autofix_level: fix::FixMode,
    flags: Flags,
}

impl Printer {
    pub const fn new(
        format: SerializationFormat,
        log_level: LogLevel,
        autofix_level: fix::FixMode,
        flags: Flags,
    ) -> Self {
        Self {
            format,
            log_level,
            autofix_level,
            flags,
        }
    }

    pub fn write_to_user(&self, message: &str) {
        if self.log_level >= LogLevel::Default {
            notify_user!("{}", message);
        }
    }

    fn post_text<T: Write>(&self, stdout: &mut T, diagnostics: &Diagnostics) -> Result<()> {
        if self.log_level >= LogLevel::Default {
            if self.flags.contains(Flags::SHOW_VIOLATIONS) {
                let fixed = diagnostics
                    .fixed
                    .values()
                    .flat_map(std::collections::HashMap::values)
                    .sum::<usize>();
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

                if show_fix_status(self.autofix_level) {
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
            } else {
                let fixed = diagnostics
                    .fixed
                    .values()
                    .flat_map(std::collections::HashMap::values)
                    .sum::<usize>();
                if fixed > 0 {
                    let s = if fixed == 1 { "" } else { "s" };
                    if matches!(self.autofix_level, fix::FixMode::Apply) {
                        writeln!(stdout, "Fixed {fixed} error{s}.")?;
                    } else if matches!(self.autofix_level, fix::FixMode::Diff) {
                        writeln!(stdout, "Would fix {fixed} error{s}.")?;
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

        if !self.flags.contains(Flags::SHOW_VIOLATIONS) {
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
                                noqa_row: message.noqa_row,
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
                            format!("org.ruff.{}", message.kind.rule().noqa_code()),
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
                    print_message(&mut stdout, message, self.autofix_level)?;
                }
                if self.flags.contains(Flags::SHOW_FIXES) {
                    if !diagnostics.fixed.is_empty() {
                        writeln!(stdout)?;
                        print_fixed(&mut stdout, &diagnostics.fixed)?;
                        writeln!(stdout)?;
                    }
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
                        print_grouped_message(
                            &mut stdout,
                            message,
                            self.autofix_level,
                            row_length,
                            column_length,
                        )?;
                    }
                    writeln!(stdout)?;
                }

                if self.flags.contains(Flags::SHOW_FIXES) {
                    if !diagnostics.fixed.is_empty() {
                        writeln!(stdout)?;
                        print_fixed(&mut stdout, &diagnostics.fixed)?;
                        writeln!(stdout)?;
                    }
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
                        message.kind.rule().noqa_code(),
                        message.kind.body(),
                    );
                    writeln!(
                        stdout,
                        "::error title=Ruff \
                         ({}),file={},line={},col={},endLine={},endColumn={}::{}",
                        message.kind.rule().noqa_code(),
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
                                    "description": format!("({}) {}", message.kind.rule().noqa_code(), message.kind.body()),
                                    "severity": "major",
                                    "fingerprint": message.kind.rule().noqa_code().to_string(),
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
                        message.kind.rule().noqa_code(),
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
                code: rule.noqa_code().to_string(),
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
                fixable: diagnostics
                    .messages
                    .iter()
                    .find(|message| message.kind.rule() == *rule)
                    .iter()
                    .any(|message| message.kind.fixable()),
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
                let any_fixable = statistics.iter().any(|statistic| statistic.fixable);

                let fixable = format!("[{}] ", "*".cyan());
                let unfixable = "[ ] ";

                // By default, we mimic Flake8's `--statistics` format.
                for statistic in statistics {
                    writeln!(
                        stdout,
                        "{:>count_width$}\t{:<code_width$}\t{}{}",
                        statistic.count.to_string().bold(),
                        statistic.code.red().bold(),
                        if any_fixable {
                            if statistic.fixable {
                                &fixable
                            } else {
                                unfixable
                            }
                        } else {
                            ""
                        },
                        statistic.message,
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

        if self.log_level >= LogLevel::Default {
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
            if self.log_level >= LogLevel::Default {
                writeln!(stdout)?;
            }
            for message in &diagnostics.messages {
                print_message(&mut stdout, message, self.autofix_level)?;
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

struct CodeAndBody<'a>(&'a Message, fix::FixMode);

impl Display for CodeAndBody<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if show_fix_status(self.1) && self.0.kind.fixable() {
            write!(
                f,
                "{code} {autofix}{body}",
                code = self.0.kind.rule().noqa_code().to_string().red().bold(),
                autofix = format_args!("[{}] ", "*".cyan()),
                body = self.0.kind.body(),
            )
        } else {
            write!(
                f,
                "{code} {body}",
                code = self.0.kind.rule().noqa_code().to_string().red().bold(),
                body = self.0.kind.body(),
            )
        }
    }
}

/// Return `true` if the [`Printer`] should indicate that a rule is fixable.
fn show_fix_status(autofix_level: fix::FixMode) -> bool {
    // If we're in application mode, avoid indicating that a rule is fixable.
    // If the specific violation were truly fixable, it would've been fixed in
    // this pass! (We're occasionally unable to determine whether a specific
    // violation is fixable without trying to fix it, so if autofix is not
    // enabled, we may inadvertently indicate that a rule is fixable.)
    !matches!(autofix_level, fix::FixMode::Apply)
}

/// Print a single `Message` with full details.
fn print_message<T: Write>(
    stdout: &mut T,
    message: &Message,
    autofix_level: fix::FixMode,
) -> Result<()> {
    let label = format!(
        "{path}{sep}{row}{sep}{col}{sep} {code_and_body}",
        path = relativize_path(Path::new(&message.filename)).bold(),
        sep = ":".cyan(),
        row = message.location.row(),
        col = message.location.column(),
        code_and_body = CodeAndBody(message, autofix_level)
    );
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
        let label = message.kind.rule().noqa_code().to_string();
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
                    label: &label,
                    annotation_type: AnnotationType::Error,
                    range: source.range,
                }],
                // The origin (file name, line number, and column number) is already encoded
                // in the `label`.
                origin: None,
                fold: false,
            }],
            opt: FormatOptions {
                color: SHOULD_COLORIZE.should_colorize(),
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

fn print_fixed<T: Write>(stdout: &mut T, fixed: &FxHashMap<String, FixTable>) -> Result<()> {
    let total = fixed
        .values()
        .map(|table| table.values().sum::<usize>())
        .sum::<usize>();
    assert!(total > 0);
    let num_digits = num_digits(
        *fixed
            .values()
            .filter_map(|table| table.values().max())
            .max()
            .unwrap(),
    );

    let s = if total == 1 { "" } else { "s" };
    let label = format!("Fixed {total} error{s}:");
    writeln!(stdout, "{}", label.bold().green())?;

    for (filename, table) in fixed
        .iter()
        .sorted_by_key(|(filename, ..)| filename.as_str())
    {
        writeln!(
            stdout,
            "{} {}{}",
            "-".cyan(),
            relativize_path(Path::new(filename)).bold(),
            ":".cyan()
        )?;
        for (rule, count) in table.iter().sorted_by_key(|(.., count)| Reverse(*count)) {
            writeln!(
                stdout,
                "    {count:>num_digits$} × {} ({})",
                rule.noqa_code().to_string().red().bold(),
                rule.as_ref(),
            )?;
        }
    }
    Ok(())
}

/// Print a grouped `Message`, assumed to be printed in a group with others from
/// the same file.
fn print_grouped_message<T: Write>(
    stdout: &mut T,
    message: &Message,
    autofix_level: fix::FixMode,
    row_length: usize,
    column_length: usize,
) -> Result<()> {
    let label = format!(
        "  {row_padding}{row}{sep}{col}{col_padding}  {code_and_body}",
        row_padding = " ".repeat(row_length - num_digits(message.location.row())),
        row = message.location.row(),
        sep = ":".cyan(),
        col = message.location.column(),
        col_padding = " ".repeat(column_length - num_digits(message.location.column())),
        code_and_body = CodeAndBody(message, autofix_level),
    );
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
        let label = message.kind.rule().noqa_code().to_string();
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
                    label: &label,
                    annotation_type: AnnotationType::Error,
                    range: source.range,
                }],
                // The origin (file name, line number, and column number) is already encoded
                // in the `label`.
                origin: None,
                fold: false,
            }],
            opt: FormatOptions {
                color: SHOULD_COLORIZE.should_colorize(),
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
