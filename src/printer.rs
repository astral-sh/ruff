use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::Serialize;

use crate::checks::{CheckCode, CheckKind};
use crate::linter::Diagnostics;
use crate::logging::LogLevel;
use crate::tell_user;

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq, Debug)]
pub enum SerializationFormat {
    Text,
    Json,
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

                for message in &diagnostics.messages {
                    println!("{message}");
                }

                if self.log_level >= &LogLevel::Default {
                    if num_fixable > 0 {
                        println!("{num_fixable} potentially fixable with the --fix option.");
                    }
                }
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
