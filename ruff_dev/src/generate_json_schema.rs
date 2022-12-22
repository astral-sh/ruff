use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use ruff::settings::options::Options;
use schemars::schema_for;

#[derive(Args)]
pub struct Cli {
    /// Write the generated table to stdout (rather than to `ruff.schema.json`).
    #[arg(long)]
    dry_run: bool,
}

pub fn main(cli: &Cli) -> Result<()> {
    let schema = schema_for!(Options);
    let schema_string = serde_json::to_string_pretty(&schema).unwrap();

    if cli.dry_run {
        println!("{schema_string}");
    } else {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to find root directory")
            .join("ruff.schema.json");
        fs::write(file, schema_string.as_bytes())?;
    }
    Ok(())
}
