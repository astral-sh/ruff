use std::process::ExitCode;

use clap::{Parser, Subcommand};
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
    let args = wild::args_os();
    let mut args =
        argfile::expand_args_from(args, argfile::parse_fromfile, argfile::PREFIX).unwrap();

    // Clap doesn't support default subcommands but we want to run `check` by
    // default for convenience and backwards-compatibility, so we just
    // preprocess the arguments accordingly before passing them to Clap.
    if let Some(arg) = args.get(1) {
        if arg
            .to_str()
            .is_some_and(|arg| !Command::has_subcommand(rewrite_legacy_subcommand(arg)))
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
                // This communicates that this isn't a linter error but ruff itself hard-errored for
                // some reason (e.g. failed to resolve the configuration)
                eprintln!("{}", "ruff failed".red().bold());
                // Currently we generally only see one error, but e.g. with io errors when resolving
                // the configuration it is help to chain errors ("resolving configuration failed" ->
                // "failed to read file: subdir/pyproject.toml")
                for cause in err.chain() {
                    eprintln!("  {} {cause}", "Cause:".bold());
                }
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
