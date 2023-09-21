//! Utility to generate Ruff's `pyproject.toml` section from a Flake8 INI file.

mod black;
mod converter;
mod external_config;
mod isort;
mod parser;
mod pep621;
mod plugin;
mod pyproject;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use configparser::ini::Ini;

use crate::converter::convert;
use crate::external_config::ExternalConfig;
use crate::plugin::Plugin;
use crate::pyproject::parse;
use ruff_linter::logging::{set_up_logging, LogLevel};

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
    plugin: Option<Vec<Plugin>>,
}

fn main() -> Result<()> {
    set_up_logging(&LogLevel::Default)?;

    let args = Args::parse();

    // Read the INI file.
    let mut ini = Ini::new_cs();
    ini.set_multiline(true);
    let config = ini.load(args.file).map_err(|msg| anyhow::anyhow!(msg))?;

    // Read the pyproject.toml file.
    let pyproject = args.pyproject.map(parse).transpose()?;
    let external_config = pyproject
        .as_ref()
        .and_then(|pyproject| pyproject.tool.as_ref())
        .map(|tool| ExternalConfig {
            black: tool.black.as_ref(),
            isort: tool.isort.as_ref(),
            ..Default::default()
        })
        .unwrap_or_default();
    let external_config = ExternalConfig {
        project: pyproject
            .as_ref()
            .and_then(|pyproject| pyproject.project.as_ref()),
        ..external_config
    };

    // Create Ruff's pyproject.toml section.
    let pyproject = convert(&config, &external_config, args.plugin);

    #[allow(clippy::print_stdout)]
    {
        println!("{}", toml::to_string_pretty(&pyproject)?);
    }

    Ok(())
}
