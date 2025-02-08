use crate::logging::Verbosity;
use crate::python_version::PythonVersion;
use clap::{ArgAction, ArgMatches, Error, Parser};
use red_knot_project::metadata::options::{EnvironmentOptions, Options};
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
    /// Run the command within the given project directory.
    ///
    /// All `pyproject.toml` files will be discovered by walking up the directory tree from the given project directory,
    /// as will the project's virtual environment (`.venv`) unless the `venv-path` option is set.
    ///
    /// Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.
    #[arg(long, value_name = "PROJECT")]
    pub(crate) project: Option<SystemPathBuf>,

    /// Path to the virtual environment the project uses.
    ///
    /// If provided, red-knot will use the `site-packages` directory of this virtual environment
    /// to resolve type information for the project's third-party dependencies.
    #[arg(long, value_name = "PATH")]
    pub(crate) venv_path: Option<SystemPathBuf>,

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

    /// Whether to output metrics about type-checking performance. If you provide a path, metrics
    /// will be written to that file. If you provide this option but don't provide a path, metrics
    /// will be written to a file called `metrics.json` in the current directory. We will _append_
    /// metrics to the file if it already exists.
    #[arg(long, value_name = "PATH", default_missing_value="metrics.json", num_args=0..=1)]
    pub(crate) metrics: Option<SystemPathBuf>,

    #[clap(flatten)]
    pub(crate) rules: RulesArg,

    /// Use exit code 1 if there are any warning-level diagnostics.
    #[arg(long, conflicts_with = "exit_zero")]
    pub(crate) error_on_warning: bool,

    /// Always use exit code 0, even when there are error-level diagnostics.
    #[arg(long)]
    pub(crate) exit_zero: bool,

    /// Run in watch mode by re-running whenever files change.
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
                venv_path: self.venv_path.map(RelativePathBuf::cli),
                typeshed: self.typeshed.map(RelativePathBuf::cli),
                extra_paths: self.extra_search_path.map(|extra_search_paths| {
                    extra_search_paths
                        .into_iter()
                        .map(RelativePathBuf::cli)
                        .collect()
                }),
                ..EnvironmentOptions::default()
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
