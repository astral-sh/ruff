#![allow(clippy::must_use_candidate, dead_code)]

mod cli;

use clap::CommandFactory;

/// Returns the output of `ruff --help`.
pub fn help() -> String {
    cli::Cli::command().render_help().to_string()
}
