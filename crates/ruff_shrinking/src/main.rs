//! Minimize a failing python file, a process known as [shrinking](https://www.educative.io/answers/what-is-shrinking)
//!
//! See `--help` (or the [Args] docs) for usage.
//!
//! ## Example
//!
//! Shrink a file with unstable formatting
//! ```shell
//! cargo run --bin ruff-minimizer -- target/cpython/Lib/test/inspect_fodder.py target/minirepo/a.py "Unstable formatting" "target/debug/ruff_dev format-dev --stability-check target/minirepo"
//! ```
//! This could emit
//! ```python
//! class WhichComments:    # before f        return 1        # end f    # after f    # before asyncf - line 108
//!     async def asyncf(self):        return 2
//!         # end asyncf    # end of WhichComments - line 114# a closing parenthesis with the opening paren being in another line
//! (
//! );
//! ```
//!
//! Shrink a file with a syntax error
//! ```shell
//! cargo run --bin ruff_shrinking -- target/checkouts/jhnnsrs:mikro-napari/mikro_napari/models/representation.py target/minirepo/code.py "invalid syntax" "target/debug/ruff_dev format-dev --stability-check target/minirepo"
//! ```
//! This could emit
//! ```python
//! class RepresentationQtModel():
//!             data[:,] = rep.data
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::{Context, Result};
use clap::Parser;
use fs_err as fs;
use regex::Regex;
use ruff_python_ast::statement_visitor::{walk_body, walk_stmt, StatementVisitor};
use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::{Expr, Stmt, Suite};
use ruff_python_parser::Mode;
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::str;
use std::time::Instant;
use tracing::debug;

const STRATEGIES: &[&dyn Strategy] = &[
    (&StrategyRemoveModuleMember),
    (&StrategyRemoveStatement),
    (&StrategyRemoveExpression),
    (&StrategyRemoveLine),
    (&StrategyRemoveNewline),
    (&StrategyRemoveToken),
    (&StrategyRemoveChar),
];

/// Each strategy is a way of producing possible minimizations
trait Strategy {
    fn name(&self) -> &'static str;

    fn candidates<'a>(
        &self,
        input: &'a str,
        ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>>;
}

/// vtable surrogate trait that rust wants
trait ExactSizeStringIter: Iterator<Item = String> + ExactSizeIterator {}

impl<T> ExactSizeStringIter for T where T: Iterator<Item = String> + ExactSizeIterator {}

/// Remove a top level member from a module. Generally the most effective strategy since most
/// top level items will be unrelated to the error.
struct StrategyRemoveModuleMember;

impl Strategy for StrategyRemoveModuleMember {
    fn name(&self) -> &'static str {
        "remove module member"
    }

    fn candidates<'a>(
        &self,
        input: &'a str,
        ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>> {
        let iter = ast.iter().map(|stmt| {
            // trim the newlines the range misses
            input[..stmt.start().to_usize()].trim_end().to_string()
                + input[stmt.end().to_usize()..].trim_start()
        });
        Ok(Box::new(iter))
    }
}

/// Finds the ranges of all statements.
#[derive(Default)]
struct StatementCollector {
    /// The ranges of all statements
    ranges: Vec<TextRange>,
}

impl StatementVisitor<'_> for StatementCollector {
    fn visit_body(&mut self, body: &[Stmt]) {
        if let (Some(first), Some(last)) = (body.first(), body.last()) {
            if !(first == last && matches!(first, Stmt::Pass(_))) {
                self.ranges.push(TextRange::new(first.start(), last.end()));
            }
        }
        walk_body(self, body);
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        if !matches!(stmt, Stmt::Pass(_)) {
            self.ranges.push(stmt.range());
        }
        walk_stmt(self, stmt);
    }
}

/// Try to remove each statement or replace it statement with `pass`
fn strategy_statement<'a>(
    input: &'a str,
    ast: &'a Suite,
    pass_dummy: bool,
) -> Box<dyn ExactSizeStringIter + 'a> {
    let mut visitor = StatementCollector::default();
    visitor.visit_body(ast);

    // Remove the largest statements first
    let mut ranges = visitor.ranges;
    ranges.sort_by_key(|range| range.len());
    ranges.reverse();

    let iter = ranges.into_iter().map(move |range| {
        let mut without_stmt = String::new();
        // trim the newlines the range misses
        without_stmt.push_str(input[..range.start().to_usize()].trim_end());
        if pass_dummy {
            without_stmt.push_str("pass");
        }
        without_stmt.push_str(&input[range.end().to_usize()..]);
        without_stmt
    });
    Box::new(iter)
}

struct StrategyRemoveStatement;

impl Strategy for StrategyRemoveStatement {
    fn name(&self) -> &'static str {
        "remove statement"
    }
    fn candidates<'a>(
        &self,
        input: &'a str,
        ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>> {
        Ok(strategy_statement(input, ast, false))
    }
}

/// Finds the ranges of all expressions.
#[derive(Default)]
struct ExpressionCollector {
    /// The ranges of all expressions
    ranges: Vec<TextRange>,
}

impl Visitor<'_> for ExpressionCollector {
    fn visit_expr(&mut self, expr: &Expr) {
        self.ranges.push(expr.range());
        walk_expr(self, expr);
    }
}

struct StrategyRemoveExpression;

impl Strategy for StrategyRemoveExpression {
    fn name(&self) -> &'static str {
        "remove expression"
    }

    /// Try to remove each expression
    fn candidates<'a>(
        &self,
        input: &'a str,
        ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>> {
        let mut visitor = ExpressionCollector::default();
        visitor.visit_body(ast);
        let iter = visitor.ranges.into_iter().map(move |range| {
            input[TextRange::up_to(range.start())].to_string() + &input[range.end().to_usize()..]
        });
        Ok(Box::new(iter))
    }
}

/// Remove each line (physical lines, not logical lines).
struct StrategyRemoveLine;

impl Strategy for StrategyRemoveLine {
    fn name(&self) -> &'static str {
        "remove line"
    }

    /// Returns the number of permutations and each permutation
    fn candidates<'a>(
        &self,
        input: &'a str,
        _ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>> {
        let lines: Vec<_> = input.lines().collect();
        let iter = (0..lines.len()).map(move |removed_line| {
            let mut result = String::new();
            result.push_str(&lines[..removed_line].join("\n"));
            if removed_line > 0 {
                result.push('\n');
            }
            result.push_str(&lines[removed_line + 1..].join("\n"));
            result
        });
        Ok(Box::new(iter))
    }
}

/// Try removing newline characters
struct StrategyRemoveNewline;

impl Strategy for StrategyRemoveNewline {
    fn name(&self) -> &'static str {
        "remove newline"
    }

    /// Returns the number of permutations and each permutation
    fn candidates<'a>(
        &self,
        input: &'a str,
        _ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>> {
        let newline_positions: Vec<_> = input
            .char_indices()
            .filter_map(|(pos, char)| {
                // Don't remove newlines after `:`. Indexing is save because pos > 0 is checked and
                // pos - 1 is behind a position we know exists and we're indexing into bytes instead
                // of chars
                if char == '\n' && pos > 0 && input.as_bytes()[pos - 1] != b':' {
                    Some(pos)
                } else {
                    None
                }
            })
            .collect();
        let iter = newline_positions.into_iter().map(move |newline_position| {
            // trim to remove the indentation
            input[..newline_position].to_string()
                + input[newline_position + '\n'.len_utf8()..].trim_start()
        });
        Ok(Box::new(iter))
    }
}

/// Try removing each python token. This is really slow and runs at the end
struct StrategyRemoveToken;

impl Strategy for StrategyRemoveToken {
    fn name(&self) -> &'static str {
        "remove token"
    }

    fn candidates<'a>(
        &self,
        input: &'a str,
        _ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>> {
        let token_ranges: Vec<_> = ruff_python_parser::tokenize(input, Mode::Module)
            .into_iter()
            // At this point we know we have valid python code
            .map(Result::unwrap)
            .filter(|token| token.1.len().to_usize() > 0)
            .map(|token| token.1)
            .collect();

        let iter = token_ranges.into_iter().map(move |range| {
            input[..range.start().to_usize()].to_string() + &input[range.end().to_usize()..]
        });
        Ok(Box::new(iter))
    }
}

/// Try removing each individual character in the file. This is really slow and runs at the end
struct StrategyRemoveChar;

impl Strategy for StrategyRemoveChar {
    fn name(&self) -> &'static str {
        "remove character"
    }

    fn candidates<'a>(
        &self,
        input: &'a str,
        _ast: &'a Suite,
    ) -> Result<Box<dyn ExactSizeStringIter + 'a>> {
        let char_indices: Vec<_> = input.char_indices().collect();
        let iter = char_indices
            .into_iter()
            .map(move |(pos, char)| input[..pos].to_string() + &input[pos + char.len_utf8()..]);
        Ok(Box::new(iter))
    }
}

/// Returns strategy, posing in the iteration (so they can be skipped in the next attempt) and
/// minimized code.
fn minimization_step(
    input: &str,
    location: &Path,
    command_args: &[String],
    pattern: &Regex,
    last_strategy_and_idx: Option<(&'static dyn Strategy, usize)>,
) -> Result<Option<(&'static dyn Strategy, usize, String)>> {
    let tokens = ruff_python_parser::tokenize(input, Mode::Module);
    let ast = ruff_python_parser::parse_program_tokens(tokens, input, false)
        .context("not valid python")?;

    // Try the last succeeding strategy first, skipping all that failed last time
    if let Some((last_strategy, last_idx)) = last_strategy_and_idx {
        let iter = last_strategy.candidates(input, &ast)?;
        println!(
            "Trying {} ({last_idx} skipped) {} candidates",
            iter.len() - last_idx,
            last_strategy.name()
        );
        for (idx, entry) in iter.enumerate().skip(last_idx) {
            if is_failing(&entry, location, command_args, pattern)? {
                // This one is still failing in the right way
                return Ok(Some((last_strategy, idx, entry)));
            }
        }
    }

    // Try all strategies in order, including the last successful one without skipping inputs
    for strategy in STRATEGIES {
        let iter = strategy.candidates(input, &ast)?;
        println!("Trying {} {} candidates", iter.len(), strategy.name());
        for (idx, entry) in iter.enumerate() {
            if is_failing(&entry, location, command_args, pattern)? {
                // This one is still failing in the right way
                return Ok(Some((*strategy, idx, entry)));
            }
        }
    }

    // None of the minimizations worked
    Ok(None)
}

/// Does the candidate still produce the expected error?
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

/// You specify an input file that fails. The minimizer will write minimization candidates to the
/// the file given as second argument. It will run the command and if the output still matches the
/// error pattern, the candidate will be considered a successful minimization step, otherwise it's
/// rolled back.
///
/// ## Example
///  ```shell
/// cargo run --bin ruff_shrinking -- target/checkouts/jhnnsrs:mikro-napari/mikro_napari/models/representation.py target/minirepo/code.py "invalid syntax" "target/debug/ruff_dev format-dev --stability-check target/minirepo"
/// ```
/// This could emit (if it wasn't fixed):
/// ```python
/// class RepresentationQtModel():
///             data[:,] = rep.data
/// ```
#[derive(Parser)]
struct Args {
    /// The input file that fails
    input_file: PathBuf,
    /// The minimization attempt is written to this location
    output_file: PathBuf,
    /// Continue this path of the minimization if either stderr or stdout match this regex
    error_pattern: String,
    /// The command to run to test if the smaller version still emits the same error
    ///
    /// TODO(konstin): Move this to some form of trailing args so we don't need shlex
    command: String,
}

fn run() -> Result<()> {
    // e.g. `RUST_LOG=ruff_shrinking=debug`
    tracing_subscriber::fmt::init();

    let args: Args = Args::parse();
    let pattern = Regex::new(&args.error_pattern).context("Invalid error_pattern")?;
    let command_args = shlex::split(&args.command).context("Couldn't split command input")?;

    let loop_start = Instant::now();
    let mut stats = HashMap::new();

    // Normalize line endings for the remove newline dependent rules
    let mut input = fs::read_to_string(args.input_file)?.replace('\r', "");

    // This can happen e.g. when main changed between collecting the errors list and running this
    // script
    if !is_failing(&input, &args.output_file, &command_args, &pattern)? {
        println!("Input doesn't match");
        fs::write(&args.output_file, "")?;
        return Ok(());
    }

    let mut num_iterations = 0;
    let mut last_strategy_and_idx = None;
    loop {
        let start = Instant::now();
        num_iterations += 1;
        let smaller_failure = minimization_step(
            &input,
            &args.output_file,
            &command_args,
            &pattern,
            last_strategy_and_idx,
        )?;
        let duration = start.elapsed();
        if let Some((strategy, idx, smaller_failure)) = smaller_failure {
            println!(
                "Match found with {} {idx} in {:.2}s, {} bytes remaining",
                strategy.name(),
                duration.as_secs_f32(),
                smaller_failure.len()
            );
            *stats.entry(strategy.name()).or_insert(0) += 1;
            input = smaller_failure;
            last_strategy_and_idx = Some((strategy, idx));
        } else {
            // The last minimization failed, write back the original content
            fs::write(&args.output_file, input.as_bytes())?;
            println!(
                "Last iteration in {:.2}s, {} bytes remaining",
                duration.as_secs_f32(),
                input.as_bytes().len()
            );
            break;
        }
    }

    println!("Strategies taken: {stats:?}");
    println!(
        "Done with {num_iterations} iterations in {:.2}s. Find your minimized example in {}:\n---\n{}\n---\n",
        loop_start.elapsed().as_secs_f32(),
        args.output_file.display(),
        input
    );

    Ok(())
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("💥 Minimizer failed");
        for cause in e.chain() {
            eprintln!("  Cause: {cause}");
        }
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
