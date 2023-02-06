//! This crate implements an internal CLI for developers of Ruff.
//!
//! Within the ruff repository you can run it with `cargo dev`.

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

const ROOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run all code and documentation generation steps.
    GenerateAll(generate_all::Args),
    /// Generate JSON schema for the TOML configuration file.
    GenerateJSONSchema(generate_json_schema::Args),
    /// Generate a Markdown-compatible table of supported lint rules.
    GenerateRulesTable(generate_rules_table::Args),
    /// Generate a Markdown-compatible listing of configuration options.
    GenerateOptions(generate_options::Args),
    /// Generate CLI help.
    GenerateCliHelp(generate_cli_help::Args),
    /// Print the AST for a given Python file.
    PrintAST(print_ast::Args),
    /// Print the LibCST CST for a given Python file.
    PrintCST(print_cst::Args),
    /// Print the token stream for a given Python file.
    PrintTokens(print_tokens::Args),
    /// Run round-trip source code generation on a given Python file.
    RoundTrip(round_trip::Args),
}

fn main() -> Result<()> {
    let args = Args::parse();
    match &args.command {
        Command::GenerateAll(args) => generate_all::main(args)?,
        Command::GenerateJSONSchema(args) => generate_json_schema::main(args)?,
        Command::GenerateRulesTable(args) => generate_rules_table::main(args)?,
        Command::GenerateOptions(args) => generate_options::main(args)?,
        Command::GenerateCliHelp(args) => generate_cli_help::main(args)?,
        Command::PrintAST(args) => print_ast::main(args)?,
        Command::PrintCST(args) => print_cst::main(args)?,
        Command::PrintTokens(args) => print_tokens::main(args)?,
        Command::RoundTrip(args) => round_trip::main(args)?,
    }
    Ok(())
}
