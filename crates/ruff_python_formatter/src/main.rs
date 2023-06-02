use std::io::{stdout, Read, Write};
use std::{fs, io};

use anyhow::{bail, Context, Result};
use clap::Parser as ClapParser;

use ruff_python_formatter::cli::{Cli, Emit};
use ruff_python_formatter::format_module;

/// Read a `String` from `stdin`.
pub(crate) fn read_from_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let cli: Cli = Cli::parse();

    if cli.files.is_empty() {
        if !matches!(cli.emit, None | Some(Emit::Stdout)) {
            bail!(
                "Can only write to stdout when formatting from stdin, but you asked for {:?}",
                cli.emit
            );
        }
        let input = read_from_stdin()?;
        let formatted = format_module(&input)?;
        if cli.check {
            if formatted.as_code() == input {
                return Ok(());
            } else {
                bail!("Content not correctly formatted")
            }
        }
        stdout().lock().write_all(formatted.as_code().as_bytes())?;
    } else {
        for file in cli.files {
            let unformatted = fs::read_to_string(&file)
                .with_context(|| format!("Could not read {}: ", file.display()))?;
            let formatted = format_module(&unformatted)?;
            match cli.emit {
                Some(Emit::Stdout) => stdout().lock().write_all(formatted.as_code().as_bytes())?,
                None | Some(Emit::Files) => {
                    fs::write(&file, formatted.as_code().as_bytes()).with_context(|| {
                        format!("Could not write to {}, exiting", file.display())
                    })?;
                }
            }
        }
    }

    Ok(())
}
