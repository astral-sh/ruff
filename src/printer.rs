use colored::Colorize;
use std::io::Write;

use anyhow::Result;
use clap::ValueEnum;

use crate::message::Message;
use crate::tell_user;

#[derive(Clone, ValueEnum, PartialEq, Eq, Debug)]
pub enum SerializationFormat {
    Text,
    Json,
}

pub struct Printer<W> {
    pub writer: W,
    format: SerializationFormat,
}

impl<W: Write> Printer<W> {
    pub fn new(writer: W, format: SerializationFormat) -> Self {
        Self { writer, format }
    }

    pub fn write_once(&mut self, messages: &Vec<Message>) -> Result<()> {
        let (fixed, outstanding): (Vec<&Message>, Vec<&Message>) =
            messages.iter().partition(|message| message.fixed);
        let num_fixable = outstanding
            .iter()
            .filter(|message| message.kind.fixable())
            .count();

        match self.format {
            SerializationFormat::Json => {
                writeln!(self.writer, "{}", serde_json::to_string_pretty(&messages)?)?
            }
            SerializationFormat::Text => {
                if !fixed.is_empty() {
                    writeln!(
                        self.writer,
                        "Found {} error(s) ({} fixed).",
                        outstanding.len(),
                        fixed.len()
                    )?
                } else {
                    writeln!(self.writer, "Found {} error(s).", outstanding.len())?
                }

                for message in outstanding {
                    writeln!(self.writer, "{}", message)?
                }

                if num_fixable > 0 {
                    writeln!(
                        self.writer,
                        "{num_fixable} potentially fixable with the --fix option."
                    )?
                }
            }
        }

        Ok(())
    }

    pub fn write_continuously(&mut self, messages: Vec<Message>) -> Result<()> {
        tell_user!(
            self.writer,
            "Found {} error(s). Watching for file changes.",
            messages.len(),
        );

        if !messages.is_empty() {
            writeln!(self.writer, "\n")?;
            for message in messages {
                writeln!(self.writer, "{}", message)?
            }
        }

        Ok(())
    }
}
