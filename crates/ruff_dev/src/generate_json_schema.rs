#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use crate::generate_all::REGENERATE_ALL_COMMAND;
use anyhow::{bail, Result};
use pretty_assertions::StrComparison;
use ruff::settings::options::Options;
use schemars::schema_for;

use crate::ROOT_DIR;

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated table to stdout (rather than to `ruff.schema.json`).
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Don't write to the file, check if the file is up-to-date and error if not
    #[arg(long)]
    pub(crate) check: bool,
}

pub fn main(args: &Args) -> Result<()> {
    let schema = schema_for!(Options);
    let schema_string = serde_json::to_string_pretty(&schema).unwrap();
    let filename = "ruff.schema.json";
    let schema_path = PathBuf::from(ROOT_DIR).join(filename);

    if args.dry_run {
        println!("{schema_string}");
    } else if args.check {
        let current = fs::read_to_string(schema_path)?;
        if current == schema_string {
            println!("up-to-date: {filename}");
        } else {
            let comparison = StrComparison::new(&current, &schema_string);
            bail!("{filename} changed, please run `{REGENERATE_ALL_COMMAND}`:\n{comparison}");
        }
    } else {
        let file = schema_path;
        fs::write(file, schema_string.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::{main, Args};

    #[test]
    fn test_generate_json_schema() {
        main(&Args {
            dry_run: false,
            check: true,
        })
        .unwrap();
    }
}
