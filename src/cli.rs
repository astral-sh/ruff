use std::fmt;
use std::path::PathBuf;

use clap::{command, Parser};
use log::warn;
use regex::Regex;

use crate::checks::CheckCode;
use crate::printer::SerializationFormat;
use crate::pyproject::StrCheckCodePair;
use crate::settings::PythonVersion;
use crate::RawSettings;

#[derive(Debug, Parser)]
#[command(author, about = "ruff: An extremely fast Python linter.")]
#[command(version)]
pub struct Cli {
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Enable verbose logging.
    #[arg(short, long)]
    pub verbose: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting errors).
    #[arg(short, long)]
    pub quiet: bool,
    /// Exit with status code "0", even upon detecting errors.
    #[arg(short, long)]
    pub exit_zero: bool,
    /// Run in watch mode by re-running whenever files change.
    #[arg(short, long)]
    pub watch: bool,
    /// Attempt to automatically fix lint errors.
    #[arg(short, long)]
    pub fix: bool,
    /// Disable cache reads.
    #[arg(short, long)]
    pub no_cache: bool,
    /// List of error codes to enable.
    #[arg(long, value_delimiter = ',')]
    pub select: Vec<CheckCode>,
    /// Like --select, but adds additional error codes on top of the selected ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_select: Vec<CheckCode>,
    /// List of error codes to ignore.
    #[arg(long, value_delimiter = ',')]
    pub ignore: Vec<CheckCode>,
    /// Like --ignore, but adds additional error codes on top of the ignored ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_ignore: Vec<CheckCode>,
    /// List of paths, used to exclude files and/or directories from checks.
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,
    /// Like --exclude, but adds additional files and directories on top of the excluded ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_exclude: Vec<String>,
    /// List of mappings from file pattern to code to exclude
    #[arg(long, value_delimiter = ',')]
    pub per_file_ignores: Vec<StrCheckCodePair>,
    /// Output serialization format for error messages.
    #[arg(long, value_enum, default_value_t=SerializationFormat::Text)]
    pub format: SerializationFormat,
    /// See the files ruff will be run against with the current settings.
    #[arg(long)]
    pub show_files: bool,
    /// See ruff's settings.
    #[arg(long)]
    pub show_settings: bool,
    /// Enable automatic additions of noqa directives to failing lines.
    #[arg(long)]
    pub add_noqa: bool,
    /// Regular expression matching the name of dummy variables.
    #[arg(long)]
    pub dummy_variable_rgx: Option<Regex>,
    /// The minimum Python version that should be supported.
    #[arg(long)]
    pub target_version: Option<PythonVersion>,
    /// Round-trip auto-formatting.
    // TODO(charlie): This should be a sub-command.
    #[arg(long, hide = true)]
    pub autoformat: bool,
}

pub enum Warnable {
    Select,
    ExtendSelect,
}

impl fmt::Display for Warnable {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Warnable::Select => fmt.write_str("--select"),
            Warnable::ExtendSelect => fmt.write_str("--extend-select"),
        }
    }
}

/// Warn the user if they attempt to enable a code that won't be respected.
pub fn warn_on(
    flag: Warnable,
    codes: &Vec<CheckCode>,
    cli_ignore: &Vec<CheckCode>,
    cli_extend_ignore: &Vec<CheckCode>,
    pyproject_settings: &RawSettings,
    pyproject_path: &Option<PathBuf>,
) {
    for code in codes {
        if !cli_ignore.is_empty() {
            if cli_ignore.contains(code) {
                warn!("{code:?} was passed to {flag}, but ignored via --ignore")
            }
        } else if pyproject_settings.ignore.contains(code) {
            if let Some(path) = pyproject_path {
                warn!(
                    "{code:?} was passed to {flag}, but ignored by the `ignore` field in {}",
                    path.to_string_lossy()
                )
            } else {
                warn!("{code:?} was passed to {flag}, but ignored by the default `ignore` field",)
            }
        }
        if !cli_extend_ignore.is_empty() {
            if cli_extend_ignore.contains(code) {
                warn!("{code:?} was passed to {flag}, but ignored via --extend-ignore")
            }
        } else if pyproject_settings.extend_ignore.contains(code) {
            if let Some(path) = pyproject_path {
                warn!(
                    "{code:?} was passed to {flag}, but ignored by the `extend_ignore` field in {}",
                    path.to_string_lossy()
                )
            } else {
                warn!("{code:?} was passed to {flag}, but ignored by the default `extend_ignore` field")
            }
        }
    }
}
