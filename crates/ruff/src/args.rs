use std::path::PathBuf;

use anyhow::anyhow;
use clap::builder::{TypedValueParser, ValueParserFactory};
use clap::{command, Parser};
use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashMap;
use toml;

use ruff_linter::line_width::LineLength;
use ruff_linter::logging::LogLevel;
use ruff_linter::registry::Rule;
use ruff_linter::settings::types::{
    ExtensionPair, FilePattern, PatternPrefixPair, PerFileIgnore, PreviewMode, PythonVersion,
    SerializationFormat, UnsafeFixes,
};
use ruff_linter::{RuleParser, RuleSelector, RuleSelectorParser};
use ruff_workspace::configuration::{Configuration, RuleSelection};
use ruff_workspace::options::{Options, PycodestyleOptions};
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
    /// Either a path to a TOML configuration file (`pyproject.toml` or `ruff.toml`),
    /// or "inline TOML" providing configuration overrides from the command line.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "CONFIG_OPTION",
        value_parser = ConfigOptionParser,
    )]
    pub config: Option<Vec<ConfigOption>>,
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
    #[arg(long, help_heading = "Miscellaneous")]
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
    /// Either a path to a TOML configuration file (`pyproject.toml` or `ruff.toml`),
    /// or "inline TOML" providing configuration overrides from the command line.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "CONFIG_OPTION",
        value_parser = ConfigOptionParser,
    )]
    pub config: Option<Vec<ConfigOption>>,

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
    #[arg(long, help_heading = "Miscellaneous")]
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

pub struct ConfigArgs {
    /// Path to a pyproject.toml or ruff.toml config file (etc.), if provided
    config_file: Option<PathBuf>,
    /// Overrides provided via the `--config FOO=BAR` option on the CLI
    config_overrides: Configuration,
    /// Overrides provided via specific (legacy-ish) flags such as --line-length etc
    cli_overrides: CliOverrides,
}

impl ConfigArgs {
    pub fn config_file(&self) -> Option<&PathBuf> {
        self.config_file.as_ref()
    }

    fn with_no_config_flag_provided(cli_overrides: CliOverrides) -> Self {
        Self {
            cli_overrides,
            config_file: None,
            config_overrides: Configuration::default(),
        }
    }

    fn from_cli_options(
        config_options: Option<Vec<ConfigOption>>,
        cli_overrides: CliOverrides,
        isolated: bool,
    ) -> Result<Self, anyhow::Error> {
        let mut new = Self::with_no_config_flag_provided(cli_overrides);

        let Some(options) = config_options else {
            return Ok(new);
        };

        for option in options {
            match option {
                ConfigOption::ConfigOverride(overridden_option) => {
                    new.config_overrides = new.config_overrides.combine(
                        Configuration::from_options(*overridden_option, &path_dedot::CWD)?,
                    );
                }
                ConfigOption::PathToConfigFile(path) => {
                    if isolated {
                        let context = format!(
                            "Both `--isolated` and `--config={}` were specified on the command line",
                            path.display()
                        );
                        let error = anyhow!(
                            "Cannot specify `--isolated` and also specify a configuration file"
                        )
                        .context(context);
                        return Err(error);
                    }
                    match new.config_file {
                        None => new.config_file = Some(path),
                        Some(ref config_file) => {
                            if config_file == &path {
                                // I'm not sure why you'd specify the same config file twice,
                                // but it seems reasonable to let it pass if we do encounter that
                                continue;
                            }
                            let (first, second) = (config_file.display(), path.display());
                            return Err(anyhow!("Cannot specify more than one configuration file on the command line")
                                .context(format!("Both `--config={first}` and `--config={second}` were specified")));
                        }
                    }
                }
            }
        }
        Ok(new)
    }
}

impl ConfigurationTransformer for ConfigArgs {
    fn transform(&self, config: Configuration) -> Configuration {
        self.cli_overrides
            .transform(self.config_overrides.transform(config))
    }
}

impl CheckCommand {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(self) -> Result<(CheckArguments, ConfigArgs), anyhow::Error> {
        let check_arguments = CheckArguments {
            add_noqa: self.add_noqa,
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
        };

        let cli_overrides = CliOverrides {
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
            show_source: resolve_bool_arg(self.show_source, self.no_show_source),
            target_version: self.target_version,
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
        };

        let config_args = ConfigArgs::from_cli_options(self.config, cli_overrides, self.isolated)?;
        Ok((check_arguments, config_args))
    }
}

impl FormatCommand {
    /// Partition the CLI into command-line arguments and configuration
    /// overrides.
    pub fn partition(self) -> Result<(FormatArguments, ConfigArgs), anyhow::Error> {
        let format_args = FormatArguments {
            check: self.check,
            diff: self.diff,
            files: self.files,
            isolated: self.isolated,
            no_cache: self.no_cache,
            stdin_filename: self.stdin_filename,
        };

        let cli_overrides = CliOverrides {
            line_length: self.line_length,
            respect_gitignore: resolve_bool_arg(self.respect_gitignore, self.no_respect_gitignore),
            exclude: self.exclude,
            preview: resolve_bool_arg(self.preview, self.no_preview).map(PreviewMode::from),
            force_exclude: resolve_bool_arg(self.force_exclude, self.no_force_exclude),
            target_version: self.target_version,
            cache_dir: self.cache_dir,
            extension: self.extension,

            // Unsupported on the formatter CLI, but required on `Overrides`.
            ..CliOverrides::default()
        };

        let config_args = ConfigArgs::from_cli_options(self.config, cli_overrides, self.isolated)?;
        Ok((format_args, config_args))
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

/// --config arguments passed via the CLI
/// Users may pass 0 or 1 paths to a configuration file,
/// and an arbitrary number of "inline TOML" options
///
/// For example:
///
/// ```
/// ruff check --config "path/to/pyproject.toml" --config "max-line-length=90" --config "isort.case-sensitive=false"
/// ```
#[derive(Clone, Debug)]
pub enum ConfigOption {
    PathToConfigFile(PathBuf),
    ConfigOverride(Box<Options>),
}

#[derive(Clone)]
pub struct ConfigOptionParser;

impl ValueParserFactory for ConfigOption {
    type Parser = ConfigOptionParser;

    fn value_parser() -> Self::Parser {
        ConfigOptionParser
    }
}

impl TypedValueParser for ConfigOptionParser {
    type Value = ConfigOption;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let value = value
            .to_str()
            .ok_or_else(|| clap::Error::new(clap::error::ErrorKind::InvalidUtf8))?;
        let path_to_config_file = PathBuf::from(value);
        if path_to_config_file.exists() {
            return Ok(ConfigOption::PathToConfigFile(path_to_config_file));
        }
        if let Ok(option) = toml::from_str(value) {
            Ok(ConfigOption::ConfigOverride(option))
        } else {
            let mut new_error =
                clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
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
            new_error.insert(
                clap::error::ContextKind::Suggested,
                clap::error::ContextValue::StyledStrs(vec![
                    "The `--config` flag must either be a path to a `.toml` configuration file or a TOML string providing configuration overrides".into(),
                    format!("The path `{value}` does not exist on your filesystem").into()
                ]),
            );
            Err(new_error)
        }
    }
}

/// CLI settings that are distinct from configuration (commands, lists of files,
/// etc.).
#[allow(clippy::struct_excessive_bools)]
pub struct CheckArguments {
    pub add_noqa: bool,
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
    pub files: Vec<PathBuf>,
    pub isolated: bool,
    pub stdin_filename: Option<PathBuf>,
}

/// CLI settings that function as configuration overrides.
#[derive(Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
struct CliOverrides {
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
    show_source: Option<bool>,
    target_version: Option<PythonVersion>,
    unfixable: Option<Vec<RuleSelector>>,
    // TODO(charlie): Captured in pyproject.toml as a default, but not part of `Settings`.
    cache_dir: Option<PathBuf>,
    fix: Option<bool>,
    fix_only: Option<bool>,
    unsafe_fixes: Option<UnsafeFixes>,
    force_exclude: Option<bool>,
    output_format: Option<SerializationFormat>,
    show_fixes: Option<bool>,
    extension: Option<Vec<ExtensionPair>>,
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
        if let Some(show_source) = &self.show_source {
            config.show_source = Some(*show_source);
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
