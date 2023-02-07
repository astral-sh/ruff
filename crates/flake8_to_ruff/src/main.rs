//! Utility to generate Ruff's `pyproject.toml` section from a Flake8 INI file.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use configparser::ini::Ini;
use ruff::flake8_to_ruff::{self, ExternalConfig};

#[derive(Parser)]
#[command(
    about = "Convert existing Flake8 configuration to Ruff.",
    long_about = None
)]
struct Args {
    /// Path to the Flake8 configuration file (e.g., `setup.cfg`, `tox.ini`, or
    /// `.flake8`).
    #[arg(required = true)]
    file: PathBuf,
    /// Optional path to a `pyproject.toml` file, used to ensure compatibility
    /// with Black.
    #[arg(long)]
    pyproject: Option<PathBuf>,
    /// List of plugins to enable.
    #[arg(long, value_delimiter = ',')]
    plugin: Option<Vec<flake8_to_ruff::Plugin>>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Read the INI file.
    let mut ini = Ini::new_cs();
    ini.set_multiline(true);
    let config = ini.load(args.file).map_err(|msg| anyhow::anyhow!(msg))?;

    // Read the pyproject.toml file.
    let pyproject = args.pyproject.map(flake8_to_ruff::parse).transpose()?;
    let external_config = pyproject
        .as_ref()
        .and_then(|pyproject| pyproject.tool.as_ref())
        .map(|tool| ExternalConfig {
            black: tool.black.as_ref(),
            isort: tool.isort.as_ref(),
        })
        .unwrap_or_default();

    // Create Ruff's pyproject.toml section.
    let pyproject = flake8_to_ruff::convert(&config, &external_config, args.plugin)?;

    #[allow(clippy::print_stdout)]
    {
        println!("{}", toml::to_string_pretty(&pyproject)?);
    }

    Ok(())
}
