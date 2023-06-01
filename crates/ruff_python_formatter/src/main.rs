use std::fs;

use anyhow::Result;
use clap::Parser as ClapParser;

use ruff_python_formatter::cli::Cli;
use ruff_python_formatter::format_module;

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let cli = Cli::parse();
    let contents = fs::read_to_string(cli.file)?;
    println!("{}", format_module(&contents)?.as_code());
    Ok(())
}
