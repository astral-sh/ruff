use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;
use rustpython_parser::ast::Location;
use serde::Serialize;

use crate::checks::{CheckCode, CheckKind};
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

pub struct Printer {
    format: SerializationFormat,
    verbose: bool,
}

impl Printer {
    pub fn new(format: SerializationFormat, verbose: bool) -> Self {
        Self { format, verbose }
    }

    pub fn write_once(&mut self, messages: &[Message]) -> Result<()> {
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
                            .map(|m| ExpandedMessage {
                                kind: &m.kind,
                                code: m.kind.code(),
                                message: m.kind.body(),
                                fixed: m.fixed,
                                location: m.location,
                                end_location: m.end_location,
                                filename: &m.filename,
                            })
                            .collect::<Vec<_>>()
                    )?
                )
            }
            SerializationFormat::Text => {
                if !fixed.is_empty() {
                    println!(
                        "Found {} error(s) ({} fixed).",
                        outstanding.len(),
                        fixed.len()
                    )
                } else if !outstanding.is_empty() || self.verbose {
                    println!("Found {} error(s).", outstanding.len())
                }

                for message in outstanding {
                    println!("{}", message)
                }

                if num_fixable > 0 {
                    println!("{num_fixable} potentially fixable with the --fix option.")
                }
            }
        }

        Ok(())
    }

    pub fn write_continuously(&mut self, messages: &[Message]) -> Result<()> {
        tell_user!(
            "Found {} error(s). Watching for file changes.",
            messages.len(),
        );

        if !messages.is_empty() {
            println!();
            for message in messages {
                println!("{}", message)
            }
        }

        Ok(())
    }

    pub fn clear_screen(&mut self) -> Result<()> {
        clearscreen::clear()?;
        Ok(())
    }
}
