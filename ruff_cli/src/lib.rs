//! This library only exists to enable the Ruff internal tooling (`ruff_dev`)
//! to automatically update the `ruff --help` output in the `README.md`.
//!
//! For the actual Ruff library, see [`ruff`].
#![allow(clippy::must_use_candidate, dead_code)]

mod cli;

use clap::CommandFactory;

/// Returns the output of `ruff --help`.
pub fn help() -> String {
    cli::Cli::command().render_help().to_string()
}
