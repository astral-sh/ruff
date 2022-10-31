//! Utility to generate Ruff's pyproject.toml section from a Flake8 INI file.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use configparser::ini::Ini;

use flake8_to_ruff::converter;

#[derive(Parser)]
#[command(
    about = "Convert existing Flake8 configuration to Ruff.",
    long_about = None
)]
struct Cli {
    /// Path to the Flake8 configuration file (e.g., 'setup.cfg', 'tox.ini', or '.flake8').
    #[arg(required = true)]
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Read the INI file.
    let mut ini = Ini::new_cs();
    ini.set_multiline(true);
    let config = ini.load(cli.file).map_err(|msg| anyhow::anyhow!(msg))?;

    // Create the pyproject.toml.
    let pyproject = converter::convert(config)?;
    println!("{}", toml::to_string_pretty(&pyproject)?);

    Ok(())
}
