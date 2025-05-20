use std::cmp::Reverse;
use std::hash::Hash;
use std::io::Write;

use anyhow::Result;
use bitflags::bitflags;
use colored::Colorize;
use itertools::{Itertools, iterate};
use ruff_linter::codes::NoqaCode;
use serde::Serialize;

use ruff_linter::fs::relativize_path;
use ruff_linter::logging::LogLevel;
use ruff_linter::message::{
    AzureEmitter, Emitter, EmitterContext, GithubEmitter, GitlabEmitter, GroupedEmitter,
    JsonEmitter, JsonLinesEmitter, JunitEmitter, Message, PylintEmitter, RdjsonEmitter,
    SarifEmitter, TextEmitter,
};
use ruff_linter::notify_user;
use ruff_linter::settings::flags::{self};
use ruff_linter::settings::types::{OutputFormat, UnsafeFixes};

use crate::diagnostics::{Diagnostics, FixMap};

bitflags! {
    #[derive(Default, Debug, Copy, Clone)]
    pub(crate) struct Flags: u8 {
        /// Whether to show violations when emitting diagnostics.
        const SHOW_VIOLATIONS = 1 << 0;
        /// Whether to show a summary of the fixed violations when emitting diagnostics.
        const SHOW_FIX_SUMMARY = 1 << 1;
        /// Whether to show a diff of each fixed violation when emitting diagnostics.
        const SHOW_FIX_DIFF = 1 << 2;
    }
}

#[derive(Serialize)]
struct ExpandedStatistics {
    code: Option<NoqaCode>,
    name: &'static str,
    count: usize,
    fixable: bool,
}

pub(crate) struct Printer {
    format: OutputFormat,
    log_level: LogLevel,
    fix_mode: flags::FixMode,
    unsafe_fixes: UnsafeFixes,
    flags: Flags,
}

impl Printer {
    pub(crate) const fn new(
        format: OutputFormat,
        log_level: LogLevel,
        fix_mode: flags::FixMode,
        unsafe_fixes: UnsafeFixes,
        flags: Flags,
    ) -> Self {
        Self {
            format,
            log_level,
            fix_mode,
            unsafe_fixes,
            flags,
        }
    }

    pub(crate) fn write_to_user(&self, message: &str) {
        if self.log_level >= LogLevel::Default {
            notify_user!("{}", message);
        }
    }

    fn write_summary_text(&self, writer: &mut dyn Write, diagnostics: &Diagnostics) -> Result<()> {
        if self.log_level >= LogLevel::Default {
            let fixables = FixableStatistics::try_from(diagnostics, self.unsafe_fixes);

            let fixed = diagnostics
                .fixed
                .values()
                .flat_map(std::collections::HashMap::values)
                .sum::<usize>();

            if self.flags.intersects(Flags::SHOW_VIOLATIONS) {
                let remaining = diagnostics.messages.len();
                let total = fixed + remaining;
                if fixed > 0 {
                    let s = if total == 1 { "" } else { "s" };
                    writeln!(
                        writer,
                        "Found {total} error{s} ({fixed} fixed, {remaining} remaining)."
                    )?;
                } else if remaining > 0 {
                    let s = if remaining == 1 { "" } else { "s" };
                    writeln!(writer, "Found {remaining} error{s}.")?;
                } else if remaining == 0 {
                    writeln!(writer, "All checks passed!")?;
                }

                if let Some(fixables) = fixables {
                    let fix_prefix = format!("[{}]", "*".cyan());

                    if self.unsafe_fixes.is_hint() {
                        if fixables.applicable > 0 && fixables.inapplicable_unsafe > 0 {
                            let es = if fixables.inapplicable_unsafe == 1 {
                                ""
                            } else {
                                "es"
                            };
                            writeln!(
                                writer,
                                "{fix_prefix} {} fixable with the `--fix` option ({} hidden fix{es} can be enabled with the `--unsafe-fixes` option).",
                                fixables.applicable, fixables.inapplicable_unsafe
                            )?;
                        } else if fixables.applicable > 0 {
                            // Only applicable fixes
                            writeln!(
                                writer,
                                "{fix_prefix} {} fixable with the `--fix` option.",
                                fixables.applicable,
                            )?;
                        } else {
                            // Only inapplicable fixes
                            let es = if fixables.inapplicable_unsafe == 1 {
                                ""
                            } else {
                                "es"
                            };
                            writeln!(
                                writer,
                                "No fixes available ({} hidden fix{es} can be enabled with the `--unsafe-fixes` option).",
                                fixables.inapplicable_unsafe
                            )?;
                        }
                    } else {
                        if fixables.applicable > 0 {
                            writeln!(
                                writer,
                                "{fix_prefix} {} fixable with the --fix option.",
                                fixables.applicable
                            )?;
                        }
                    }
                }
            } else {
                // Check if there are unapplied fixes
                let unapplied = {
                    if let Some(fixables) = fixables {
                        fixables.inapplicable_unsafe
                    } else {
                        0
                    }
                };

                if unapplied > 0 {
                    let es = if unapplied == 1 { "" } else { "es" };
                    if fixed > 0 {
                        let s = if fixed == 1 { "" } else { "s" };
                        if self.fix_mode.is_apply() {
                            writeln!(
                                writer,
                                "Fixed {fixed} error{s} ({unapplied} additional fix{es} available with `--unsafe-fixes`)."
                            )?;
                        } else {
                            writeln!(
                                writer,
                                "Would fix {fixed} error{s} ({unapplied} additional fix{es} available with `--unsafe-fixes`)."
                            )?;
                        }
                    } else {
                        if self.fix_mode.is_apply() {
                            writeln!(
                                writer,
                                "No errors fixed ({unapplied} fix{es} available with `--unsafe-fixes`)."
                            )?;
                        } else {
                            writeln!(
                                writer,
                                "No errors would be fixed ({unapplied} fix{es} available with `--unsafe-fixes`)."
                            )?;
                        }
                    }
                } else {
                    if fixed > 0 {
                        let s = if fixed == 1 { "" } else { "s" };
                        if self.fix_mode.is_apply() {
                            writeln!(writer, "Fixed {fixed} error{s}.")?;
                        } else {
                            writeln!(writer, "Would fix {fixed} error{s}.")?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn write_once(
        &self,
        diagnostics: &Diagnostics,
        writer: &mut dyn Write,
    ) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        if !self.flags.intersects(Flags::SHOW_VIOLATIONS) {
            if matches!(
                self.format,
                OutputFormat::Full | OutputFormat::Concise | OutputFormat::Grouped
            ) {
                if self.flags.intersects(Flags::SHOW_FIX_SUMMARY) {
                    if !diagnostics.fixed.is_empty() {
                        writeln!(writer)?;
                        print_fix_summary(writer, &diagnostics.fixed)?;
                        writeln!(writer)?;
                    }
                }
                self.write_summary_text(writer, diagnostics)?;
            }
            return Ok(());
        }

        let context = EmitterContext::new(&diagnostics.notebook_indexes);
        let fixables = FixableStatistics::try_from(diagnostics, self.unsafe_fixes);

        match self.format {
            OutputFormat::Json => {
                JsonEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::Rdjson => {
                RdjsonEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::JsonLines => {
                JsonLinesEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::Junit => {
                JunitEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::Concise | OutputFormat::Full => {
                TextEmitter::default()
                    .with_show_fix_status(show_fix_status(self.fix_mode, fixables.as_ref()))
                    .with_show_fix_diff(self.flags.intersects(Flags::SHOW_FIX_DIFF))
                    .with_show_source(self.format == OutputFormat::Full)
                    .with_unsafe_fixes(self.unsafe_fixes)
                    .emit(writer, &diagnostics.messages, &context)?;

                if self.flags.intersects(Flags::SHOW_FIX_SUMMARY) {
                    if !diagnostics.fixed.is_empty() {
                        writeln!(writer)?;
                        print_fix_summary(writer, &diagnostics.fixed)?;
                        writeln!(writer)?;
                    }
                }

                self.write_summary_text(writer, diagnostics)?;
            }
            OutputFormat::Grouped => {
                GroupedEmitter::default()
                    .with_show_fix_status(show_fix_status(self.fix_mode, fixables.as_ref()))
                    .with_unsafe_fixes(self.unsafe_fixes)
                    .emit(writer, &diagnostics.messages, &context)?;

                if self.flags.intersects(Flags::SHOW_FIX_SUMMARY) {
                    if !diagnostics.fixed.is_empty() {
                        writeln!(writer)?;
                        print_fix_summary(writer, &diagnostics.fixed)?;
                        writeln!(writer)?;
                    }
                }
                self.write_summary_text(writer, diagnostics)?;
            }
            OutputFormat::Github => {
                GithubEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::Gitlab => {
                GitlabEmitter::default().emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::Pylint => {
                PylintEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::Azure => {
                AzureEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
            OutputFormat::Sarif => {
                SarifEmitter.emit(writer, &diagnostics.messages, &context)?;
            }
        }

        writer.flush()?;

        Ok(())
    }

    pub(crate) fn write_statistics(
        &self,
        diagnostics: &Diagnostics,
        writer: &mut dyn Write,
    ) -> Result<()> {
        let statistics: Vec<ExpandedStatistics> = diagnostics
            .messages
            .iter()
            .map(|message| (message.to_noqa_code(), message))
            .sorted_by_key(|(code, message)| (*code, message.fixable()))
            .fold(
                vec![],
                |mut acc: Vec<((Option<NoqaCode>, &Message), usize)>, (code, message)| {
                    if let Some(((prev_code, _prev_message), count)) = acc.last_mut() {
                        if *prev_code == code {
                            *count += 1;
                            return acc;
                        }
                    }
                    acc.push(((code, message), 1));
                    acc
                },
            )
            .iter()
            .map(|&((code, message), count)| ExpandedStatistics {
                code,
                name: message.name(),
                count,
                fixable: if let Some(fix) = message.fix() {
                    fix.applies(self.unsafe_fixes.required_applicability())
                } else {
                    false
                },
            })
            .sorted_by_key(|statistic| Reverse(statistic.count))
            .collect();

        if statistics.is_empty() {
            return Ok(());
        }

        match self.format {
            OutputFormat::Full | OutputFormat::Concise => {
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
                    .map(|statistic| {
                        statistic
                            .code
                            .map_or_else(String::new, |rule| rule.to_string())
                            .len()
                    })
                    .max()
                    .unwrap();
                let any_fixable = statistics.iter().any(|statistic| statistic.fixable);

                let fixable = format!("[{}] ", "*".cyan());
                let unfixable = "[ ] ";

                // By default, we mimic Flake8's `--statistics` format.
                for statistic in statistics {
                    writeln!(
                        writer,
                        "{:>count_width$}\t{:<code_width$}\t{}{}",
                        statistic.count.to_string().bold(),
                        statistic
                            .code
                            .map_or_else(String::new, |rule| rule.to_string())
                            .red()
                            .bold(),
                        if any_fixable {
                            if statistic.fixable {
                                &fixable
                            } else {
                                unfixable
                            }
                        } else {
                            ""
                        },
                        statistic.name,
                    )?;
                }

                self.write_summary_text(writer, diagnostics)?;
                return Ok(());
            }
            OutputFormat::Json => {
                writeln!(writer, "{}", serde_json::to_string_pretty(&statistics)?)?;
            }
            _ => {
                anyhow::bail!(
                    "Unsupported serialization format for statistics: {:?}",
                    self.format
                )
            }
        }

        writer.flush()?;

        Ok(())
    }

    pub(crate) fn write_continuously(
        &self,
        writer: &mut dyn Write,
        diagnostics: &Diagnostics,
        preview: bool,
    ) -> Result<()> {
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

        let fixables = FixableStatistics::try_from(diagnostics, self.unsafe_fixes);

        if !diagnostics.messages.is_empty() {
            if self.log_level >= LogLevel::Default {
                writeln!(writer)?;
            }

            let context = EmitterContext::new(&diagnostics.notebook_indexes);
            TextEmitter::default()
                .with_show_fix_status(show_fix_status(self.fix_mode, fixables.as_ref()))
                .with_show_source(preview)
                .with_unsafe_fixes(self.unsafe_fixes)
                .emit(writer, &diagnostics.messages, &context)?;
        }
        writer.flush()?;

        Ok(())
    }

    pub(crate) fn clear_screen() -> Result<()> {
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

/// Return `true` if the [`Printer`] should indicate that a rule is fixable.
fn show_fix_status(fix_mode: flags::FixMode, fixables: Option<&FixableStatistics>) -> bool {
    // If we're in application mode, avoid indicating that a rule is fixable.
    // If the specific violation were truly fixable, it would've been fixed in
    // this pass! (We're occasionally unable to determine whether a specific
    // violation is fixable without trying to fix it, so if fix is not
    // enabled, we may inadvertently indicate that a rule is fixable.)
    (!fix_mode.is_apply()) && fixables.is_some_and(FixableStatistics::any_applicable_fixes)
}

fn print_fix_summary(writer: &mut dyn Write, fixed: &FixMap) -> Result<()> {
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
    writeln!(writer, "{}", label.bold().green())?;

    for (filename, table) in fixed
        .iter()
        .sorted_by_key(|(filename, ..)| filename.as_str())
    {
        writeln!(
            writer,
            "{} {}{}",
            "-".cyan(),
            relativize_path(filename).bold(),
            ":".cyan()
        )?;
        for (rule, count) in table.iter().sorted_by_key(|(.., count)| Reverse(*count)) {
            writeln!(
                writer,
                "    {count:>num_digits$} × {} ({})",
                rule.noqa_code().to_string().red().bold(),
                rule.as_ref(),
            )?;
        }
    }
    Ok(())
}

/// Statistics for [applicable][ruff_diagnostics::Applicability] fixes.
#[derive(Debug)]
struct FixableStatistics {
    applicable: u32,
    inapplicable_unsafe: u32,
}

impl FixableStatistics {
    fn try_from(diagnostics: &Diagnostics, unsafe_fixes: UnsafeFixes) -> Option<Self> {
        let mut applicable = 0;
        let mut inapplicable_unsafe = 0;

        for message in &diagnostics.messages {
            if let Some(fix) = message.fix() {
                if fix.applies(unsafe_fixes.required_applicability()) {
                    applicable += 1;
                } else {
                    // Do not include inapplicable fixes at other levels that do not provide an opt-in
                    if fix.applicability().is_unsafe() {
                        inapplicable_unsafe += 1;
                    }
                }
            }
        }

        if applicable == 0 && inapplicable_unsafe == 0 {
            None
        } else {
            Some(Self {
                applicable,
                inapplicable_unsafe,
            })
        }
    }

    fn any_applicable_fixes(&self) -> bool {
        self.applicable > 0
    }
}
