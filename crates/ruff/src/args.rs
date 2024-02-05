use std::cmp::Ordering;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{command, Parser};
use colored::Colorize;
use regex::Regex;
use rustc_hash::FxHashMap;

use ruff_linter::line_width::LineLength;
use ruff_linter::logging::LogLevel;
use ruff_linter::registry::Rule;
use ruff_linter::settings::types::{
    ExtensionPair, FilePattern, PatternPrefixPair, PerFileIgnore, PreviewMode, PythonVersion,
    SerializationFormat, UnsafeFixes,
};
use ruff_linter::{warn_user, RuleParser, RuleSelector, RuleSelectorParser};
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::TextRange;
use ruff_workspace::configuration::{Configuration, RuleSelection};
use ruff_workspace::options::PycodestyleOptions;
use ruff_workspace::resolver::ConfigurationTransformer;

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
    Check(CheckCommand),
    /// Explain a rule (or all rules).
    #[clap(alias = "--explain")]
    #[command(group = clap::ArgGroup::new("selector").multiple(false).required(true))]
    Rule {
        /// Rule to explain
        #[arg(value_parser=RuleParser, group = "selector", hide_possible_values = true)]
        rule: Option<Rule>,

        /// Explain all rules
        #[arg(long, conflicts_with = "rule", group = "selector")]
        all: bool,

        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        output_format: HelpFormat,

        /// Output format (Deprecated: Use `--output-format` instead).
        #[arg(long, value_enum, conflicts_with = "output_format", hide = true)]
        format: Option<HelpFormat>,
    },
    /// List or describe the available configuration options.
    Config { option: Option<String> },
    /// List all supported upstream linters.
    Linter {
        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        output_format: HelpFormat,

        /// Output format (Deprecated: Use `--output-format` instead).
        #[arg(long, value_enum, conflicts_with = "output_format", hide = true)]
        format: Option<HelpFormat>,
    },
    /// Clear any caches in the current directory and any subdirectories.
    #[clap(alias = "--clean")]
    Clean,
    /// Generate shell completion.
    #[clap(alias = "--generate-shell-completion", hide = true)]
    GenerateShellCompletion { shell: clap_complete_command::Shell },
    /// Run the Ruff formatter on the given files or directories.
    Format(FormatCommand),
    /// Display Ruff's version
    Version {
        #[arg(long, value_enum, default_value = "text")]
        output_format: HelpFormat,
    },
}

// The `Parser` derive is for ruff_dev, for ruff `Args` would be sufficient
#[derive(Clone, Debug, clap::Parser)]
#[allow(clippy::struct_excessive_bools)]
pub struct CheckCommand {
    /// List of files or directories to check.
    #[clap(help = "List of files or directories to check [default: .]")]
    pub files: Vec<PathBuf>,
    /// Apply fixes to resolve lint violations.
    /// Use `--no-fix` to disable or `--unsafe-fixes` to include unsafe fixes.
    #[arg(long, overrides_with("no_fix"))]
    fix: bool,
    #[clap(long, overrides_with("fix"), hide = true)]
    no_fix: bool,
    /// Include fixes that may not retain the original intent of the code.
    /// Use `--no-unsafe-fixes` to disable.
    #[arg(long, overrides_with("no_unsafe_fixes"))]
    unsafe_fixes: bool,
    #[arg(long, overrides_with("unsafe_fixes"), hide = true)]
    no_unsafe_fixes: bool,
    /// Show violations with source code.
    /// Use `--no-show-source` to disable.
    /// (Deprecated: use `--output-format=full` or `--output-format=concise` instead of `--show-source` and `--no-show-source`, respectively)
    #[arg(long, overrides_with("no_show_source"))]
    show_source: bool,
    #[clap(long, overrides_with("show_source"), hide = true)]
    no_show_source: bool,
    /// Show an enumeration of all fixed lint violations.
    /// Use `--no-show-fixes` to disable.
    #[arg(long, overrides_with("no_show_fixes"))]
    show_fixes: bool,
    #[clap(long, overrides_with("show_fixes"), hide = true)]
    no_show_fixes: bool,
    /// Avoid writing any fixed files back; instead, output a diff for each changed file to stdout. Implies `--fix-only`.
    #[arg(long, conflicts_with = "show_fixes")]
    pub diff: bool,
    /// Run in watch mode by re-running whenever files change.
    #[arg(short, long)]
    pub watch: bool,
    /// Apply fixes to resolve lint violations, but don't report on leftover violations. Implies `--fix`.
    /// Use `--no-fix-only` to disable or `--unsafe-fixes` to include unsafe fixes.
    #[arg(long, overrides_with("no_fix_only"))]
    fix_only: bool,
    #[clap(long, overrides_with("fix_only"), hide = true)]
    no_fix_only: bool,
    /// Ignore any `# noqa` comments.
    #[arg(long)]
    ignore_noqa: bool,

    /// Output serialization format for violations.
    /// The default serialization format is "concise".
    /// In preview mode, the default serialization format is "full".
    #[arg(long, value_enum, env = "RUFF_OUTPUT_FORMAT")]
    pub output_format: Option<SerializationFormat>,

    /// Specify file to write the linter output to (default: stdout).
    #[arg(short, long)]
    pub output_file: Option<PathBuf>,
    /// The minimum Python version that should be supported.
    #[arg(long, value_enum)]
    pub target_version: Option<PythonVersion>,
    /// Enable preview mode; checks will include unstable rules and fixes.
    /// Use `--no-preview` to disable.
    #[arg(long, overrides_with("no_preview"))]
    preview: bool,
    #[clap(long, overrides_with("preview"), hide = true)]
    no_preview: bool,
    /// Path to the `pyproject.toml` or `ruff.toml` file to use for
    /// configuration.
    #[arg(long, conflicts_with = "isolated")]
    pub config: Option<PathBuf>,
    /// Comma-separated list of rule codes to enable (or ALL, to enable all rules).
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide_possible_values = true
    )]
    pub select: Option<Vec<RuleSelector>>,
    /// Comma-separated list of rule codes to disable.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide_possible_values = true
    )]
    pub ignore: Option<Vec<RuleSelector>>,
    /// Like --select, but adds additional rule codes on top of those already specified.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide_possible_values = true
    )]
    pub extend_select: Option<Vec<RuleSelector>>,
    /// Like --ignore. (Deprecated: You can just use --ignore instead.)
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide = true
    )]
    pub extend_ignore: Option<Vec<RuleSelector>>,
    /// List of mappings from file pattern to code to exclude.
    #[arg(long, value_delimiter = ',', help_heading = "Rule selection")]
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    /// Like `--per-file-ignores`, but adds additional ignores on top of those already specified.
    #[arg(long, value_delimiter = ',', help_heading = "Rule selection")]
    pub extend_per_file_ignores: Option<Vec<PatternPrefixPair>>,
    /// List of paths, used to omit files and/or directories from analysis.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "FILE_PATTERN",
        help_heading = "File selection"
    )]
    pub exclude: Option<Vec<FilePattern>>,
    /// Like --exclude, but adds additional files and directories on top of those already excluded.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "FILE_PATTERN",
        help_heading = "File selection"
    )]
    pub extend_exclude: Option<Vec<FilePattern>>,
    /// List of rule codes to treat as eligible for fix. Only applicable when fix itself is enabled (e.g., via `--fix`).
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide_possible_values = true
    )]
    pub fixable: Option<Vec<RuleSelector>>,
    /// List of rule codes to treat as ineligible for fix. Only applicable when fix itself is enabled (e.g., via `--fix`).
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide_possible_values = true
    )]
    pub unfixable: Option<Vec<RuleSelector>>,
    /// Like --fixable, but adds additional rule codes on top of those already specified.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide_possible_values = true
    )]
    pub extend_fixable: Option<Vec<RuleSelector>>,
    /// Like --unfixable. (Deprecated: You can just use --unfixable instead.)
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "RULE_CODE",
        value_parser = RuleSelectorParser,
        help_heading = "Rule selection",
        hide = true
    )]
    pub extend_unfixable: Option<Vec<RuleSelector>>,
    /// Respect file exclusions via `.gitignore` and other standard ignore files.
    /// Use `--no-respect-gitignore` to disable.
    #[arg(
        long,
        overrides_with("no_respect_gitignore"),
        help_heading = "File selection"
    )]
    respect_gitignore: bool,
    #[clap(long, overrides_with("respect_gitignore"), hide = true)]
    no_respect_gitignore: bool,
    /// Enforce exclusions, even for paths passed to Ruff directly on the command-line.
    /// Use `--no-force-exclude` to disable.
    #[arg(
        long,
        overrides_with("no_force_exclude"),
        help_heading = "File selection"
    )]
    force_exclude: bool,
    #[clap(long, overrides_with("force_exclude"), hide = true)]
    no_force_exclude: bool,
    /// Set the line-length for length-associated rules and automatic formatting.
    #[arg(long, help_heading = "Rule configuration", hide = true)]
    pub line_length: Option<LineLength>,
    /// Regular expression matching the name of dummy variables.
    #[arg(long, help_heading = "Rule configuration", hide = true)]
    pub dummy_variable_rgx: Option<Regex>,
    /// Disable cache reads.
    #[arg(short, long, env = "RUFF_NO_CACHE", help_heading = "Miscellaneous")]
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
    /// List of mappings from file extension to language (one of ["python", "ipynb", "pyi"]). For
    /// example, to treat `.ipy` files as IPython notebooks, use `--extension ipy:ipynb`.
    #[arg(long, value_delimiter = ',')]
    pub extension: Option<Vec<ExtensionPair>>,
    /// Exit with status code "0", even upon detecting lint violations.
    #[arg(
        short,
        long,
        help_heading = "Miscellaneous",
        conflicts_with = "exit_non_zero_on_fix"
    )]
    pub exit_zero: bool,
    /// Exit with a non-zero status code if any files were modified via fix, even if no lint violations remain.
    #[arg(long, help_heading = "Miscellaneous", conflicts_with = "exit_zero")]
    pub exit_non_zero_on_fix: bool,
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
        conflicts_with = "ignore_noqa",
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
        conflicts_with = "ignore_noqa",
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
        conflicts_with = "ignore_noqa",
        conflicts_with = "statistics",
        conflicts_with = "stdin_filename",
        conflicts_with = "watch",
    )]
    pub show_settings: bool,
    /// Dev-only argument to show fixes
    #[arg(long, hide = true)]
    pub ecosystem_ci: bool,
}

#[derive(Clone, Debug, clap::Parser)]
#[allow(clippy::struct_excessive_bools)]
pub struct FormatCommand {
    /// List of files or directories to format.
    #[clap(help = "List of files or directories to format [default: .]")]
    pub files: Vec<PathBuf>,
    /// Avoid writing any formatted files back; instead, exit with a non-zero status code if any
    /// files would have been modified, and zero otherwise.
    #[arg(long)]
    pub check: bool,
    /// Avoid writing any formatted files back; instead, exit with a non-zero status code and the
    /// difference between the current file and how the formatted file would look like.
    #[arg(long)]
    pub diff: bool,
    /// Path to the `pyproject.toml` or `ruff.toml` file to use for configuration.
    #[arg(long, conflicts_with = "isolated")]
    pub config: Option<PathBuf>,

    /// Disable cache reads.
    #[arg(short, long, env = "RUFF_NO_CACHE", help_heading = "Miscellaneous")]
    pub no_cache: bool,
    /// Path to the cache directory.
    #[arg(long, env = "RUFF_CACHE_DIR", help_heading = "Miscellaneous")]
    pub cache_dir: Option<PathBuf>,

    /// Respect file exclusions via `.gitignore` and other standard ignore files.
    /// Use `--no-respect-gitignore` to disable.
    #[arg(
        long,
        overrides_with("no_respect_gitignore"),
        help_heading = "File selection"
    )]
    respect_gitignore: bool,
    #[clap(long, overrides_with("respect_gitignore"), hide = true)]
    no_respect_gitignore: bool,
    /// List of paths, used to omit files and/or directories from analysis.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "FILE_PATTERN",
        help_heading = "File selection"
    )]
    pub exclude: Option<Vec<FilePattern>>,

    /// Enforce exclusions, even for paths passed to Ruff directly on the command-line.
    /// Use `--no-force-exclude` to disable.
    #[arg(
        long,
        overrides_with("no_force_exclude"),
        help_heading = "File selection"
    )]
    force_exclude: bool,
    #[clap(long, overrides_with("force_exclude"), hide = true)]
    no_force_exclude: bool,
    /// Set the line-length.
    #[arg(long, help_heading = "Format configuration")]
    pub line_length: Option<LineLength>,
    /// Ignore all configuration files.
    #[arg(long, conflicts_with = "config", help_heading = "Miscellaneous")]
    pub isolated: bool,
    /// The name of the file when passing it through stdin.
    #[arg(long, help_heading = "Miscellaneous")]
    pub stdin_filename: Option<PathBuf>,
    /// List of mappings from file extension to language (one of ["python", "ipynb", "pyi"]). For
    /// example, to treat `.ipy` files as IPython notebooks, use `--extension ipy:ipynb`.
    #[arg(long, value_delimiter = ',')]
    pub extension: Option<Vec<ExtensionPair>>,
    /// The minimum Python version that should be supported.
    #[arg(long, value_enum)]
    pub target_version: Option<PythonVersion>,
    /// Enable preview mode; enables unstable formatting.
    /// Use `--no-preview` to disable.
    #[arg(long, overrides_with("no_preview"))]
    preview: bool,
    #[clap(long, overrides_with("preview"), hide = true)]
    no_preview: bool,

    /// When specified, Ruff will try to only format the code in the given range.
    /// It might be necessary to extend the start backwards or the end forwards, to fully enclose a logical line.
    /// The `<RANGE>` uses the format `<start_line>:<start_column>-<end_line>:<end_column>`.
    ///
    /// - The line and column numbers are 1 based.
    /// - The column specifies the nth-unicode codepoint on that line.
    /// - The end offset is exclusive.
    /// - The column numbers are optional. You can write `--range=1-2` instead of `--range=1:1-2:1`.
    /// - The end position is optional. You can write `--range=2` to format the entire document starting from the second line.
    /// - The start position is optional. You can write `--range=-3` to format the first three lines of the document.
    ///
    /// The option can only be used when formatting a single file. Range formatting of notebooks is unsupported.
    #[clap(long, help_heading = "Editor options", verbatim_doc_comment)]
    pub range: Option<FormatRange>,
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
    /// Print diagnostics, but nothing else.
    #[arg(
        short,
        long,
        global = true,
        group = "verbosity",
        help_heading = "Log levels"
    )]
    pub quiet: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting diagnostics).
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

impl CheckCommand {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(self) -> (CheckArguments, CliOverrides) {
        (
            CheckArguments {
                add_noqa: self.add_noqa,
                config: self.config,
                diff: self.diff,
                ecosystem_ci: self.ecosystem_ci,
                exit_non_zero_on_fix: self.exit_non_zero_on_fix,
                exit_zero: self.exit_zero,
                files: self.files,
                ignore_noqa: self.ignore_noqa,
                isolated: self.isolated,
                no_cache: self.no_cache,
                output_file: self.output_file,
                show_files: self.show_files,
                show_settings: self.show_settings,
                statistics: self.statistics,
                stdin_filename: self.stdin_filename,
                watch: self.watch,
            },
            CliOverrides {
                dummy_variable_rgx: self.dummy_variable_rgx,
                exclude: self.exclude,
                extend_exclude: self.extend_exclude,
                extend_fixable: self.extend_fixable,
                extend_ignore: self.extend_ignore,
                extend_per_file_ignores: self.extend_per_file_ignores,
                extend_select: self.extend_select,
                extend_unfixable: self.extend_unfixable,
                fixable: self.fixable,
                ignore: self.ignore,
                line_length: self.line_length,
                per_file_ignores: self.per_file_ignores,
                preview: resolve_bool_arg(self.preview, self.no_preview).map(PreviewMode::from),
                respect_gitignore: resolve_bool_arg(
                    self.respect_gitignore,
                    self.no_respect_gitignore,
                ),
                select: self.select,
                target_version: self.target_version,
                unfixable: self.unfixable,
                // TODO(charlie): Included in `pyproject.toml`, but not inherited.
                cache_dir: self.cache_dir,
                fix: resolve_bool_arg(self.fix, self.no_fix),
                fix_only: resolve_bool_arg(self.fix_only, self.no_fix_only),
                unsafe_fixes: resolve_bool_arg(self.unsafe_fixes, self.no_unsafe_fixes)
                    .map(UnsafeFixes::from),
                force_exclude: resolve_bool_arg(self.force_exclude, self.no_force_exclude),
                output_format: resolve_output_format(
                    self.output_format,
                    resolve_bool_arg(self.show_source, self.no_show_source),
                    resolve_bool_arg(self.preview, self.no_preview).unwrap_or_default(),
                ),
                show_fixes: resolve_bool_arg(self.show_fixes, self.no_show_fixes),
                extension: self.extension,
            },
        )
    }
}

impl FormatCommand {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(self) -> (FormatArguments, CliOverrides) {
        (
            FormatArguments {
                check: self.check,
                diff: self.diff,
                config: self.config,
                files: self.files,
                isolated: self.isolated,
                no_cache: self.no_cache,
                stdin_filename: self.stdin_filename,
                range: self.range,
            },
            CliOverrides {
                line_length: self.line_length,
                respect_gitignore: resolve_bool_arg(
                    self.respect_gitignore,
                    self.no_respect_gitignore,
                ),
                exclude: self.exclude,
                preview: resolve_bool_arg(self.preview, self.no_preview).map(PreviewMode::from),
                force_exclude: resolve_bool_arg(self.force_exclude, self.no_force_exclude),
                target_version: self.target_version,
                cache_dir: self.cache_dir,
                extension: self.extension,

                // Unsupported on the formatter CLI, but required on `Overrides`.
                ..CliOverrides::default()
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

fn resolve_output_format(
    output_format: Option<SerializationFormat>,
    show_sources: Option<bool>,
    preview: bool,
) -> Option<SerializationFormat> {
    Some(match (output_format, show_sources) {
        (Some(o), None) => o,
        (Some(SerializationFormat::Grouped), Some(true)) => {
            warn_user!("`--show-source` with `--output-format=grouped` is deprecated, and will not show source files. Use `--output-format=full` to show source information.");
            SerializationFormat::Grouped
        }
        (Some(fmt), Some(true)) => {
            warn_user!("The `--show-source` argument is deprecated and has been ignored in favor of `--output-format={fmt}`.");
            fmt
        }
        (Some(fmt), Some(false)) => {
            warn_user!("The `--no-show-source` argument is deprecated and has been ignored in favor of `--output-format={fmt}`.");
            fmt
        }
        (None, Some(true)) => {
            warn_user!("The `--show-source` argument is deprecated. Use `--output-format=full` instead.");
            SerializationFormat::Full
        }
        (None, Some(false)) => {
            warn_user!("The `--no-show-source` argument is deprecated. Use `--output-format=concise` instead.");
            SerializationFormat::Concise
        }
        (None, None) => return None
    }).map(|format| match format {
        SerializationFormat::Text => {
            warn_user!("`--output-format=text` is deprecated. Use `--output-format=full` or `--output-format=concise` instead. `text` will be treated as `{}`.", SerializationFormat::default(preview));
            SerializationFormat::default(preview)
        },
        other => other
    })
}

/// CLI settings that are distinct from configuration (commands, lists of files,
/// etc.).
#[allow(clippy::struct_excessive_bools)]
pub struct CheckArguments {
    pub add_noqa: bool,
    pub config: Option<PathBuf>,
    pub diff: bool,
    pub ecosystem_ci: bool,
    pub exit_non_zero_on_fix: bool,
    pub exit_zero: bool,
    pub files: Vec<PathBuf>,
    pub ignore_noqa: bool,
    pub isolated: bool,
    pub no_cache: bool,
    pub output_file: Option<PathBuf>,
    pub show_files: bool,
    pub show_settings: bool,
    pub statistics: bool,
    pub stdin_filename: Option<PathBuf>,
    pub watch: bool,
}

/// CLI settings that are distinct from configuration (commands, lists of files,
/// etc.).
#[allow(clippy::struct_excessive_bools)]
pub struct FormatArguments {
    pub check: bool,
    pub no_cache: bool,
    pub diff: bool,
    pub config: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub isolated: bool,
    pub stdin_filename: Option<PathBuf>,
    pub range: Option<FormatRange>,
}

/// A text range specified by line and column numbers.
#[derive(Copy, Clone, Debug)]
pub struct FormatRange {
    start: LineColumn,
    end: LineColumn,
}

impl FormatRange {
    /// Converts the line:column range to a byte offset range specific for `source`.
    ///
    /// Returns an empty range if the start range is past the end of `source`.
    pub(super) fn to_text_range(self, source: &str, line_index: &LineIndex) -> TextRange {
        let start_byte_offset = line_index.offset(self.start.line, self.start.column, source);
        let end_byte_offset = line_index.offset(self.end.line, self.end.column, source);

        TextRange::new(start_byte_offset, end_byte_offset)
    }
}

impl FromStr for FormatRange {
    type Err = FormatRangeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (start, end) = value.split_once('-').unwrap_or((value, ""));

        let start = if start.is_empty() {
            LineColumn::default()
        } else {
            start.parse().map_err(FormatRangeParseError::InvalidStart)?
        };

        let end = if end.is_empty() {
            LineColumn {
                line: OneIndexed::MAX,
                column: OneIndexed::MAX,
            }
        } else {
            end.parse().map_err(FormatRangeParseError::InvalidEnd)?
        };

        if start > end {
            return Err(FormatRangeParseError::StartGreaterThanEnd(start, end));
        }

        Ok(FormatRange { start, end })
    }
}

#[derive(Clone, Debug)]
pub enum FormatRangeParseError {
    InvalidStart(LineColumnParseError),
    InvalidEnd(LineColumnParseError),

    StartGreaterThanEnd(LineColumn, LineColumn),
}

impl std::fmt::Display for FormatRangeParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let tip = "  tip:".bold().green();
        match self {
            FormatRangeParseError::StartGreaterThanEnd(start, end) => {
                write!(
                    f,
                    "the start position '{start_invalid}' is greater than the end position '{end_invalid}'.\n  {tip} Try switching start and end: '{end}-{start}'",
                    start_invalid=start.to_string().bold().yellow(),
                    end_invalid=end.to_string().bold().yellow(),
                    start=start.to_string().green().bold(),
                    end=end.to_string().green().bold()
                )
            }
            FormatRangeParseError::InvalidStart(inner) => inner.write(f, true),
            FormatRangeParseError::InvalidEnd(inner) => inner.write(f, false),
        }
    }
}

impl std::error::Error for FormatRangeParseError {}

#[derive(Copy, Clone, Debug)]
pub struct LineColumn {
    pub line: OneIndexed,
    pub column: OneIndexed,
}

impl std::fmt::Display for LineColumn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{line}:{column}", line = self.line, column = self.column)
    }
}

impl Default for LineColumn {
    fn default() -> Self {
        LineColumn {
            line: OneIndexed::MIN,
            column: OneIndexed::MIN,
        }
    }
}

impl PartialOrd for LineColumn {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LineColumn {
    fn cmp(&self, other: &Self) -> Ordering {
        self.line
            .cmp(&other.line)
            .then(self.column.cmp(&other.column))
    }
}

impl PartialEq for LineColumn {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for LineColumn {}

impl FromStr for LineColumn {
    type Err = LineColumnParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (line, column) = value.split_once(':').unwrap_or((value, "1"));

        let line: usize = line.parse().map_err(LineColumnParseError::LineParseError)?;
        let column: usize = column
            .parse()
            .map_err(LineColumnParseError::ColumnParseError)?;

        match (OneIndexed::new(line), OneIndexed::new(column)) {
            (Some(line), Some(column)) => Ok(LineColumn { line, column }),
            (Some(line), None) => Err(LineColumnParseError::ZeroColumnIndex { line }),
            (None, Some(column)) => Err(LineColumnParseError::ZeroLineIndex { column }),
            (None, None) => Err(LineColumnParseError::ZeroLineAndColumnIndex),
        }
    }
}

#[derive(Clone, Debug)]
pub enum LineColumnParseError {
    ZeroLineIndex { column: OneIndexed },
    ZeroColumnIndex { line: OneIndexed },
    ZeroLineAndColumnIndex,
    LineParseError(std::num::ParseIntError),
    ColumnParseError(std::num::ParseIntError),
}

impl LineColumnParseError {
    fn write(&self, f: &mut std::fmt::Formatter, start_range: bool) -> std::fmt::Result {
        let tip = "tip:".bold().green();

        let range = if start_range { "start" } else { "end" };

        match self {
            LineColumnParseError::ColumnParseError(inner) => {
                write!(f, "the {range}s column is not a valid number ({inner})'\n  {tip} The format is 'line:column'.")
            }
            LineColumnParseError::LineParseError(inner) => {
                write!(f, "the {range} line is not a valid number ({inner})\n  {tip} The format is 'line:column'.")
            }
            LineColumnParseError::ZeroColumnIndex { line } => {
                write!(
                    f,
                    "the {range} column is 0, but it should be 1 or greater.\n  {tip} The column numbers start at 1.\n  {tip} Try {suggestion} instead.",
                    suggestion=format!("{line}:1").green().bold()
                )
            }
            LineColumnParseError::ZeroLineIndex { column } => {
                write!(
                    f,
                    "the {range} line is 0, but it should be 1 or greater.\n  {tip} The line numbers start at 1.\n  {tip} Try {suggestion} instead.",
                    suggestion=format!("1:{column}").green().bold()
                )
            }
            LineColumnParseError::ZeroLineAndColumnIndex => {
                write!(
                    f,
                    "the {range} line and column are both 0, but they should be 1 or greater.\n  {tip} The line and column numbers start at 1.\n  {tip} Try {suggestion} instead.",
                    suggestion="1:1".to_string().green().bold()
                )
            }
        }
    }
}

/// CLI settings that function as configuration overrides.
#[derive(Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct CliOverrides {
    pub dummy_variable_rgx: Option<Regex>,
    pub exclude: Option<Vec<FilePattern>>,
    pub extend_exclude: Option<Vec<FilePattern>>,
    pub extend_fixable: Option<Vec<RuleSelector>>,
    pub extend_ignore: Option<Vec<RuleSelector>>,
    pub extend_select: Option<Vec<RuleSelector>>,
    pub extend_unfixable: Option<Vec<RuleSelector>>,
    pub fixable: Option<Vec<RuleSelector>>,
    pub ignore: Option<Vec<RuleSelector>>,
    pub line_length: Option<LineLength>,
    pub per_file_ignores: Option<Vec<PatternPrefixPair>>,
    pub extend_per_file_ignores: Option<Vec<PatternPrefixPair>>,
    pub preview: Option<PreviewMode>,
    pub respect_gitignore: Option<bool>,
    pub select: Option<Vec<RuleSelector>>,
    pub target_version: Option<PythonVersion>,
    pub unfixable: Option<Vec<RuleSelector>>,
    // TODO(charlie): Captured in pyproject.toml as a default, but not part of `Settings`.
    pub cache_dir: Option<PathBuf>,
    pub fix: Option<bool>,
    pub fix_only: Option<bool>,
    pub unsafe_fixes: Option<UnsafeFixes>,
    pub force_exclude: Option<bool>,
    pub output_format: Option<SerializationFormat>,
    pub show_fixes: Option<bool>,
    pub extension: Option<Vec<ExtensionPair>>,
}

impl ConfigurationTransformer for CliOverrides {
    fn transform(&self, mut config: Configuration) -> Configuration {
        if let Some(cache_dir) = &self.cache_dir {
            config.cache_dir = Some(cache_dir.clone());
        }
        if let Some(dummy_variable_rgx) = &self.dummy_variable_rgx {
            config.lint.dummy_variable_rgx = Some(dummy_variable_rgx.clone());
        }
        if let Some(exclude) = &self.exclude {
            config.exclude = Some(exclude.clone());
        }
        if let Some(extend_exclude) = &self.extend_exclude {
            config.extend_exclude.extend(extend_exclude.clone());
        }
        if let Some(extend_per_file_ignores) = &self.extend_per_file_ignores {
            config
                .lint
                .extend_per_file_ignores
                .extend(collect_per_file_ignores(extend_per_file_ignores.clone()));
        }
        if let Some(fix) = &self.fix {
            config.fix = Some(*fix);
        }
        if let Some(fix_only) = &self.fix_only {
            config.fix_only = Some(*fix_only);
        }
        if self.unsafe_fixes.is_some() {
            config.unsafe_fixes = self.unsafe_fixes;
        }
        config.lint.rule_selections.push(RuleSelection {
            select: self.select.clone(),
            ignore: self
                .ignore
                .iter()
                .cloned()
                .chain(self.extend_ignore.iter().cloned())
                .flatten()
                .collect(),
            extend_select: self.extend_select.clone().unwrap_or_default(),
            fixable: self.fixable.clone(),
            unfixable: self
                .unfixable
                .iter()
                .cloned()
                .chain(self.extend_unfixable.iter().cloned())
                .flatten()
                .collect(),
            extend_fixable: self.extend_fixable.clone().unwrap_or_default(),
        });
        if let Some(output_format) = &self.output_format {
            config.output_format = Some(*output_format);
        }
        if let Some(force_exclude) = &self.force_exclude {
            config.force_exclude = Some(*force_exclude);
        }
        if let Some(line_length) = self.line_length {
            config.line_length = Some(line_length);
            config.lint.pycodestyle = Some(PycodestyleOptions {
                max_line_length: Some(line_length),
                ..config.lint.pycodestyle.unwrap_or_default()
            });
        }
        if let Some(preview) = &self.preview {
            config.preview = Some(*preview);
            config.lint.preview = Some(*preview);
            config.format.preview = Some(*preview);
        }
        if let Some(per_file_ignores) = &self.per_file_ignores {
            config.lint.per_file_ignores = Some(collect_per_file_ignores(per_file_ignores.clone()));
        }
        if let Some(respect_gitignore) = &self.respect_gitignore {
            config.respect_gitignore = Some(*respect_gitignore);
        }
        if let Some(show_fixes) = &self.show_fixes {
            config.show_fixes = Some(*show_fixes);
        }
        if let Some(target_version) = &self.target_version {
            config.target_version = Some(*target_version);
        }
        if let Some(extension) = &self.extension {
            config.extension = Some(extension.iter().cloned().collect());
        }

        config
    }
}

/// Convert a list of `PatternPrefixPair` structs to `PerFileIgnore`.
pub fn collect_per_file_ignores(pairs: Vec<PatternPrefixPair>) -> Vec<PerFileIgnore> {
    let mut per_file_ignores: FxHashMap<String, Vec<RuleSelector>> = FxHashMap::default();
    for pair in pairs {
        per_file_ignores
            .entry(pair.pattern)
            .or_default()
            .push(pair.prefix);
    }
    per_file_ignores
        .into_iter()
        .map(|(pattern, prefixes)| PerFileIgnore::new(pattern, &prefixes, None))
        .collect()
}
