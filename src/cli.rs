use std::path::PathBuf;

use clap::{command, Parser};
use regex::Regex;
use rustc_hash::FxHashMap;

use crate::checks_gen::CheckCodePrefix;
use crate::logging::LogLevel;
use crate::printer::SerializationFormat;
use crate::settings::types::{PatternPrefixPair, PerFileIgnore, PythonVersion};

#[derive(Debug, Parser)]
#[command(author, about = "Ruff: An extremely fast Python linter.")]
#[command(version)]
pub struct Cli {
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Path to the `pyproject.toml` file to use for configuration.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Enable verbose logging.
    #[arg(short, long, group = "verbosity")]
    pub verbose: bool,
    /// Only log errors.
    #[arg(short, long, group = "verbosity")]
    pub quiet: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting
    /// errors).
    #[arg(short, long, group = "verbosity")]
    pub silent: bool,
    /// Exit with status code "0", even upon detecting errors.
    #[arg(short, long)]
    pub exit_zero: bool,
    /// Run in watch mode by re-running whenever files change.
    #[arg(short, long)]
    pub watch: bool,
    /// Attempt to automatically fix lint errors.
    #[arg(long, overrides_with("no_fix"))]
    fix: bool,
    #[clap(long, overrides_with("fix"), hide = true)]
    no_fix: bool,
    /// Disable cache reads.
    #[arg(short, long)]
    pub no_cache: bool,
    /// List of error codes to enable.
    #[arg(long, value_delimiter = ',')]
    pub select: Vec<CheckCodePrefix>,
    /// Like --select, but adds additional error codes on top of the selected
    /// ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_select: Vec<CheckCodePrefix>,
    /// List of error codes to ignore.
    #[arg(long, value_delimiter = ',')]
    pub ignore: Vec<CheckCodePrefix>,
    /// Like --ignore, but adds additional error codes on top of the ignored
    /// ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_ignore: Vec<CheckCodePrefix>,
    /// List of paths, used to exclude files and/or directories from checks.
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,
    /// Like --exclude, but adds additional files and directories on top of the
    /// excluded ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_exclude: Vec<String>,
    /// List of error codes to treat as eligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(long, value_delimiter = ',')]
    pub fixable: Vec<CheckCodePrefix>,
    /// List of error codes to treat as ineligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(long, value_delimiter = ',')]
    pub unfixable: Vec<CheckCodePrefix>,
    /// List of mappings from file pattern to code to exclude
    #[arg(long, value_delimiter = ',')]
    pub per_file_ignores: Vec<PatternPrefixPair>,
    /// Output serialization format for error messages.
    #[arg(long, value_enum, default_value_t=SerializationFormat::Text)]
    pub format: SerializationFormat,
    /// Show violations with source code.
    #[arg(long)]
    pub show_source: bool,
    /// See the files Ruff will be run against with the current settings.
    #[arg(long)]
    pub show_files: bool,
    /// See Ruff's settings.
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
    /// Set the line-length for length-associated checks and automatic
    /// formatting.
    #[arg(long)]
    pub line_length: Option<usize>,
    /// Max McCabe complexity allowed for a function.
    #[arg(long)]
    pub max_complexity: Option<usize>,
    /// Round-trip auto-formatting.
    // TODO(charlie): This should be a sub-command.
    #[arg(long, hide = true)]
    pub autoformat: bool,
    /// The name of the file when passing it through stdin.
    #[arg(long)]
    pub stdin_filename: Option<String>,
}

impl Cli {
    // See: https://github.com/clap-rs/clap/issues/3146
    pub fn fix(&self) -> Option<bool> {
        resolve_bool_arg(self.fix, self.no_fix)
    }
}

fn resolve_bool_arg(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
        (..) => unreachable!("Clap should make this impossible"),
    }
}

/// Map the CLI settings to a `LogLevel`.
pub fn extract_log_level(cli: &Cli) -> LogLevel {
    if cli.silent {
        LogLevel::Silent
    } else if cli.quiet {
        LogLevel::Quiet
    } else if cli.verbose {
        LogLevel::Verbose
    } else if matches!(cli.format, SerializationFormat::Json) {
        LogLevel::Quiet
    } else {
        LogLevel::Default
    }
}

/// Convert a list of `PatternPrefixPair` structs to `PerFileIgnore`.
pub fn collect_per_file_ignores(
    pairs: Vec<PatternPrefixPair>,
    project_root: Option<&PathBuf>,
) -> Vec<PerFileIgnore> {
    let mut per_file_ignores: FxHashMap<String, Vec<CheckCodePrefix>> = FxHashMap::default();
    for pair in pairs {
        per_file_ignores
            .entry(pair.pattern)
            .or_insert_with(Vec::new)
            .push(pair.prefix);
    }
    per_file_ignores
        .iter()
        .map(|(pattern, prefixes)| PerFileIgnore::new(pattern, prefixes, project_root))
        .collect()
}
