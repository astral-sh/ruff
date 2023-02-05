#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use ruff::settings::options::Options;
use schemars::schema_for;

use crate::ROOT_DIR;

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated table to stdout (rather than to `ruff.schema.json`).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub fn main(args: &Args) -> Result<()> {
    let schema = schema_for!(Options);
    let schema_string = serde_json::to_string_pretty(&schema).unwrap();

    if args.dry_run {
        println!("{schema_string}");
    } else {
        let file = PathBuf::from(ROOT_DIR).join("ruff.schema.json");
        fs::write(file, schema_string.as_bytes())?;
    }
    Ok(())
}
