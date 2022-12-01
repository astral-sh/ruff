//! Utility to generate Ruff's pyproject.toml section from a Flake8 INI file.
#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use configparser::ini::Ini;
use flake8_to_ruff::converter;
use flake8_to_ruff::plugin::Plugin;

#[derive(Parser)]
#[command(
    about = "Convert existing Flake8 configuration to Ruff.",
    long_about = None
)]
struct Cli {
    /// Path to the Flake8 configuration file (e.g., 'setup.cfg', 'tox.ini', or
    /// '.flake8').
    #[arg(required = true)]
    file: PathBuf,
    /// List of plugins to enable.
    #[arg(long, value_delimiter = ',')]
    plugin: Option<Vec<Plugin>>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Read the INI file.
    let mut ini = Ini::new_cs();
    ini.set_multiline(true);
    let config = ini.load(cli.file).map_err(|msg| anyhow::anyhow!(msg))?;

    // Extract the Flake8 section.
    let flake8 = config
        .get("flake8")
        .expect("Unable to find flake8 section in INI file");

    // Create the pyproject.toml.
    let pyproject = converter::convert(flake8, cli.plugin)?;
    println!("{}", toml::to_string_pretty(&pyproject)?);

    Ok(())
}
