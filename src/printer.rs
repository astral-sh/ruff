use std::path::Path;

use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::Serialize;

use crate::checks::{CheckCode, CheckKind};
use crate::fs::relativize_path;
use crate::linter::Diagnostics;
use crate::logging::LogLevel;
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

    fn post_test(&self, num_fixable: usize) {
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
                    println!("{message}");
                }

                self.post_test(num_fixable);
            }
            SerializationFormat::Grouped => {
                self.pre_text(diagnostics);

                let mut filename = "".to_string();

                for message in &diagnostics.messages {
                    if filename != message.filename {
                        filename = message.filename.clone();
                        println!(
                            "\n{}:",
                            relativize_path(Path::new(&message.filename))
                                .bold()
                                .underline()
                        );
                    }

                    println!(
                        "    {}{}{} {}  {}",
                        message.location.row(),
                        ":".cyan(),
                        message.location.column(),
                        message.kind.code().as_ref().red().bold(),
                        message.kind.body(),
                    );
                }

                println!(""); // Add a newline after the last message
                self.post_test(num_fixable);
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
                println!("{message}");
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
