use std::process::ExitCode;

use clap::{Parser, Subcommand};
use colored::Colorize;
use log::error;
use std::io::Write;

use ruff::args::{Args, Command};
use ruff::{run, ExitStatus};
use ruff_linter::logging::{set_up_logging, LogLevel};

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    not(target_os = "aix"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub fn main() -> ExitCode {
    // Enabled ANSI colors on Windows 10.
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    // support FORCE_COLOR env var
    if let Some(force_color) = std::env::var_os("FORCE_COLOR") {
        if force_color.len() > 0 {
            colored::control::set_override(true);
        }
    }

    let args = wild::args_os();
    let args = argfile::expand_args_from(args, argfile::parse_fromfile, argfile::PREFIX).unwrap();

    // We can't use `warn_user` here because logging isn't set up at this point
    // and we also don't know if the user runs ruff with quiet.
    // Keep the message and pass it to `run` that is responsible for emitting the warning.
    let deprecated_alias_error = match args.get(1).and_then(|arg| arg.to_str()) {
        // Deprecated aliases that are handled by clap
        Some("--explain") => {
            Some("`ruff --explain <RULE>` has been removed. Use `ruff rule <RULE>` instead.")
        }
        Some("--clean") => {
            Some("`ruff --clean` has been removed. Use `ruff clean` instead.")
        }
        Some("--generate-shell-completion") => {
            Some("`ruff --generate-shell-completion <SHELL>` has been removed. Use `ruff generate-shell-completion <SHELL>` instead.")
        }
        // Deprecated `ruff` alias to `ruff check`
        // Clap doesn't support default subcommands but we want to run `check` by
        // default for convenience and backwards-compatibility, so we just
        // preprocess the arguments accordingly before passing them to Clap.
        Some(arg) if !Command::has_subcommand(arg)
            && arg != "-h"
            && arg != "--help"
            && arg != "-V"
            && arg != "--version"
            && arg != "help" => {
            {
                Some("`ruff <path>` has been removed. Use `ruff check <path>` instead.")
            }
        },
        _ => None
    };

    if let Some(error) = deprecated_alias_error {
        #[allow(clippy::print_stderr)]
        if set_up_logging(LogLevel::Default).is_ok() {
            error!("{}", error);
        } else {
            eprintln!("{}", error.red().bold());
        }
        return ExitCode::FAILURE;
    }

    let args = Args::parse_from(args);

    match run(args) {
        Ok(code) => code.into(),
        Err(err) => {
            {
                // Exit "gracefully" on broken pipe errors.
                //
                // See: https://github.com/BurntSushi/ripgrep/blob/bf63fe8f258afc09bae6caa48f0ae35eaf115005/crates/core/main.rs#L47C1-L61C14
                for cause in err.chain() {
                    if let Some(ioerr) = cause.downcast_ref::<std::io::Error>() {
                        if ioerr.kind() == std::io::ErrorKind::BrokenPipe {
                            return ExitCode::from(0);
                        }
                    }
                }

                // Use `writeln` instead of `eprintln` to avoid panicking when the stderr pipe is broken.
                let mut stderr = std::io::stderr().lock();

                // This communicates that this isn't a linter error but ruff itself hard-errored for
                // some reason (e.g. failed to resolve the configuration)
                writeln!(stderr, "{}", "ruff failed".red().bold()).ok();
                // Currently we generally only see one error, but e.g. with io errors when resolving
                // the configuration it is help to chain errors ("resolving configuration failed" ->
                // "failed to read file: subdir/pyproject.toml")
                for cause in err.chain() {
                    writeln!(stderr, "  {} {cause}", "Cause:".bold()).ok();
                }
            }
            ExitStatus::Error.into()
        }
    }
}
