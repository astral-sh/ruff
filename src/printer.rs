use colored::Colorize;

use anyhow::Result;
use clap::ValueEnum;

use crate::message::Message;
use crate::tell_user;

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq, Debug)]
pub enum SerializationFormat {
    Text,
    Json,
}

pub struct Printer {
    format: SerializationFormat,
}

impl Printer {
    pub fn new(format: SerializationFormat) -> Self {
        Self { format }
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
                println!("{}", serde_json::to_string_pretty(&messages)?)
            }
            SerializationFormat::Text => {
                if !fixed.is_empty() {
                    println!(
                        "Found {} error(s) ({} fixed).",
                        outstanding.len(),
                        fixed.len()
                    )
                } else {
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
