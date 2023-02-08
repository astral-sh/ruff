#![allow(dead_code)]

use std::path::PathBuf;

use clap::{command, Parser};
use regex::Regex;
use ruff::logging::LogLevel;
use ruff::registry::Rule;
use ruff::resolver::ConfigProcessor;
use ruff::settings::configuration::RuleSelection;
use ruff::settings::types::{
    FilePattern, PatternPrefixPair, PerFileIgnore, PythonVersion, SerializationFormat,
};
use ruff::RuleSelector;
use rustc_hash::FxHashMap;

#[derive(Debug, Parser)]
#[command(
    author,
    name = "ruff",
    about = "Ruff: An extremely fast Python linter.",
    after_help = "For help with a specific command, see: `ruff help <command>`."
)]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
    #[clap(flatten)]
    pub log_level_args: LogLevelArgs,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Run Ruff on the given files or directories (default).
    Check(CheckArgs),
    /// Explain a rule.
    #[clap(alias = "--explain")]
    Rule {
        #[arg(value_parser=Rule::from_code)]
        rule: Rule,

        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: HelpFormat,
    },
    /// List all supported upstream linters
    Linter {
        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: HelpFormat,
    },
    /// Clear any caches in the current directory and any subdirectories.
    #[clap(alias = "--clean")]
    Clean,
    /// Generate shell completion.
    #[clap(alias = "--generate-shell-completion", hide = true)]
    GenerateShellCompletion { shell: clap_complete_command::Shell },
}

#[derive(Debug, clap::Args)]
#[allow(clippy::struct_excessive_bools, clippy::module_name_repetitions)]
pub struct CheckArgs {
    /// List of files or directories to check.
    pub files: Vec<PathBuf>,
    /// Attempt to automatically fix lint violations.
    #[arg(long, overrides_with("no_fix"))]
    fix: bool,
    #[clap(long, overrides_with("fix"), hide = true)]
    no_fix: bool,
    /// Show violations with source code.
    #[arg(long, overrides_with("no_show_source"))]
    show_source: bool,
    #[clap(long, overrides_with("show_source"), hide = true)]
    no_show_source: bool,
    /// Avoid writing any fixed files back; instead, output a diff for each
    /// changed file to stdout.
    #[arg(long)]
    pub diff: bool,
    /// Run in watch mode by re-running whenever files change.
    #[arg(short, long)]
    pub watch: bool,
    /// Fix any fixable lint violations, but don't report on leftover
    /// violations. Implies `--fix`.
    #[arg(long, overrides_with("no_fix_only"))]
    fix_only: bool,
    #[clap(long, overrides_with("fix_only"), hide = true)]
    no_fix_only: bool,
    /// Output serialization format for violations.
    #[arg(long, value_enum, env = "RUFF_FORMAT")]
    pub format: Option<SerializationFormat>,
    /// Path to the `pyproject.toml` or `ruff.toml` file to use for
    /// configuration.
    #[arg(long, conflicts_with = "isolated")]
    pub config: Option<PathBuf>,
    /// Comma-separated list of rule codes to enable (or ALL, to enable all
    /// rules).
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help_heading = "Rule selection"
    )]
    pub select: Option<Vec<RuleSelector>>,
    /// Comma-separated list of rule codes to disable.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help_heading = "Rule selection"
    )]
    pub ignore: Option<Vec<RuleSelector>>,
    /// Like --select, but adds additional rule codes on top of the selected
    /// ones.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help_heading = "Rule selection"
    )]
    pub extend_select: Option<Vec<RuleSelector>>,
    /// Like --ignore. (Deprecated: You can just use --ignore instead.)
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help_heading = "Rule selection",
        hide = true
    )]
    pub extend_ignore: Option<Vec<RuleSelector>>,
    /// List of mappings from file pattern to code to exclude
    #[arg(long, value_delimiter = ',', help_heading = "Rule selection")]
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    /// List of paths, used to omit files and/or directories from analysis.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "FILE_PATTERN",
        help_heading = "File selection"
    )]
    pub exclude: Option<Vec<FilePattern>>,
    /// Like --exclude, but adds additional files and directories on top of
    /// those already excluded.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "FILE_PATTERN",
        help_heading = "File selection"
    )]
    pub extend_exclude: Option<Vec<FilePattern>>,
    /// List of rule codes to treat as eligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help_heading = "Rule selection"
    )]
    pub fixable: Option<Vec<RuleSelector>>,
    /// List of rule codes to treat as ineligible for autofix. Only applicable
    /// when autofix itself is enabled (e.g., via `--fix`).
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        help_heading = "Rule selection"
    )]
    pub unfixable: Option<Vec<RuleSelector>>,
    /// Respect file exclusions via `.gitignore` and other standard ignore
    /// files.
    #[arg(
        long,
        overrides_with("no_respect_gitignore"),
        help_heading = "File selection"
    )]
    respect_gitignore: bool,
    #[clap(long, overrides_with("respect_gitignore"), hide = true)]
    no_respect_gitignore: bool,
    /// Enforce exclusions, even for paths passed to Ruff directly on the
    /// command-line.
    #[arg(
        long,
        overrides_with("no_force_exclude"),
        help_heading = "File selection"
    )]
    force_exclude: bool,
    #[clap(long, overrides_with("force_exclude"), hide = true)]
    no_force_exclude: bool,
    /// The minimum Python version that should be supported.
    #[arg(long, help_heading = "Rule configuration")]
    pub target_version: Option<PythonVersion>,
    /// Set the line-length for length-associated rules and automatic
    /// formatting.
    #[arg(long, help_heading = "Rule configuration")]
    pub line_length: Option<usize>,
    /// Regular expression matching the name of dummy variables.
    #[arg(long, help_heading = "Rule configuration")]
    pub dummy_variable_rgx: Option<Regex>,
    /// Disable cache reads.
    #[arg(short, long, help_heading = "Miscellaneous")]
    pub no_cache: bool,
    /// Ignore all configuration files.
    #[arg(long, conflicts_with = "config", help_heading = "Miscellaneous")]
    pub isolated: bool,
    /// Path to the cache directory.
    #[arg(long, env = "RUFF_CACHE_DIR", help_heading = "Miscellaneous")]
    pub cache_dir: Option<PathBuf>,
    /// The name of the file when passing it through stdin.
    #[arg(long, help_heading = "Miscellaneous")]
    pub stdin_filename: Option<PathBuf>,
    /// Exit with status code "0", even upon detecting lint violations.
    #[arg(
        short,
        long,
        help_heading = "Miscellaneous",
        conflicts_with = "exit_non_zero_on_fix"
    )]
    pub exit_zero: bool,
    /// Exit with a non-zero status code if any files were modified via
    /// autofix, even if no lint violations remain.
    #[arg(long, help_heading = "Miscellaneous", conflicts_with = "exit_zero")]
    pub exit_non_zero_on_fix: bool,
    /// Does nothing and will be removed in the future.
    #[arg(
        long,
        overrides_with("no_update_check"),
        help_heading = "Miscellaneous",
        hide = true
    )]
    update_check: bool,
    #[clap(long, overrides_with("update_check"), hide = true)]
    no_update_check: bool,
    /// Show counts for every rule with at least one violation.
    #[arg(
        long,
        // Unsupported default-command arguments.
        conflicts_with = "diff",
        conflicts_with = "show_source",
        conflicts_with = "watch",
    )]
    pub statistics: bool,
    /// Enable automatic additions of `noqa` directives to failing lines.
    #[arg(
        long,
        // conflicts_with = "add_noqa",
        conflicts_with = "show_files",
        conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "statistics",
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
        conflicts_with = "fix",
    )]
    pub add_noqa: bool,
    /// See the files Ruff will be run against with the current settings.
    #[arg(
        long,
        // Fake subcommands.
        conflicts_with = "add_noqa",
        // conflicts_with = "show_files",
        conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "statistics",
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub show_files: bool,
    /// See the settings Ruff will use to lint a given Python file.
    #[arg(
        long,
        // Fake subcommands.
        conflicts_with = "add_noqa",
        conflicts_with = "show_files",
        // conflicts_with = "show_settings",
        // Unsupported default-command arguments.
        conflicts_with = "statistics",
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub show_settings: bool,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum HelpFormat {
    Text,
    Json,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, clap::Args)]
pub struct LogLevelArgs {
    /// Enable verbose logging.
    #[arg(
        short,
        long,
        global = true,
        group = "verbosity",
        help_heading = "Log levels"
    )]
    pub verbose: bool,
    /// Print lint violations, but nothing else.
    #[arg(
        short,
        long,
        global = true,
        group = "verbosity",
        help_heading = "Log levels"
    )]
    pub quiet: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting
    /// lint violations).
    #[arg(
        short,
        long,
        global = true,
        group = "verbosity",
        help_heading = "Log levels"
    )]
    pub silent: bool,
}

impl From<&LogLevelArgs> for LogLevel {
    fn from(args: &LogLevelArgs) -> Self {
        if args.silent {
            Self::Silent
        } else if args.quiet {
            Self::Quiet
        } else if args.verbose {
            Self::Verbose
        } else {
            Self::Default
        }
    }
}

impl CheckArgs {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(self) -> (Arguments, Overrides) {
        (
            Arguments {
                add_noqa: self.add_noqa,
                config: self.config,
                diff: self.diff,
                exit_zero: self.exit_zero,
                exit_non_zero_on_fix: self.exit_non_zero_on_fix,
                files: self.files,
                isolated: self.isolated,
                no_cache: self.no_cache,
                show_files: self.show_files,
                show_settings: self.show_settings,
                statistics: self.statistics,
                stdin_filename: self.stdin_filename,
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
    pub config: Option<PathBuf>,
    pub diff: bool,
    pub exit_zero: bool,
    pub exit_non_zero_on_fix: bool,
    pub files: Vec<PathBuf>,
    pub isolated: bool,
    pub no_cache: bool,
    pub show_files: bool,
    pub show_settings: bool,
    pub statistics: bool,
    pub stdin_filename: Option<PathBuf>,
    pub watch: bool,
}

/// CLI settings that function as configuration overrides.
#[derive(Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Overrides {
    pub dummy_variable_rgx: Option<Regex>,
    pub exclude: Option<Vec<FilePattern>>,
    pub extend_exclude: Option<Vec<FilePattern>>,
    pub extend_ignore: Option<Vec<RuleSelector>>,
    pub extend_select: Option<Vec<RuleSelector>>,
    pub fixable: Option<Vec<RuleSelector>>,
    pub ignore: Option<Vec<RuleSelector>>,
    pub line_length: Option<usize>,
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    pub respect_gitignore: Option<bool>,
    pub select: Option<Vec<RuleSelector>>,
    pub show_source: Option<bool>,
    pub target_version: Option<PythonVersion>,
    pub unfixable: Option<Vec<RuleSelector>>,
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
        config.rule_selections.push(RuleSelection {
            select: self.select.clone(),
            ignore: self
                .ignore
                .iter()
                .cloned()
                .chain(self.extend_ignore.iter().cloned().into_iter())
                .flatten()
                .collect(),
            extend_select: self.extend_select.clone().unwrap_or_default(),
            fixable: self.fixable.clone(),
            unfixable: self.unfixable.clone().unwrap_or_default(),
        });
        if let Some(format) = &self.format {
            config.format = Some(*format);
        }
        if let Some(force_exclude) = &self.force_exclude {
            config.force_exclude = Some(*force_exclude);
        }
        if let Some(line_length) = &self.line_length {
            config.line_length = Some(*line_length);
        }
        if let Some(per_file_ignores) = &self.per_file_ignores {
            config.per_file_ignores = Some(collect_per_file_ignores(per_file_ignores.clone()));
        }
        if let Some(respect_gitignore) = &self.respect_gitignore {
            config.respect_gitignore = Some(*respect_gitignore);
        }
        if let Some(show_source) = &self.show_source {
            config.show_source = Some(*show_source);
        }
        if let Some(target_version) = &self.target_version {
            config.target_version = Some(*target_version);
        }
        if let Some(update_check) = &self.update_check {
            config.update_check = Some(*update_check);
        }
    }
}

/// Convert a list of `PatternPrefixPair` structs to `PerFileIgnore`.
pub fn collect_per_file_ignores(pairs: Vec<PatternPrefixPair>) -> Vec<PerFileIgnore> {
    let mut per_file_ignores: FxHashMap<String, Vec<RuleSelector>> = FxHashMap::default();
    for pair in pairs {
        per_file_ignores
            .entry(pair.pattern)
            .or_insert_with(Vec::new)
            .push(pair.prefix);
    }
    per_file_ignores
        .into_iter()
        .map(|(pattern, prefixes)| PerFileIgnore::new(pattern, &prefixes, None))
        .collect()
}
