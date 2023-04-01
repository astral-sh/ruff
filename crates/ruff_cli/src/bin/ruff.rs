use clap::{Parser, Subcommand};
use std::process::ExitCode;

use colored::Colorize;

use ruff_cli::args::{Args, Command};
use ruff_cli::{run, ExitStatus};

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub fn main() -> ExitCode {
    let mut args: Vec<_> = wild::args().collect();

    // Clap doesn't support default subcommands but we want to run `check` by
    // default for convenience and backwards-compatibility, so we just
    // preprocess the arguments accordingly before passing them to Clap.
    if let Some(arg) = args.get(1) {
        if !Command::has_subcommand(rewrite_legacy_subcommand(arg))
            && arg != "-h"
            && arg != "--help"
            && arg != "-V"
            && arg != "--version"
            && arg != "help"
        {
            args.insert(1, "check".into());
        }
    }

    let args = Args::parse_from(args);

    match run(args) {
        Ok(code) => code.into(),
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{}{} {err:?}", "error".red().bold(), ":".bold());
            }
            ExitStatus::Error.into()
        }
    }
}

fn rewrite_legacy_subcommand(cmd: &str) -> &str {
    match cmd {
        "--explain" => "rule",
        "--clean" => "clean",
        "--generate-shell-completion" => "generate-shell-completion",
        cmd => cmd,
    }
}
