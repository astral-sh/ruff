use std::fs;

use anyhow::Result;
use clap::Parser as ClapParser;
use ruff_fmt::cli::Cli;
use ruff_fmt::fmt;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let contents = fs::read_to_string(cli.file)?;
    #[allow(clippy::print_stdout)]
    {
        println!("{}", fmt(&contents)?.print()?.as_code());
    }
    Ok(())
}
