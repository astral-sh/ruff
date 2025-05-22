use crate::logging::Verbosity;
use crate::python_version::PythonVersion;
use clap::error::ErrorKind;
use clap::{ArgAction, ArgMatches, Error, Parser};
use ruff_db::system::SystemPathBuf;
use ty_project::combine::Combine;
use ty_project::metadata::options::{EnvironmentOptions, Options, TerminalOptions};
use ty_project::metadata::value::{RangedValue, RelativePathBuf, ValueSource};
use ty_python_semantic::lint;

#[derive(Debug, Parser)]
#[command(author, name = "ty", about = "An extremely fast Python type checker.")]
#[command(long_version = crate::version::version())]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub(crate) enum Command {
    /// Check a project for type errors.
    Check(CheckCommand),

    /// Start the language server
    Server,

    /// Display ty's version
    Version,

    /// Generate shell completion
    #[clap(hide = true)]
    GenerateShellCompletion { shell: clap_complete_command::Shell },
}

#[derive(Debug, Parser)]
pub(crate) struct CheckCommand {
    /// List of files or directories to check.
    #[clap(
        help = "List of files or directories to check [default: the project root]",
        value_name = "PATH"
    )]
    pub paths: Vec<SystemPathBuf>,

    /// Run the command within the given project directory.
    ///
    /// All `pyproject.toml` files will be discovered by walking up the directory tree from the given project directory,
    /// as will the project's virtual environment (`.venv`) unless the `venv-path` option is set.
    ///
    /// Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.
    #[arg(long, value_name = "PROJECT")]
    pub(crate) project: Option<SystemPathBuf>,

    /// Path to the Python environment.
    ///
    /// ty uses the Python environment to resolve type information and third-party dependencies.
    ///
    /// If not specified, ty will attempt to infer it from the `VIRTUAL_ENV` environment variable or
    /// discover a `.venv` directory in the project root or working directory.
    ///
    /// If a path to a Python interpreter is provided, e.g., `.venv/bin/python3`, ty will attempt to
    /// find an environment two directories up from the interpreter's path, e.g., `.venv`. At this
    /// time, ty does not invoke the interpreter to determine the location of the environment. This
    /// means that ty will not resolve dynamic executables such as a shim.
    ///
    /// ty will search in the resolved environments's `site-packages` directories for type
    /// information and third-party imports.
    #[arg(long, value_name = "PATH")]
    pub(crate) python: Option<SystemPathBuf>,

    /// Custom directory to use for stdlib typeshed stubs.
    #[arg(long, value_name = "PATH", alias = "custom-typeshed-dir")]
    pub(crate) typeshed: Option<SystemPathBuf>,

    /// Additional path to use as a module-resolution source (can be passed multiple times).
    #[arg(long, value_name = "PATH")]
    pub(crate) extra_search_path: Option<Vec<SystemPathBuf>>,

    /// Python version to assume when resolving types.
    ///
    /// The Python version affects allowed syntax, type definitions of the standard library, and
    /// type definitions of first- and third-party modules that are conditional on the Python version.
    ///
    /// By default, the Python version is inferred as the lower bound of the project's
    /// `requires-python` field from the `pyproject.toml`, if available. Otherwise, the latest
    /// stable version supported by ty is used, which is currently 3.13.
    ///
    /// ty will not infer the Python version from the Python environment at this time.
    #[arg(long, value_name = "VERSION", alias = "target-version")]
    pub(crate) python_version: Option<PythonVersion>,

    /// Target platform to assume when resolving types.
    ///
    /// This is used to specialize the type of `sys.platform` and will affect the visibility
    /// of platform-specific functions and attributes. If the value is set to `all`, no
    /// assumptions are made about the target platform. If unspecified, the current system's
    /// platform will be used.
    #[arg(long, value_name = "PLATFORM", alias = "platform")]
    pub(crate) python_platform: Option<String>,

    #[clap(flatten)]
    pub(crate) verbosity: Verbosity,

    #[clap(flatten)]
    pub(crate) rules: RulesArg,

    #[clap(flatten)]
    pub(crate) config: ConfigsArg,

    /// The format to use for printing diagnostic messages.
    #[arg(long)]
    pub(crate) output_format: Option<OutputFormat>,

    /// Control when colored output is used.
    #[arg(long, value_name = "WHEN")]
    pub(crate) color: Option<TerminalColor>,

    /// Use exit code 1 if there are any warning-level diagnostics.
    #[arg(long, conflicts_with = "exit_zero", default_missing_value = "true", num_args=0..1)]
    pub(crate) error_on_warning: Option<bool>,

    /// Always use exit code 0, even when there are error-level diagnostics.
    #[arg(long)]
    pub(crate) exit_zero: bool,

    /// Watch files for changes and recheck files related to the changed files.
    #[arg(long, short = 'W')]
    pub(crate) watch: bool,

    /// Respect file exclusions via `.gitignore` and other standard ignore files.
    /// Use `--no-respect-gitignore` to disable.
    #[arg(
        long,
        overrides_with("no_respect_ignore_files"),
        help_heading = "File selection",
        default_missing_value = "true",
        num_args = 0..1
    )]
    respect_ignore_files: Option<bool>,
    #[clap(long, overrides_with("respect_ignore_files"), hide = true)]
    no_respect_ignore_files: bool,
}

impl CheckCommand {
    pub(crate) fn into_options(self) -> Options {
        let rules = if self.rules.is_empty() {
            None
        } else {
            Some(
                self.rules
                    .into_iter()
                    .map(|(rule, level)| (RangedValue::cli(rule), RangedValue::cli(level)))
                    .collect(),
            )
        };

        // --no-respect-gitignore defaults to false and is set true by CLI flag. If passed, override config file
        // Otherwise, only pass this through if explicitly set (don't default to anything here to
        // make sure that doesn't take precedence over an explicitly-set config file value)
        let respect_ignore_files = self
            .no_respect_ignore_files
            .then_some(false)
            .or(self.respect_ignore_files);
        let options = Options {
            environment: Some(EnvironmentOptions {
                python_version: self
                    .python_version
                    .map(|version| RangedValue::cli(version.into())),
                python_platform: self
                    .python_platform
                    .map(|platform| RangedValue::cli(platform.into())),
                python: self.python.map(RelativePathBuf::cli),
                typeshed: self.typeshed.map(RelativePathBuf::cli),
                extra_paths: self.extra_search_path.map(|extra_search_paths| {
                    extra_search_paths
                        .into_iter()
                        .map(RelativePathBuf::cli)
                        .collect()
                }),
            }),
            terminal: Some(TerminalOptions {
                output_format: self
                    .output_format
                    .map(|output_format| RangedValue::cli(output_format.into())),
                error_on_warning: self.error_on_warning,
            }),
            rules,
            respect_ignore_files,
            ..Default::default()
        };
        // Merge with options passed in via --config
        options.combine(self.config.into_options().unwrap_or_default())
    }
}

/// A list of rules to enable or disable with a given severity.
///
/// This type is used to parse the `--error`, `--warn`, and `--ignore` arguments
/// while preserving the order in which they were specified (arguments last override previous severities).
#[derive(Debug)]
pub(crate) struct RulesArg(Vec<(String, lint::Level)>);

impl RulesArg {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn into_iter(self) -> impl Iterator<Item = (String, lint::Level)> {
        self.0.into_iter()
    }
}

impl clap::FromArgMatches for RulesArg {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, Error> {
        let mut rules = Vec::new();

        for (level, arg_id) in [
            (lint::Level::Ignore, "ignore"),
            (lint::Level::Warn, "warn"),
            (lint::Level::Error, "error"),
        ] {
            let indices = matches.indices_of(arg_id).into_iter().flatten();
            let levels = matches.get_many::<String>(arg_id).into_iter().flatten();
            rules.extend(
                indices
                    .zip(levels)
                    .map(|(index, rule)| (index, rule, level)),
            );
        }

        // Sort by their index so that values specified later override earlier ones.
        rules.sort_by_key(|(index, _, _)| *index);

        Ok(Self(
            rules
                .into_iter()
                .map(|(_, rule, level)| (rule.to_owned(), level))
                .collect(),
        ))
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), Error> {
        self.0 = Self::from_arg_matches(matches)?.0;
        Ok(())
    }
}

impl clap::Args for RulesArg {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        const HELP_HEADING: &str = "Enabling / disabling rules";

        cmd.arg(
            clap::Arg::new("error")
                .long("error")
                .action(ArgAction::Append)
                .help("Treat the given rule as having severity 'error'. Can be specified multiple times.")
                .value_name("RULE")
                .help_heading(HELP_HEADING),
        )
        .arg(
            clap::Arg::new("warn")
                .long("warn")
                .action(ArgAction::Append)
                .help("Treat the given rule as having severity 'warn'. Can be specified multiple times.")
                .value_name("RULE")
                .help_heading(HELP_HEADING),
        )
        .arg(
            clap::Arg::new("ignore")
                .long("ignore")
                .action(ArgAction::Append)
                .help("Disables the rule. Can be specified multiple times.")
                .value_name("RULE")
                .help_heading(HELP_HEADING),
        )
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        Self::augment_args(cmd)
    }
}

/// The diagnostic output format.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Print diagnostics verbosely, with context and helpful hints \[default\].
    ///
    /// Diagnostic messages may include additional context and
    /// annotations on the input to help understand the message.
    #[default]
    #[value(name = "full")]
    Full,
    /// Print diagnostics concisely, one per line.
    ///
    /// This will guarantee that each diagnostic is printed on
    /// a single line. Only the most important or primary aspects
    /// of the diagnostic are included. Contextual information is
    /// dropped.
    #[value(name = "concise")]
    Concise,
}

impl From<OutputFormat> for ruff_db::diagnostic::DiagnosticFormat {
    fn from(format: OutputFormat) -> ruff_db::diagnostic::DiagnosticFormat {
        match format {
            OutputFormat::Full => Self::Full,
            OutputFormat::Concise => Self::Concise,
        }
    }
}

/// Control when colored output is used.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub(crate) enum TerminalColor {
    /// Display colors if the output goes to an interactive terminal.
    #[default]
    Auto,

    /// Always display colors.
    Always,

    /// Never display colors.
    Never,
}
/// A TOML `<KEY> = <VALUE>` pair
/// (such as you might find in a `ty.toml` configuration file)
/// overriding a specific configuration option.
/// Overrides of individual settings using this option always take precedence
/// over all configuration files.
#[derive(Debug, Clone)]
pub(crate) struct ConfigsArg(Option<Options>);

impl clap::FromArgMatches for ConfigsArg {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, Error> {
        let combined = matches
            .get_many::<String>("config")
            .into_iter()
            .flatten()
            .map(|s| {
                Options::from_toml_str(s, ValueSource::Cli)
                    .map_err(|err| Error::raw(ErrorKind::InvalidValue, err.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .reduce(|acc, item| item.combine(acc));
        Ok(Self(combined))
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), Error> {
        self.0 = Self::from_arg_matches(matches)?.0;
        Ok(())
    }
}

impl clap::Args for ConfigsArg {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("CONFIG_OPTION")
                .help("A TOML `<KEY> = <VALUE>` pair")
                .action(ArgAction::Append),
        )
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        Self::augment_args(cmd)
    }
}

impl ConfigsArg {
    pub(crate) fn into_options(self) -> Option<Options> {
        self.0
    }
}
