//! This crate implements an internal CLI for developers of Ruff.
//!
//! Within the ruff repository you can run it with `cargo dev`.
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod generate_all;
mod generate_cli_help;
mod generate_json_schema;
mod generate_options;
mod generate_rules_table;
mod print_ast;
mod print_cst;
mod print_tokens;
mod round_trip;
mod utils;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all code and documentation generation steps.
    GenerateAll(generate_all::Cli),
    /// Generate JSON schema for the TOML configuration file.
    GenerateJSONSchema(generate_json_schema::Cli),
    /// Generate a Markdown-compatible table of supported lint rules.
    GenerateRulesTable(generate_rules_table::Cli),
    /// Generate a Markdown-compatible listing of configuration options.
    GenerateOptions(generate_options::Cli),
    /// Generate CLI help.
    GenerateCliHelp(generate_cli_help::Cli),
    /// Print the AST for a given Python file.
    PrintAST(print_ast::Cli),
    /// Print the LibCST CST for a given Python file.
    PrintCST(print_cst::Cli),
    /// Print the token stream for a given Python file.
    PrintTokens(print_tokens::Cli),
    /// Run round-trip source code generation on a given Python file.
    RoundTrip(round_trip::Cli),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::GenerateAll(args) => generate_all::main(args)?,
        Commands::GenerateJSONSchema(args) => generate_json_schema::main(args)?,
        Commands::GenerateRulesTable(args) => generate_rules_table::main(args)?,
        Commands::GenerateOptions(args) => generate_options::main(args)?,
        Commands::GenerateCliHelp(args) => generate_cli_help::main(args)?,
        Commands::PrintAST(args) => print_ast::main(args)?,
        Commands::PrintCST(args) => print_cst::main(args)?,
        Commands::PrintTokens(args) => print_tokens::main(args)?,
        Commands::RoundTrip(args) => round_trip::main(args)?,
    }
    Ok(())
}
