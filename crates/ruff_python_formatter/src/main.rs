use std::fs;

use anyhow::Result;
use clap::Parser as ClapParser;

use ruff_python_formatter::cli::Cli;
use ruff_python_formatter::format_module;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let contents = fs::read_to_string(cli.file)?;
    #[allow(clippy::print_stdout)]
    {
        println!("{}", format_module(&contents)?.as_code());
    }
    Ok(())
}
