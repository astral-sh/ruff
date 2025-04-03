use std::cmp::Ordering;
use std::fmt::{Formatter, Write as _};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::bail;
use clap::builder::{TypedValueParser, ValueParserFactory};
use clap::{command, Parser, Subcommand};
use colored::Colorize;
use itertools::Itertools;
use path_absolutize::path_dedot;
use regex::Regex;
use ruff_graph::Direction;
use ruff_linter::line_width::LineLength;
use ruff_linter::logging::LogLevel;
use ruff_linter::registry::Rule;
use ruff_linter::settings::types::{
    ExtensionPair, FilePattern, OutputFormat, PatternPrefixPair, PerFileIgnore, PreviewMode,
    PythonVersion, UnsafeFixes,
};
use ruff_linter::{RuleParser, RuleSelector, RuleSelectorParser};
use ruff_python_ast as ast;
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::TextRange;
use ruff_workspace::configuration::{Configuration, RuleSelection};
use ruff_workspace::options::{Options, PycodestyleOptions};
use ruff_workspace::options_base::{OptionEntry, OptionsMetadata};
use ruff_workspace::resolver::ConfigurationTransformer;
use rustc_hash::FxHashMap;
use toml;

use crate::commands::completions::config::{OptionString, OptionStringParser};

/// All configuration options that can be passed "globally",
/// i.e., can be passed to all subcommands
#[derive(Debug, Default, Clone, clap::Args)]
pub struct GlobalConfigArgs {
    #[clap(flatten)]
    log_level_args: LogLevelArgs,
    /// Either a path to a TOML configuration file (`pyproject.toml` or `ruff.toml`),
    /// or a TOML `<KEY> = <VALUE>` pair
    /// (such as you might find in a `ruff.toml` configuration file)
    /// overriding a specific configuration option.
    /// Overrides of individual settings using this option always take precedence
    /// over all configuration files, including configuration files that were also
    /// specified using `--config`.
    #[arg(
        long,
        action = clap::ArgAction::Append,
        value_name = "CONFIG_OPTION",
        value_parser = ConfigArgumentParser,
        global = true,
        help_heading = "Global options",
    )]
    pub config: Vec<SingleConfigArgument>,
    /// Ignore all configuration files.
    //
    // Note: We can't mark this as conflicting with `--config` here
    // as `--config` can be used for specifying configuration overrides
    // as well as configuration files.
    // Specifying a configuration file conflicts with `--isolated`;
    // specifying a configuration override does not.
    // If a user specifies `ruff check --isolated --config=ruff.toml`,
    // we emit an error later on, after the initial parsing by clap.
    #[arg(long, help_heading = "Global options", global = true)]
    pub isolated: bool,
}

impl GlobalConfigArgs {
    pub fn log_level(&self) -> LogLevel {
        LogLevel::from(&self.log_level_args)
    }

    #[must_use]
    fn partition(self) -> (LogLevel, Vec<SingleConfigArgument>, bool) {
        (self.log_level(), self.config, self.isolated)
    }
}

#[derive(Debug, Parser)]
#[command(
    author,
    name = "ruff",
    about = "Ruff: An extremely fast Python linter and code formatter.",
    after_help = "For help with a specific command, see: `ruff help <command>`."
)]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub(crate) command: Command,
    #[clap(flatten)]
    pub(crate) global_options: GlobalConfigArgs,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Run Ruff on the given files or directories.
    Check(CheckCommand),
    /// Explain a rule (or all rules).
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
    },
    /// List or describe the available configuration options.
    Config {
        /// Config key to show
        #[arg(
            value_parser = OptionStringParser,
            hide_possible_values = true
        )]
        option: Option<OptionString>,
        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        output_format: HelpFormat,
    },
    /// List all supported upstream linters.
    Linter {
        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        output_format: HelpFormat,
    },
    /// Clear any caches in the current directory and any subdirectories.
    Clean,
    /// Generate shell completion.
    #[clap(hide = true)]
    GenerateShellCompletion { shell: clap_complete_command::Shell },
    /// Run the Ruff formatter on the given files or directories.
    Format(FormatCommand),
    /// Run the language server.
    Server(ServerCommand),
    /// Run analysis over Python source code.
    #[clap(subcommand)]
    Analyze(AnalyzeCommand),
    /// Display Ruff's version
    Version {
        #[arg(long, value_enum, default_value = "text")]
        output_format: HelpFormat,
    },
}

#[derive(Debug, Subcommand)]
pub enum AnalyzeCommand {
    /// Generate a map of Python file dependencies or dependents.
    Graph(AnalyzeGraphCommand),
}

#[derive(Clone, Debug, clap::Parser)]
pub struct AnalyzeGraphCommand {
    /// List of files or directories to include.
    #[clap(help = "List of files or directories to include [default: .]")]
    files: Vec<PathBuf>,
    /// The direction of the import map. By default, generates a dependency map, i.e., a map from
    /// file to files that it depends on. Use `--direction dependents` to generate a map from file
    /// to files that depend on it.
    #[clap(long, value_enum, default_value_t)]
    direction: Direction,
    /// Attempt to detect imports from string literals.
    #[clap(long)]
    detect_string_imports: bool,
    /// Enable preview mode. Use `--no-preview` to disable.
    #[arg(long, overrides_with("no_preview"))]
    preview: bool,
    #[clap(long, overrides_with("preview"), hide = true)]
    no_preview: bool,
    /// The minimum Python version that should be supported.
    #[arg(long, value_enum)]
    target_version: Option<PythonVersion>,
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
    /// Show an enumeration of all fixed lint violations.
    /// Use `--no-show-fixes` to disable.
    #[arg(long, overrides_with("no_show_fixes"))]
    show_fixes: bool,
    #[clap(long, overrides_with("show_fixes"), hide = true)]
    no_show_fixes: bool,
    /// Avoid writing any fixed files back; instead, output a diff for each changed file to stdout, and exit 0 if there are no diffs.
    /// Implies `--fix-only`.
    #[arg(long, conflicts_with = "show_fixes")]
    pub diff: bool,
    /// Run in watch mode by re-running whenever files change.
    #[arg(short, long)]
    pub watch: bool,
    /// Apply fixes to resolve lint violations, but don't report on, or exit non-zero for, leftover violations. Implies `--fix`.
    /// Use `--no-fix-only` to disable or `--unsafe-fixes` to include unsafe fixes.
    #[arg(long, overrides_with("no_fix_only"))]
    fix_only: bool,
    #[clap(long, overrides_with("fix_only"), hide = true)]
    no_fix_only: bool,
    /// Ignore any `# noqa` comments.
    #[arg(long)]
    ignore_noqa: bool,

    /// Output serialization format for violations.
    /// The default serialization format is "full".
    #[arg(long, value_enum, env = "RUFF_OUTPUT_FORMAT")]
    pub output_format: Option<OutputFormat>,

    /// Specify file to write the linter output to (default: stdout).
    #[arg(short, long, env = "RUFF_OUTPUT_FILE")]
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
    /// Path to the cache directory.
    #[arg(long, env = "RUFF_CACHE_DIR", help_heading = "Miscellaneous")]
    pub cache_dir: Option<PathBuf>,
    /// The name of the file when passing it through stdin.
    #[arg(long, help_heading = "Miscellaneous")]
    pub stdin_filename: Option<PathBuf>,
    /// List of mappings from file extension to language (one of `python`, `ipynb`, `pyi`). For
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
    /// The name of the file when passing it through stdin.
    #[arg(long, help_heading = "Miscellaneous")]
    pub stdin_filename: Option<PathBuf>,
    /// List of mappings from file extension to language (one of `python`, `ipynb`, `pyi`). For
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

    /// Exit with a non-zero status code if any files were modified via format, even if all files were formatted successfully.
    #[arg(long, help_heading = "Miscellaneous", alias = "exit-non-zero-on-fix")]
    pub exit_non_zero_on_format: bool,
}

#[derive(Copy, Clone, Debug, clap::Parser)]
pub struct ServerCommand {
    /// Enable preview mode. Use `--no-preview` to disable.
    ///
    /// This enables unstable server features and turns on the preview mode for the linter
    /// and the formatter.
    #[arg(long, overrides_with("no_preview"))]
    preview: bool,
    #[clap(long, overrides_with("preview"), hide = true)]
    no_preview: bool,
}

impl ServerCommand {
    pub(crate) fn resolve_preview(self) -> Option<bool> {
        resolve_bool_arg(self.preview, self.no_preview)
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum HelpFormat {
    Text,
    Json,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Default, Clone, clap::Args)]
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

/// Configuration-related arguments passed via the CLI.
#[derive(Default)]
pub struct ConfigArguments {
    /// Whether the user specified --isolated on the command line
    pub(crate) isolated: bool,
    /// The logging level to be used, derived from command-line arguments passed
    pub(crate) log_level: LogLevel,
    /// Path to a pyproject.toml or ruff.toml configuration file (etc.).
    /// Either 0 or 1 configuration file paths may be provided on the command line.
    config_file: Option<PathBuf>,
    /// Overrides provided via the `--config "KEY=VALUE"` option.
    /// An arbitrary number of these overrides may be provided on the command line.
    /// These overrides take precedence over all configuration files,
    /// even configuration files that were also specified using `--config`.
    overrides: Configuration,
    /// Overrides provided via dedicated flags such as `--line-length` etc.
    /// These overrides take precedence over all configuration files,
    /// and also over all overrides specified using any `--config "KEY=VALUE"` flags.
    per_flag_overrides: ExplicitConfigOverrides,
}

impl ConfigArguments {
    pub fn config_file(&self) -> Option<&Path> {
        self.config_file.as_deref()
    }

    fn from_cli_arguments(
        global_options: GlobalConfigArgs,
        per_flag_overrides: ExplicitConfigOverrides,
    ) -> anyhow::Result<Self> {
        let (log_level, config_options, isolated) = global_options.partition();
        let mut config_file: Option<PathBuf> = None;
        let mut overrides = Configuration::default();

        for option in config_options {
            match option {
                SingleConfigArgument::SettingsOverride(overridden_option) => {
                    let overridden_option = Arc::try_unwrap(overridden_option)
                        .unwrap_or_else(|option| option.deref().clone());
                    overrides = overrides.combine(Configuration::from_options(
                        overridden_option,
                        None,
                        &path_dedot::CWD,
                    )?);
                }
                SingleConfigArgument::FilePath(path) => {
                    if isolated {
                        bail!(
                            "\
The argument `--config={}` cannot be used with `--isolated`

  tip: You cannot specify a configuration file and also specify `--isolated`,
       as `--isolated` causes ruff to ignore all configuration files.
       For more information, try `--help`.
",
                            path.display()
                        );
                    }
                    if let Some(ref config_file) = config_file {
                        let (first, second) = (config_file.display(), path.display());
                        bail!(
                            "\
You cannot specify more than one configuration file on the command line.

  tip: remove either `--config={first}` or `--config={second}`.
       For more information, try `--help`.
"
                        );
                    }
                    config_file = Some(path);
                }
            }
        }
        Ok(Self {
            isolated,
            log_level,
            config_file,
            overrides,
            per_flag_overrides,
        })
    }
}

impl ConfigurationTransformer for ConfigArguments {
    fn transform(&self, config: Configuration) -> Configuration {
        let with_config_overrides = self.overrides.clone().combine(config);
        self.per_flag_overrides.transform(with_config_overrides)
    }
}

impl CheckCommand {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(
        self,
        global_options: GlobalConfigArgs,
    ) -> anyhow::Result<(CheckArguments, ConfigArguments)> {
        let check_arguments = CheckArguments {
            add_noqa: self.add_noqa,
            diff: self.diff,
            exit_non_zero_on_fix: self.exit_non_zero_on_fix,
            exit_zero: self.exit_zero,
            files: self.files,
            ignore_noqa: self.ignore_noqa,
            no_cache: self.no_cache,
            output_file: self.output_file,
            show_files: self.show_files,
            show_settings: self.show_settings,
            statistics: self.statistics,
            stdin_filename: self.stdin_filename,
            watch: self.watch,
        };

        let cli_overrides = ExplicitConfigOverrides {
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
            respect_gitignore: resolve_bool_arg(self.respect_gitignore, self.no_respect_gitignore),
            select: self.select,
            target_version: self.target_version.map(ast::PythonVersion::from),
            unfixable: self.unfixable,
            // TODO(charlie): Included in `pyproject.toml`, but not inherited.
            cache_dir: self.cache_dir,
            fix: resolve_bool_arg(self.fix, self.no_fix),
            fix_only: resolve_bool_arg(self.fix_only, self.no_fix_only),
            unsafe_fixes: resolve_bool_arg(self.unsafe_fixes, self.no_unsafe_fixes)
                .map(UnsafeFixes::from),
            force_exclude: resolve_bool_arg(self.force_exclude, self.no_force_exclude),
            output_format: self.output_format,
            show_fixes: resolve_bool_arg(self.show_fixes, self.no_show_fixes),
            extension: self.extension,
            ..ExplicitConfigOverrides::default()
        };

        let config_args = ConfigArguments::from_cli_arguments(global_options, cli_overrides)?;
        Ok((check_arguments, config_args))
    }
}

impl FormatCommand {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(
        self,
        global_options: GlobalConfigArgs,
    ) -> anyhow::Result<(FormatArguments, ConfigArguments)> {
        let format_arguments = FormatArguments {
            check: self.check,
            diff: self.diff,
            files: self.files,
            no_cache: self.no_cache,
            stdin_filename: self.stdin_filename,
            range: self.range,
            exit_non_zero_on_format: self.exit_non_zero_on_format,
        };

        let cli_overrides = ExplicitConfigOverrides {
            line_length: self.line_length,
            respect_gitignore: resolve_bool_arg(self.respect_gitignore, self.no_respect_gitignore),
            exclude: self.exclude,
            preview: resolve_bool_arg(self.preview, self.no_preview).map(PreviewMode::from),
            force_exclude: resolve_bool_arg(self.force_exclude, self.no_force_exclude),
            target_version: self.target_version.map(ast::PythonVersion::from),
            cache_dir: self.cache_dir,
            extension: self.extension,
            ..ExplicitConfigOverrides::default()
        };

        let config_args = ConfigArguments::from_cli_arguments(global_options, cli_overrides)?;
        Ok((format_arguments, config_args))
    }
}

impl AnalyzeGraphCommand {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(
        self,
        global_options: GlobalConfigArgs,
    ) -> anyhow::Result<(AnalyzeGraphArgs, ConfigArguments)> {
        let format_arguments = AnalyzeGraphArgs {
            files: self.files,
            direction: self.direction,
        };

        let cli_overrides = ExplicitConfigOverrides {
            detect_string_imports: if self.detect_string_imports {
                Some(true)
            } else {
                None
            },
            preview: resolve_bool_arg(self.preview, self.no_preview).map(PreviewMode::from),
            target_version: self.target_version.map(ast::PythonVersion::from),
            ..ExplicitConfigOverrides::default()
        };

        let config_args = ConfigArguments::from_cli_arguments(global_options, cli_overrides)?;
        Ok((format_arguments, config_args))
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

/// Enumeration of various ways in which a --config CLI flag
/// could be invalid
#[derive(Debug)]
enum InvalidConfigFlagReason {
    InvalidToml(toml::de::Error),
    /// It was valid TOML, but not a valid ruff config file.
    /// E.g. the user tried to select a rule that doesn't exist,
    /// or tried to enable a setting that doesn't exist
    ValidTomlButInvalidRuffSchema(toml::de::Error),
    /// It was a valid ruff config file, but the user tried to pass a
    /// value for `extend` as part of the config override.
    /// `extend` is special, because it affects which config files we look at
    /// in the first place. We currently only parse --config overrides *after*
    /// we've combined them with all the arguments from the various config files
    /// that we found, so trying to override `extend` as part of a --config
    /// override is forbidden.
    ExtendPassedViaConfigFlag,
}

impl InvalidConfigFlagReason {
    const fn description(&self) -> &'static str {
        match self {
            Self::InvalidToml(_) => "The supplied argument is not valid TOML",
            Self::ValidTomlButInvalidRuffSchema(_) => {
                "Could not parse the supplied argument as a `ruff.toml` configuration option"
            }
            Self::ExtendPassedViaConfigFlag => "Cannot include `extend` in a --config flag value",
        }
    }
}

/// Enumeration to represent a single `--config` argument
/// passed via the CLI.
///
/// Using the `--config` flag, users may pass 0 or 1 paths
/// to configuration files and an arbitrary number of
/// "inline TOML" overrides for specific settings.
///
/// For example:
///
/// ```sh
/// ruff check --config "path/to/ruff.toml" --config "extend-select=['E501', 'F841']" --config "lint.per-file-ignores = {'some_file.py' = ['F841']}"
/// ```
#[derive(Clone, Debug)]
pub enum SingleConfigArgument {
    FilePath(PathBuf),
    SettingsOverride(Arc<Options>),
}

#[derive(Clone)]
pub struct ConfigArgumentParser;

impl ValueParserFactory for SingleConfigArgument {
    type Parser = ConfigArgumentParser;

    fn value_parser() -> Self::Parser {
        ConfigArgumentParser
    }
}

impl TypedValueParser for ConfigArgumentParser {
    type Value = SingleConfigArgument;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        // Convert to UTF-8.
        let Some(value) = value.to_str() else {
            // But respect non-UTF-8 paths.
            let path_to_config_file = PathBuf::from(value);
            if path_to_config_file.is_file() {
                return Ok(SingleConfigArgument::FilePath(path_to_config_file));
            }
            return Err(clap::Error::new(clap::error::ErrorKind::InvalidUtf8));
        };

        // Expand environment variables and tildes.
        if let Ok(path_to_config_file) =
            shellexpand::full(value).map(|config| PathBuf::from(&*config))
        {
            if path_to_config_file.is_file() {
                return Ok(SingleConfigArgument::FilePath(path_to_config_file));
            }
        }

        let config_parse_error = match toml::Table::from_str(value) {
            Ok(table) => match table.try_into::<Options>() {
                Ok(option) => {
                    if option.extend.is_none() {
                        return Ok(SingleConfigArgument::SettingsOverride(Arc::new(option)));
                    }
                    InvalidConfigFlagReason::ExtendPassedViaConfigFlag
                }
                Err(underlying_error) => {
                    InvalidConfigFlagReason::ValidTomlButInvalidRuffSchema(underlying_error)
                }
            },
            Err(underlying_error) => InvalidConfigFlagReason::InvalidToml(underlying_error),
        };

        let mut new_error = clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
        if let Some(arg) = arg {
            new_error.insert(
                clap::error::ContextKind::InvalidArg,
                clap::error::ContextValue::String(arg.to_string()),
            );
        }
        new_error.insert(
            clap::error::ContextKind::InvalidValue,
            clap::error::ContextValue::String(value.to_string()),
        );

        let underlying_error = match &config_parse_error {
            InvalidConfigFlagReason::ExtendPassedViaConfigFlag => {
                let tip = config_parse_error.description().into();
                new_error.insert(
                    clap::error::ContextKind::Suggested,
                    clap::error::ContextValue::StyledStrs(vec![tip]),
                );
                return Err(new_error);
            }
            InvalidConfigFlagReason::InvalidToml(underlying_error)
            | InvalidConfigFlagReason::ValidTomlButInvalidRuffSchema(underlying_error) => {
                underlying_error
            }
        };

        // small hack so that multiline tips
        // have the same indent on the left-hand side:
        let tip_indent = " ".repeat("  tip: ".len());

        let mut tip = format!(
            "\
A `--config` flag must either be a path to a `.toml` configuration file
{tip_indent}or a TOML `<KEY> = <VALUE>` pair overriding a specific configuration
{tip_indent}option"
        );

        // Here we do some heuristics to try to figure out whether
        // the user was trying to pass in a path to a configuration file
        // or some inline TOML.
        // We want to display the most helpful error to the user as possible.
        if Path::new(value)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            if !value.contains('=') {
                let _ = write!(
                    &mut tip,
                    "

It looks like you were trying to pass a path to a configuration file.
The path `{value}` does not point to a configuration file"
                );
            }
        } else if let Some((key, value)) = value.split_once('=') {
            let key = key.trim_ascii();
            let value = value.trim_ascii_start();

            match Options::metadata().find(key) {
                Some(OptionEntry::Set(set)) if !value.starts_with('{') => {
                    let prefixed_subfields = set
                        .collect_fields()
                        .iter()
                        .map(|(name, _)| format!("- `{key}.{name}`"))
                        .join("\n");

                    let _ = write!(
                        &mut tip,
                        "

`{key}` is a table of configuration options.
Did you want to override one of the table's subkeys?

Possible choices:

{prefixed_subfields}"
                    );
                }
                _ => {
                    let _ = write!(
                        &mut tip,
                        "\n\n{}:\n\n{underlying_error}",
                        config_parse_error.description()
                    );
                }
            }
        }
        let tip = tip.trim_end().to_owned().into();

        new_error.insert(
            clap::error::ContextKind::Suggested,
            clap::error::ContextValue::StyledStrs(vec![tip]),
        );

        Err(new_error)
    }
}

/// CLI settings that are distinct from configuration (commands, lists of files,
/// etc.).
#[allow(clippy::struct_excessive_bools)]
pub struct CheckArguments {
    pub add_noqa: bool,
    pub diff: bool,
    pub exit_non_zero_on_fix: bool,
    pub exit_zero: bool,
    pub files: Vec<PathBuf>,
    pub ignore_noqa: bool,
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
    pub files: Vec<PathBuf>,
    pub stdin_filename: Option<PathBuf>,
    pub range: Option<FormatRange>,
    pub exit_non_zero_on_format: bool,
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

/// CLI settings that are distinct from configuration (commands, lists of files, etc.).
#[derive(Clone, Debug)]
pub struct AnalyzeGraphArgs {
    pub files: Vec<PathBuf>,
    pub direction: Direction,
}

/// Configuration overrides provided via dedicated CLI flags:
/// `--line-length`, `--respect-gitignore`, etc.
#[derive(Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
struct ExplicitConfigOverrides {
    dummy_variable_rgx: Option<Regex>,
    exclude: Option<Vec<FilePattern>>,
    extend_exclude: Option<Vec<FilePattern>>,
    extend_fixable: Option<Vec<RuleSelector>>,
    extend_ignore: Option<Vec<RuleSelector>>,
    extend_select: Option<Vec<RuleSelector>>,
    extend_unfixable: Option<Vec<RuleSelector>>,
    fixable: Option<Vec<RuleSelector>>,
    ignore: Option<Vec<RuleSelector>>,
    line_length: Option<LineLength>,
    per_file_ignores: Option<Vec<PatternPrefixPair>>,
    extend_per_file_ignores: Option<Vec<PatternPrefixPair>>,
    preview: Option<PreviewMode>,
    respect_gitignore: Option<bool>,
    select: Option<Vec<RuleSelector>>,
    target_version: Option<ast::PythonVersion>,
    unfixable: Option<Vec<RuleSelector>>,
    // TODO(charlie): Captured in pyproject.toml as a default, but not part of `Settings`.
    cache_dir: Option<PathBuf>,
    fix: Option<bool>,
    fix_only: Option<bool>,
    unsafe_fixes: Option<UnsafeFixes>,
    force_exclude: Option<bool>,
    output_format: Option<OutputFormat>,
    show_fixes: Option<bool>,
    extension: Option<Vec<ExtensionPair>>,
    detect_string_imports: Option<bool>,
}

impl ConfigurationTransformer for ExplicitConfigOverrides {
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
        if let Some(detect_string_imports) = &self.detect_string_imports {
            config.analyze.detect_string_imports = Some(*detect_string_imports);
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
