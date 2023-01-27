//! This library only exists to enable the Ruff internal tooling (`ruff_dev`)
//! to automatically update the `ruff --help` output in the `README.md`.
//!
//! For the actual Ruff library, see [`ruff`].
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate, dead_code)]

mod args;

use clap::CommandFactory;

/// Returns the output of `ruff --help`.
pub fn help() -> String {
    args::Args::command().render_help().to_string()
}
