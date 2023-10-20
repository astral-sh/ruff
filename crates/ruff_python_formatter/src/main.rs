use std::io::{stdout, Read, Write};
use std::path::Path;
use std::{fs, io};

use anyhow::{bail, Context, Result};
use clap::Parser as ClapParser;

use ruff_python_formatter::cli::{format_and_debug_print, Cli, Emit};

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
        let source = read_from_stdin()?;
        // It seems reasonable to give this a dummy name
        let formatted = format_and_debug_print(&source, &cli, Path::new("stdin.py"))?;
        if cli.check {
            if formatted == source {
                return Ok(());
            }
            bail!("Content not correctly formatted")
        }
        stdout().lock().write_all(formatted.as_bytes())?;
    } else {
        for file in &cli.files {
            let source = fs::read_to_string(file)
                .with_context(|| format!("Could not read {}: ", file.display()))?;
            let formatted = format_and_debug_print(&source, &cli, file)?;
            match cli.emit {
                Some(Emit::Stdout) => stdout().lock().write_all(formatted.as_bytes())?,
                None | Some(Emit::Files) => {
                    fs::write(file, formatted.as_bytes()).with_context(|| {
                        format!("Could not write to {}, exiting", file.display())
                    })?;
                }
            }
        }
    }

    Ok(())
}
