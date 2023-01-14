use std::path::{Path, PathBuf};

use clap::{command, Parser};
use regex::Regex;
use ruff::logging::LogLevel;
use ruff::registry::{RuleCode, RuleCodePrefix};
use ruff::resolver::ConfigProcessor;
use ruff::settings::types::{
    FilePattern, PatternPrefixPair, PerFileIgnore, PythonVersion, SerializationFormat,
};
use ruff::{fs, mccabe};
use rustc_hash::FxHashMap;

#[derive(Debug, Parser)]
#[command(
    author,
    name = "ruff",
    about = "Ruff: An extremely fast Python linter."
)]
#[command(version)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[arg(required_unless_present_any = ["clean", "explain", "generate_shell_completion"])]
    pub files: Vec<PathBuf>,
    /// Path to the `pyproject.toml` or `ruff.toml` file to use for
    /// configuration.
    #[arg(long, conflicts_with = "isolated")]
    pub config: Option<PathBuf>,
    /// Enable verbose logging.
    #[arg(short, long, group = "verbosity")]
    pub verbose: bool,
    /// Print lint violations, but nothing else.
    #[arg(short, long, group = "verbosity")]
    pub quiet: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting
    /// lint violations).
    #[arg(short, long, group = "verbosity")]
    pub silent: bool,
    /// Exit with status code "0", even upon detecting lint violations.
    #[arg(short, long)]
    pub exit_zero: bool,
    /// Run in watch mode by re-running whenever files change.
    #[arg(short, long)]
    pub watch: bool,
    /// Attempt to automatically fix lint violations.
    #[arg(long, overrides_with("no_fix"))]
    fix: bool,
    #[clap(long, overrides_with("fix"), hide = true)]
    no_fix: bool,
    /// Fix any fixable lint violations, but don't report on leftover
    /// violations. Implies `--fix`.
    #[arg(long, overrides_with("no_fix_only"))]
    fix_only: bool,
    #[clap(long, overrides_with("fix_only"), hide = true)]
    no_fix_only: bool,
    /// Avoid writing any fixed files back; instead, output a diff for each
    /// changed file to stdout.
    #[arg(long)]
    pub diff: bool,
    /// Disable cache reads.
    #[arg(short, long)]
    pub no_cache: bool,
    /// Ignore all configuration files.
    #[arg(long, conflicts_with = "config")]
    pub isolated: bool,
    /// Comma-separated list of rule codes to enable (or ALL, to enable all
    /// rules).
    #[arg(long, value_delimiter = ',', value_name = "RULE_CODE")]
    pub select: Option<Vec<RuleCodePrefix>>,
    /// Like --select, but adds additional rule codes on top of the selected
    /// ones.
    #[arg(long, value_delimiter = ',', value_name = "RULE_CODE")]
    pub extend_select: Option<Vec<RuleCodePrefix>>,
    /// Comma-separated list of rule codes to disable.
    #[arg(long, value_delimiter = ',', value_name = "RULE_CODE")]
    pub ignore: Option<Vec<RuleCodePrefix>>,
    /// Like --ignore, but adds additional rule codes on top of the ignored
    /// ones.
    #[arg(long, value_delimiter = ',', value_name = "RULE_CODE")]
    pub extend_ignore: Option<Vec<RuleCodePrefix>>,
    /// List of paths, used to omit files and/or directories from analysis.
    #[arg(long, value_delimiter = ',', value_name = "FILE_PATTERN")]
    pub exclude: Option<Vec<FilePattern>>,
    /// Like --exclude, but adds additional files and directories on top of
    /// those already excluded.
    #[arg(long, value_delimiter = ',', value_name = "FILE_PATTERN")]
    pub extend_exclude: Option<Vec<FilePattern>>,
    /// List of rule codes to treat as eligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(long, value_delimiter = ',', value_name = "RULE_CODE")]
    pub fixable: Option<Vec<RuleCodePrefix>>,
    /// List of rule codes to treat as ineligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(long, value_delimiter = ',', value_name = "RULE_CODE")]
    pub unfixable: Option<Vec<RuleCodePrefix>>,
    /// List of mappings from file pattern to code to exclude
    #[arg(long, value_delimiter = ',')]
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    /// Output serialization format for violations.
    #[arg(long, value_enum, env = "RUFF_FORMAT")]
    pub format: Option<SerializationFormat>,
    /// The name of the file when passing it through stdin.
    #[arg(long)]
    pub stdin_filename: Option<PathBuf>,
    /// Path to the cache directory.
    #[arg(long, env = "RUFF_CACHE_DIR")]
    pub cache_dir: Option<PathBuf>,
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
    #[arg(long, overrides_with("no_force_exclude"))]
    force_exclude: bool,
    #[clap(long, overrides_with("force_exclude"), hide = true)]
    no_force_exclude: bool,
    /// Enable or disable automatic update checks.
    #[arg(long, overrides_with("no_update_check"))]
    update_check: bool,
    #[clap(long, overrides_with("update_check"), hide = true)]
    no_update_check: bool,
    /// Regular expression matching the name of dummy variables.
    #[arg(long)]
    pub dummy_variable_rgx: Option<Regex>,
    /// The minimum Python version that should be supported.
    #[arg(long)]
    pub target_version: Option<PythonVersion>,
    /// Set the line-length for length-associated rules and automatic
    /// formatting.
    #[arg(long)]
    pub line_length: Option<usize>,
    /// Maximum McCabe complexity allowed for a given function.
    #[arg(long)]
    pub max_complexity: Option<usize>,
    /// Enable automatic additions of `noqa` directives to failing lines.
    #[arg(
        long,
        // conflicts_with = "add_noqa",
        conflicts_with = "clean",
        conflicts_with = "explain",
        conflicts_with = "generate_shell_completion",
        conflicts_with = "show_files",
        conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub add_noqa: bool,
    /// Clear any caches in the current directory or any subdirectories.
    #[arg(
        long,
        // Fake subcommands.
        conflicts_with = "add_noqa",
        // conflicts_with = "clean",
        conflicts_with = "explain",
        conflicts_with = "generate_shell_completion",
        conflicts_with = "show_files",
        conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub clean: bool,
    /// Explain a rule.
    #[arg(
        long,
        // Fake subcommands.
        conflicts_with = "add_noqa",
        conflicts_with = "clean",
        // conflicts_with = "explain",
        conflicts_with = "generate_shell_completion",
        conflicts_with = "show_files",
        conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub explain: Option<RuleCode>,
    /// Generate shell completion
    #[arg(
        long,
        hide = true,
        value_name = "SHELL",
        // Fake subcommands.
        conflicts_with = "add_noqa",
        conflicts_with = "clean",
        conflicts_with = "explain",
        // conflicts_with = "generate_shell_completion",
        conflicts_with = "show_files",
        conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub generate_shell_completion: Option<clap_complete_command::Shell>,
    /// See the files Ruff will be run against with the current settings.
    #[arg(
        long,
        // Fake subcommands.
        conflicts_with = "add_noqa",
        conflicts_with = "clean",
        conflicts_with = "explain",
        conflicts_with = "generate_shell_completion",
        // conflicts_with = "show_files",
        conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub show_files: bool,
    /// See the settings Ruff will use to lint a given Python file.
    #[arg(
        long,
        // Fake subcommands.
        conflicts_with = "add_noqa",
        conflicts_with = "clean",
        conflicts_with = "explain",
        conflicts_with = "generate_shell_completion",
        conflicts_with = "show_files",
        // conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub show_settings: bool,
}

impl Cli {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(self) -> (Arguments, Overrides) {
        (
            Arguments {
                add_noqa: self.add_noqa,
                clean: self.clean,
                config: self.config,
                diff: self.diff,
                exit_zero: self.exit_zero,
                explain: self.explain,
                files: self.files,
                generate_shell_completion: self.generate_shell_completion,
                isolated: self.isolated,
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
                cache_dir: self.cache_dir,
                fix: resolve_bool_arg(self.fix, self.no_fix),
                fix_only: resolve_bool_arg(self.fix_only, self.no_fix_only),
                force_exclude: resolve_bool_arg(self.force_exclude, self.no_force_exclude),
                format: self.format,
                update_check: resolve_bool_arg(self.update_check, self.no_update_check),
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
    pub clean: bool,
    pub config: Option<PathBuf>,
    pub diff: bool,
    pub exit_zero: bool,
    pub explain: Option<RuleCode>,
    pub files: Vec<PathBuf>,
    pub generate_shell_completion: Option<clap_complete_command::Shell>,
    pub isolated: bool,
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
    pub extend_ignore: Option<Vec<RuleCodePrefix>>,
    pub extend_select: Option<Vec<RuleCodePrefix>>,
    pub fixable: Option<Vec<RuleCodePrefix>>,
    pub ignore: Option<Vec<RuleCodePrefix>>,
    pub line_length: Option<usize>,
    pub max_complexity: Option<usize>,
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    pub respect_gitignore: Option<bool>,
    pub select: Option<Vec<RuleCodePrefix>>,
    pub show_source: Option<bool>,
    pub target_version: Option<PythonVersion>,
    pub unfixable: Option<Vec<RuleCodePrefix>>,
    // TODO(charlie): Captured in pyproject.toml as a default, but not part of `Settings`.
    pub cache_dir: Option<PathBuf>,
    pub fix: Option<bool>,
    pub fix_only: Option<bool>,
    pub force_exclude: Option<bool>,
    pub format: Option<SerializationFormat>,
    pub update_check: Option<bool>,
}

impl ConfigProcessor for &Overrides {
    fn process_config(&self, config: &mut ruff::settings::configuration::Configuration) {
        if let Some(cache_dir) = &self.cache_dir {
            config.cache_dir = Some(cache_dir.clone());
        }
        if let Some(dummy_variable_rgx) = &self.dummy_variable_rgx {
            config.dummy_variable_rgx = Some(dummy_variable_rgx.clone());
        }
        if let Some(exclude) = &self.exclude {
            config.exclude = Some(exclude.clone());
        }
        if let Some(extend_exclude) = &self.extend_exclude {
            config.extend_exclude.extend(extend_exclude.clone());
        }
        if let Some(fix) = &self.fix {
            config.fix = Some(*fix);
        }
        if let Some(fix_only) = &self.fix_only {
            config.fix_only = Some(*fix_only);
        }
        if let Some(fixable) = &self.fixable {
            config.fixable = Some(fixable.clone());
        }
        if let Some(format) = &self.format {
            config.format = Some(*format);
        }
        if let Some(force_exclude) = &self.force_exclude {
            config.force_exclude = Some(*force_exclude);
        }
        if let Some(ignore) = &self.ignore {
            config.ignore = Some(ignore.clone());
        }
        if let Some(line_length) = &self.line_length {
            config.line_length = Some(*line_length);
        }
        if let Some(max_complexity) = &self.max_complexity {
            config.mccabe = Some(mccabe::settings::Options {
                max_complexity: Some(*max_complexity),
            });
        }
        if let Some(per_file_ignores) = &self.per_file_ignores {
            config.per_file_ignores = Some(collect_per_file_ignores(per_file_ignores.clone()));
        }
        if let Some(respect_gitignore) = &self.respect_gitignore {
            config.respect_gitignore = Some(*respect_gitignore);
        }
        if let Some(select) = &self.select {
            config.select = Some(select.clone());
        }
        if let Some(show_source) = &self.show_source {
            config.show_source = Some(*show_source);
        }
        if let Some(target_version) = &self.target_version {
            config.target_version = Some(*target_version);
        }
        if let Some(unfixable) = &self.unfixable {
            config.unfixable = Some(unfixable.clone());
        }
        if let Some(update_check) = &self.update_check {
            config.update_check = Some(*update_check);
        }
        // Special-case: `extend_ignore` and `extend_select` are parallel arrays, so
        // push an empty array if only one of the two is provided.
        match (&self.extend_ignore, &self.extend_select) {
            (Some(extend_ignore), Some(extend_select)) => {
                config.extend_ignore.push(extend_ignore.clone());
                config.extend_select.push(extend_select.clone());
            }
            (Some(extend_ignore), None) => {
                config.extend_ignore.push(extend_ignore.clone());
                config.extend_select.push(Vec::new());
            }
            (None, Some(extend_select)) => {
                config.extend_ignore.push(Vec::new());
                config.extend_select.push(extend_select.clone());
            }
            (None, None) => {}
        }
    }
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
    let mut per_file_ignores: FxHashMap<String, Vec<RuleCodePrefix>> = FxHashMap::default();
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
