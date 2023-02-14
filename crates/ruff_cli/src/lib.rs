//! This library only exists to enable the Ruff internal tooling (`ruff_dev`)
//! to automatically update the `ruff help` output in the `README.md`.
//!
//! For the actual Ruff library, see [`ruff`].

mod args;

use clap::CommandFactory;

/// Returns the output of `ruff help`.
pub fn command_help() -> String {
    args::Args::command().render_help().to_string()
}

/// Returns the output of `ruff help check`.
pub fn subcommand_help() -> String {
    let mut cmd = args::Args::command();

    // The build call is necessary for the help output to contain `Usage: ruff
    // check` instead of `Usage: check` see https://github.com/clap-rs/clap/issues/4685
    cmd.build();

    cmd.find_subcommand_mut("check")
        .expect("`check` subcommand not found")
        .render_help()
        .to_string()
}
