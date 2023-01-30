//! This library only exists to enable the Ruff internal tooling (`ruff_dev`)
//! to automatically update the `ruff help` output in the `README.md`.
//!
//! For the actual Ruff library, see [`ruff`].
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate, dead_code)]

mod args;

use clap::CommandFactory;

/// Returns the output of `ruff help`.
pub fn command_help() -> String {
    args::Args::command().render_help().to_string()
}

/// Returns the output of `ruff help check`.
pub fn subcommand_help() -> String {
    let output = args::Args::command()
        .find_subcommand_mut("check")
        .expect("`check` subcommand not found")
        .render_help()
        .to_string();

    // Replace the header, to fix Clap's omission of "ruff" on the "Usage: check" line.
    let header =
        "Run Ruff on the given files or directories (default)\n\nUsage: check [OPTIONS] [FILES]...";
    let replacement =
        "Run Ruff on the given files or directories\n\nUsage: ruff check [OPTIONS] [FILES]...";
    let output = output
        .strip_prefix(header)
        .expect("`output` does not start expected header");
    format!("{replacement}{output}")
}
