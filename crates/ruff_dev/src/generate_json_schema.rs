#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use pretty_assertions::StrComparison;
use schemars::schema_for;

use crate::generate_all::{Mode, REGENERATE_ALL_COMMAND};
use crate::ROOT_DIR;
use ruff_workspace::options::Options;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Write the generated table to stdout (rather than to `ruff.schema.json`).
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let schema = schema_for!(Options);
    let schema_string = serde_json::to_string_pretty(&schema).unwrap();
    let filename = "ruff.schema.json";
    let schema_path = PathBuf::from(ROOT_DIR).join(filename);

    match args.mode {
        Mode::DryRun => {
            println!("{schema_string}");
        }
        Mode::Check => {
            let current = fs::read_to_string(schema_path)?;
            if current == schema_string {
                println!("Up-to-date: {filename}");
            } else {
                let comparison = StrComparison::new(&current, &schema_string);
                bail!("{filename} changed, please run `{REGENERATE_ALL_COMMAND}`:\n{comparison}");
            }
        }
        Mode::Write => {
            let current = fs::read_to_string(&schema_path)?;
            if current == schema_string {
                println!("Up-to-date: {filename}");
            } else {
                println!("Updating: {filename}");
                fs::write(schema_path, schema_string.as_bytes())?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::env;

    use crate::generate_all::Mode;

    use super::{main, Args};

    #[test]
    fn test_generate_json_schema() -> Result<()> {
        let mode = if env::var("RUFF_UPDATE_SCHEMA").as_deref() == Ok("1") {
            Mode::Write
        } else {
            Mode::Check
        };
        main(&Args { mode })
    }
}
