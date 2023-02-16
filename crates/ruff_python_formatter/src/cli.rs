use std::path::PathBuf;

use clap::{command, Parser};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Python file to round-trip.
    #[arg(required = true)]
    pub file: PathBuf,
}
