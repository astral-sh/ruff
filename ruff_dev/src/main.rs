use anyhow::Result;
use clap::{Parser, Subcommand};
use ruff_dev::{
    generate_check_code_prefix, generate_rules_table, generate_source_code, print_ast,
    print_cst_ast, print_tokens,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the `CheckCodePrefix` enum.
    GenerateCheckCodePrefix(generate_check_code_prefix::Cli),
    /// Generate a Markdown-compatible table of supported lint rules.
    GenerateRulesTable(generate_rules_table::Cli),
    /// Run round-trip source code generation on a given Python file.
    GenerateSourceCode(generate_source_code::Cli),
    /// Print the AST for a given Python file.
    PrintAST(print_ast::Cli),
    /// Print the LibCST AST for a given Python file.
    PrintCstAST(print_cst_ast::Cli),
    /// Print the token stream for a given Python file.
    PrintTokens(print_tokens::Cli),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::GenerateCheckCodePrefix(args) => generate_check_code_prefix::main(args)?,
        Commands::GenerateRulesTable(args) => generate_rules_table::main(args)?,
        Commands::GenerateSourceCode(args) => generate_source_code::main(args)?,
        Commands::PrintAST(args) => print_ast::main(args)?,
        Commands::PrintCstAST(args) => print_cst_ast::main(args)?,
        Commands::PrintTokens(args) => print_tokens::main(args)?,
    }
    Ok(())
}
