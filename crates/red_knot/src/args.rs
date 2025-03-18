use crate::logging::Verbosity;
use crate::python_version::PythonVersion;
use clap::{ArgAction, ArgMatches, Error, Parser};
use red_knot_project::metadata::options::{EnvironmentOptions, Options, TerminalOptions};
use red_knot_project::metadata::value::{RangedValue, RelativePathBuf};
use red_knot_python_semantic::lint;
use ruff_db::system::SystemPathBuf;

#[derive(Debug, Parser)]
#[command(
    author,
    name = "red-knot",
    about = "An extremely fast Python type checker."
)]
#[command(version)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum Command {
    /// Check a project for type errors.
    Check(CheckCommand),

    /// Start the language server
    Server,

    /// Display Red Knot's version
    Version,
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

    /// Path to the Python installation from which Red Knot resolves type information and third-party dependencies.
    ///
    /// Red Knot will search in the path's `site-packages` directories for type information and
    /// third-party imports.
    ///
    /// This option is commonly used to specify the path to a virtual environment.
    #[arg(long, value_name = "PATH")]
    pub(crate) python: Option<SystemPathBuf>,

    /// Custom directory to use for stdlib typeshed stubs.
    #[arg(long, value_name = "PATH", alias = "custom-typeshed-dir")]
    pub(crate) typeshed: Option<SystemPathBuf>,

    /// Additional path to use as a module-resolution source (can be passed multiple times).
    #[arg(long, value_name = "PATH")]
    pub(crate) extra_search_path: Option<Vec<SystemPathBuf>>,

    /// Python version to assume when resolving types.
    #[arg(long, value_name = "VERSION", alias = "target-version")]
    pub(crate) python_version: Option<PythonVersion>,

    #[clap(flatten)]
    pub(crate) verbosity: Verbosity,

    #[clap(flatten)]
    pub(crate) rules: RulesArg,

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

        Options {
            environment: Some(EnvironmentOptions {
                python_version: self
                    .python_version
                    .map(|version| RangedValue::cli(version.into())),
                python: self.python.map(RelativePathBuf::cli),
                typeshed: self.typeshed.map(RelativePathBuf::cli),
                extra_paths: self.extra_search_path.map(|extra_search_paths| {
                    extra_search_paths
                        .into_iter()
                        .map(RelativePathBuf::cli)
                        .collect()
                }),
                ..EnvironmentOptions::default()
            }),
            terminal: Some(TerminalOptions {
                output_format: self
                    .output_format
                    .map(|output_format| RangedValue::cli(output_format.into())),
                error_on_warning: self.error_on_warning,
            }),
            rules,
            ..Default::default()
        }
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
    /// Print diagnostics verbosely, with context and helpful hints.
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
