//! This library only exists to enable the Ruff internal tooling (`ruff_dev`)
//! to automatically update the `ruff check --help` output in the `README.md`.
//!
//! For the actual Ruff library, see [`ruff`].
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate, dead_code)]

mod args;

use clap::CommandFactory;

/// Returns the output of `ruff check --help`.
///
/// Panics if the `check` subcommand is not found.
pub fn help() -> String {
    args::Args::command()
        .find_subcommand_mut("check")
        .expect("`check` subcommand not found")
        .render_help()
        .to_string()
}
