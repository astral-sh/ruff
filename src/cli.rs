use std::path::{Path, PathBuf};

use clap::{command, Parser};
use regex::Regex;
use rustc_hash::FxHashMap;

use crate::checks::CheckCode;
use crate::checks_gen::CheckCodePrefix;
use crate::fs;
use crate::logging::LogLevel;
use crate::settings::types::{
    FilePattern, PatternPrefixPair, PerFileIgnore, PythonVersion, SerializationFormat,
};

#[derive(Debug, Parser)]
#[command(author, about = "Ruff: An extremely fast Python linter.")]
#[command(version)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[arg(required_unless_present_any = ["explain", "generate_shell_completion"])]
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
    pub select: Option<Vec<CheckCodePrefix>>,
    /// Like --select, but adds additional error codes on top of the selected
    /// ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_select: Option<Vec<CheckCodePrefix>>,
    /// List of error codes to ignore.
    #[arg(long, value_delimiter = ',')]
    pub ignore: Option<Vec<CheckCodePrefix>>,
    /// Like --ignore, but adds additional error codes on top of the ignored
    /// ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_ignore: Option<Vec<CheckCodePrefix>>,
    /// List of paths, used to exclude files and/or directories from checks.
    #[arg(long, value_delimiter = ',')]
    pub exclude: Option<Vec<FilePattern>>,
    /// Like --exclude, but adds additional files and directories on top of the
    /// excluded ones.
    #[arg(long, value_delimiter = ',')]
    pub extend_exclude: Option<Vec<FilePattern>>,
    /// List of error codes to treat as eligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(long, value_delimiter = ',')]
    pub fixable: Option<Vec<CheckCodePrefix>>,
    /// List of error codes to treat as ineligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(long, value_delimiter = ',')]
    pub unfixable: Option<Vec<CheckCodePrefix>>,
    /// List of mappings from file pattern to code to exclude
    #[arg(long, value_delimiter = ',')]
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    /// Output serialization format for error messages.
    #[arg(long, value_enum)]
    pub format: Option<SerializationFormat>,
    /// Show violations with source code.
    #[arg(long, overrides_with("no_show_source"))]
    show_source: bool,
    #[clap(long, overrides_with("show_source"), hide = true)]
    no_show_source: bool,
    /// Respect file exclusions via `.gitignore` and other standard ignore
    /// files.
    #[arg(long, overrides_with("no_respect_gitignore"))]
    respect_gitignore: bool,
    #[clap(long, overrides_with("respect_gitignore"), hide = true)]
    no_respect_gitignore: bool,
    /// Enforce exclusions, even for paths passed to Ruff directly on the
    /// command-line.
    #[arg(long, overrides_with("no_show_source"))]
    force_exclude: bool,
    #[clap(long, overrides_with("force_exclude"), hide = true)]
    no_force_exclude: bool,
    /// See the files Ruff will be run against with the current settings.
    #[arg(long)]
    pub show_files: bool,
    /// See the settings Ruff will use to check a given Python file.
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
    pub stdin_filename: Option<PathBuf>,
    /// Explain a rule.
    #[arg(long)]
    pub explain: Option<CheckCode>,
    /// Generate shell completion
    #[arg(long, hide = true, value_name = "SHELL")]
    pub generate_shell_completion: Option<clap_complete_command::Shell>,
}

impl Cli {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(self) -> (Arguments, Overrides) {
        (
            Arguments {
                add_noqa: self.add_noqa,
                autoformat: self.autoformat,
                config: self.config,
                exit_zero: self.exit_zero,
                explain: self.explain,
                files: self.files,
                generate_shell_completion: self.generate_shell_completion,
                no_cache: self.no_cache,
                quiet: self.quiet,
                show_files: self.show_files,
                show_settings: self.show_settings,
                silent: self.silent,
                stdin_filename: self.stdin_filename,
                verbose: self.verbose,
                watch: self.watch,
            },
            Overrides {
                dummy_variable_rgx: self.dummy_variable_rgx,
                exclude: self.exclude,
                extend_exclude: self.extend_exclude,
                extend_ignore: self.extend_ignore,
                extend_select: self.extend_select,
                fixable: self.fixable,
                ignore: self.ignore,
                line_length: self.line_length,
                max_complexity: self.max_complexity,
                per_file_ignores: self.per_file_ignores,
                respect_gitignore: resolve_bool_arg(
                    self.respect_gitignore,
                    self.no_respect_gitignore,
                ),
                select: self.select,
                show_source: resolve_bool_arg(self.show_source, self.no_show_source),
                target_version: self.target_version,
                unfixable: self.unfixable,
                // TODO(charlie): Included in `pyproject.toml`, but not inherited.
                fix: resolve_bool_arg(self.fix, self.no_fix),
                format: self.format,
                force_exclude: resolve_bool_arg(self.force_exclude, self.no_force_exclude),
            },
        )
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

/// CLI settings that are distinct from configuration (commands, lists of files,
/// etc.).
#[allow(clippy::struct_excessive_bools)]
pub struct Arguments {
    pub add_noqa: bool,
    pub autoformat: bool,
    pub config: Option<PathBuf>,
    pub exit_zero: bool,
    pub explain: Option<CheckCode>,
    pub files: Vec<PathBuf>,
    pub generate_shell_completion: Option<clap_complete_command::Shell>,
    pub no_cache: bool,
    pub quiet: bool,
    pub show_files: bool,
    pub show_settings: bool,
    pub silent: bool,
    pub stdin_filename: Option<PathBuf>,
    pub verbose: bool,
    pub watch: bool,
}

/// CLI settings that function as configuration overrides.
#[derive(Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Overrides {
    pub dummy_variable_rgx: Option<Regex>,
    pub exclude: Option<Vec<FilePattern>>,
    pub extend_exclude: Option<Vec<FilePattern>>,
    pub extend_ignore: Option<Vec<CheckCodePrefix>>,
    pub extend_select: Option<Vec<CheckCodePrefix>>,
    pub fixable: Option<Vec<CheckCodePrefix>>,
    pub ignore: Option<Vec<CheckCodePrefix>>,
    pub line_length: Option<usize>,
    pub max_complexity: Option<usize>,
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    pub respect_gitignore: Option<bool>,
    pub select: Option<Vec<CheckCodePrefix>>,
    pub show_source: Option<bool>,
    pub target_version: Option<PythonVersion>,
    pub unfixable: Option<Vec<CheckCodePrefix>>,
    // TODO(charlie): Captured in pyproject.toml as a default, but not part of `Settings`.
    pub fix: Option<bool>,
    pub format: Option<SerializationFormat>,
    pub force_exclude: Option<bool>,
}

/// Map the CLI settings to a `LogLevel`.
pub fn extract_log_level(cli: &Arguments) -> LogLevel {
    if cli.silent {
        LogLevel::Silent
    } else if cli.quiet {
        LogLevel::Quiet
    } else if cli.verbose {
        LogLevel::Verbose
    } else {
        LogLevel::Default
    }
}

/// Convert a list of `PatternPrefixPair` structs to `PerFileIgnore`.
pub fn collect_per_file_ignores(pairs: Vec<PatternPrefixPair>) -> Vec<PerFileIgnore> {
    let mut per_file_ignores: FxHashMap<String, Vec<CheckCodePrefix>> = FxHashMap::default();
    for pair in pairs {
        per_file_ignores
            .entry(pair.pattern)
            .or_insert_with(Vec::new)
            .push(pair.prefix);
    }
    per_file_ignores
        .into_iter()
        .map(|(pattern, prefixes)| {
            let absolute = fs::normalize_path(Path::new(&pattern));
            PerFileIgnore::new(pattern, absolute, &prefixes)
        })
        .collect()
}
