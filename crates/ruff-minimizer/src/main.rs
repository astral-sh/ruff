//! Minimize a failing python file.
//!
//! ## Example
//!
//! Minimize a file with unstable formatting
//! ```shell
//! cargo run --bin ruff-minimizer -- target/cpython/Lib/test/inspect_fodder.py target/minirepo/a.py "Unstable formatting" "target/debug/ruff_dev check-formatter-stability target/minirepo"
//! ```
//! This will emit
//! ```python
//! class WhichComments:    # before f        return 1        # end f    # after f    # before asyncf - line 108
//!     async def asyncf(self):        return 2
//!         # end asyncf    # end of WhichComments - line 114# a closing parenthesis with the opening paren being in another line
//! (
//! );
//! ```
//! which only has only the two involved top level statements and the one relevant comment line
//! remaining

#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::{Context, Result};
use clap::Parser;
use fs_err as fs;
use regex::Regex;
use rustpython_ast::Ranged;
use std::iter;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::str;
use std::time::Instant;
use tracing::debug;

#[derive(Parser)]
struct Args {
    /// The input file
    file: PathBuf,
    /// The minimization attempt will be copied to this location
    location: PathBuf,
    /// Continue this path of the minimization if either stderr or stdout match this regex
    error_pattern: String,
    /// The command to run to test if the smaller version still emits the same error
    ///
    /// TODO(konstin): Move this to some form of trailing args so we don't need shlex
    command: String,
}

/// Returns the number of permutations and each permutation
fn make_line_candidates(input: &str) -> (usize, Box<dyn Iterator<Item = String> + '_>) {
    let lines: Vec<_> = input.lines().collect();
    let num_candidates = lines.len();
    if num_candidates <= 1 {
        return (0, Box::new(iter::empty()));
    }
    let mut removed_line = 0;
    let iter = iter::from_fn(move || {
        if removed_line < lines.len() {
            let mut result = String::new();
            result.push_str(&lines[..removed_line].join("\n"));
            if removed_line > 0 {
                result.push_str("\n");
            }
            result.push_str(&lines[removed_line + 1..].join("\n"));
            removed_line += 1;
            Some(result)
        } else {
            None
        }
    });
    (num_candidates, Box::new(iter))
}

/// Try to remove each module level statement
fn make_module_candidates(input: &str) -> Result<(usize, Box<dyn Iterator<Item = String> + '_>)> {
    let tokens = ruff_rustpython::tokenize(input);
    let ast =
        ruff_rustpython::parse_program_tokens(tokens, "input.py").context("not valid python")?;
    let num_candidates = ast.len();
    if num_candidates <= 1 {
        return Ok((0, Box::new(iter::empty())));
    }
    let iter = ast.into_iter().map(|stmt| {
        let mut without_stmt = String::new();
        without_stmt.push_str(&input[..stmt.range().start().to_usize()]);
        without_stmt.push_str(&input[stmt.range().end().to_usize()..]);
        without_stmt
    });
    Ok((num_candidates, Box::new(iter)))
}

fn find_smaller_failure(
    input: &str,
    location: &Path,
    command_args: &[String],
    pattern: &Regex,
) -> Result<Option<String>> {
    let (num_candidates, iter) = make_module_candidates(input)?;
    println!("{num_candidates} module candidates");
    for entry in iter {
        if is_failing(&entry, location, command_args, pattern)? {
            // This one is still failing in the right way
            return Ok(Some(entry));
        }
    }
    let (num_candidates, iter) = make_line_candidates(input);
    println!("{num_candidates} line candidates");
    for entry in iter {
        if is_failing(&entry, location, command_args, pattern)? {
            return Ok(Some(entry));
        }
    }
    // None of the minimizations worked
    Ok(None)
}

fn is_failing(
    input: &str,
    location: &Path,
    command_args: &[String],
    pattern: &Regex,
) -> Result<bool> {
    fs::write(location, input).context("Invalid location")?;

    let output = Command::new(&command_args[0])
        .args(&command_args[1..])
        .output()
        .context("Failed to launch command")?;

    let stdout = str::from_utf8(&output.stdout).context("stdout was not utf8")?;
    let stderr = str::from_utf8(&output.stderr).context("stderr was not utf8")?;
    if pattern.is_match(stdout) {
        debug!("stdout matches");
        Ok(true)
    } else if pattern.is_match(stderr) {
        debug!("stderr matches");
        Ok(true)
    } else {
        Ok(false)
    }
}

fn run() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args: Args = Args::parse();
    let pattern = Regex::new(&args.error_pattern).context("Invalid error_pattern")?;
    let mut input = fs::read_to_string(args.file)?;
    let command_args = shlex::split(&args.command).context("Couldn't split command input")?;

    loop {
        let start = Instant::now();
        let smaller_failure =
            find_smaller_failure(&input, &args.location, &command_args, &pattern)?;
        let duration = start.elapsed();
        println!("Iteration took {:.1}s", duration.as_secs_f32());
        if let Some(smaller_failure) = smaller_failure {
            input = smaller_failure;
        } else {
            // The last minimization failed, write back the original content
            fs::write(&args.location, input.as_bytes())?;
            break;
        }
    }

    println!(
        "Done. Find your minimized example in {}",
        args.location.display()
    );

    Ok(())
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("💥 Minimizer failed");
        for cause in e.chain() {
            eprintln!("  Caused by: {cause}");
        }
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
