//! This crate implements an internal CLI for developers of Ruff.
//!
//! Within the ruff repository you can run it with `cargo dev`.

use anyhow::Result;
use clap::{Parser, Subcommand};
use ruff::{args::GlobalConfigArgs, check};
use ruff_linter::logging::set_up_logging;
use std::process::ExitCode;

mod format_dev;
mod generate_all;
mod generate_cli_help;
mod generate_docs;
mod generate_json_schema;
mod generate_options;
mod generate_rules_table;
mod print_ast;
mod print_cst;
mod print_tokens;
mod round_trip;

const ROOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    command: Command,
    #[clap(flatten)]
    global_options: GlobalConfigArgs,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Command {
    /// Run all code and documentation generation steps.
    GenerateAll(generate_all::Args),
    /// Generate JSON schema for the TOML configuration file.
    GenerateJSONSchema(generate_json_schema::Args),
    /// Generate a Markdown-compatible table of supported lint rules.
    GenerateRulesTable,
    /// Generate a Markdown-compatible listing of configuration options.
    GenerateOptions,
    /// Generate CLI help.
    GenerateCliHelp(generate_cli_help::Args),
    /// Generate Markdown docs.
    GenerateDocs(generate_docs::Args),
    /// Print the AST for a given Python file.
    PrintAST(print_ast::Args),
    /// Print the LibCST CST for a given Python file.
    PrintCST(print_cst::Args),
    /// Print the token stream for a given Python file.
    PrintTokens(print_tokens::Args),
    /// Run round-trip source code generation on a given Python file.
    RoundTrip(round_trip::Args),
    /// Run a ruff command n times for profiling/benchmarking
    Repeat {
        #[clap(flatten)]
        args: ruff::args::CheckCommand,
        /// Run this many times
        #[clap(long)]
        repeat: usize,
    },
    /// Several utils related to the formatter which can be run on one or more repositories. The
    /// selected set of files in a repository is the same as for `ruff check`.
    ///
    /// * Check formatter stability: Format a repository twice and ensure that it looks that the
    ///   first and second formatting look the same.
    /// * Format: Format the files in a repository to be able to check them with `git diff`
    /// * Statistics: The subcommand the Jaccard index between the (assumed to be black formatted)
    ///   input and the ruff formatted output
    FormatDev(format_dev::Args),
}

fn main() -> Result<ExitCode> {
    let Args {
        command,
        global_options,
    } = Args::parse();
    #[allow(clippy::print_stdout)]
    match command {
        Command::GenerateAll(args) => generate_all::main(&args)?,
        Command::GenerateJSONSchema(args) => generate_json_schema::main(&args)?,
        Command::GenerateRulesTable => println!("{}", generate_rules_table::generate()),
        Command::GenerateOptions => println!("{}", generate_options::generate()),
        Command::GenerateCliHelp(args) => generate_cli_help::main(&args)?,
        Command::GenerateDocs(args) => generate_docs::main(&args)?,
        Command::PrintAST(args) => print_ast::main(&args)?,
        Command::PrintCST(args) => print_cst::main(&args)?,
        Command::PrintTokens(args) => print_tokens::main(&args)?,
        Command::RoundTrip(args) => round_trip::main(&args)?,
        Command::Repeat {
            args: subcommand_args,
            repeat,
        } => {
            set_up_logging(global_options.log_level())?;
            for _ in 0..repeat {
                check(subcommand_args.clone(), global_options.clone())?;
            }
        }
        Command::FormatDev(args) => {
            let exit_code = format_dev::main(&args)?;
            return Ok(exit_code);
        }
    }
    Ok(ExitCode::SUCCESS)
}
