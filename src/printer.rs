use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::Serialize;

use crate::checks::{CheckCode, CheckKind};
use crate::logging::LogLevel;
use crate::message::Message;
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
    fixed: bool,
    location: Location,
    end_location: Location,
    filename: &'a String,
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

    pub fn write_once(&self, messages: &[Message]) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        let (fixed, outstanding): (Vec<&Message>, Vec<&Message>) =
            messages.iter().partition(|message| message.fixed);
        let num_fixable = outstanding
            .iter()
            .filter(|message| message.kind.fixable())
            .count();

        match self.format {
            SerializationFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &messages
                            .iter()
                            .map(|message| ExpandedMessage {
                                kind: &message.kind,
                                code: message.kind.code(),
                                message: message.kind.body(),
                                fixed: message.fixed,
                                location: message.location,
                                end_location: message.end_location,
                                filename: &message.filename,
                            })
                            .collect::<Vec<_>>()
                    )?
                )
            }
            SerializationFormat::Text => {
                if self.log_level >= &LogLevel::Default {
                    if !fixed.is_empty() {
                        println!(
                            "Found {} error(s) ({} fixed).",
                            outstanding.len(),
                            fixed.len()
                        )
                    } else if !outstanding.is_empty() {
                        println!("Found {} error(s).", outstanding.len())
                    }
                }

                for message in outstanding {
                    println!("{}", message)
                }

                if self.log_level >= &LogLevel::Default {
                    if num_fixable > 0 {
                        println!("{num_fixable} potentially fixable with the --fix option.")
                    }
                }
            }
        }

        Ok(())
    }

    pub fn write_continuously(&self, messages: &[Message]) -> Result<()> {
        if matches!(self.log_level, LogLevel::Silent) {
            return Ok(());
        }

        if self.log_level >= &LogLevel::Default {
            tell_user!(
                "Found {} error(s). Watching for file changes.",
                messages.len(),
            );
        }

        if !messages.is_empty() {
            if self.log_level >= &LogLevel::Default {
                println!();
            }
            for message in messages {
                println!("{}", message)
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
