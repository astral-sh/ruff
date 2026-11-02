//! Terminal color selection for the Ruff and ty command-line interfaces.

use std::env;
use std::ffi::OsStr;
use std::io::IsTerminal;

use ty_static::EnvVars;

/// When to use color in terminal output.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum TerminalColor {
    /// Use color when writing to a terminal, unless overridden by the environment.
    ///
    /// A non-empty `NO_COLOR` environment variable disables color. Otherwise, a non-empty
    /// `FORCE_COLOR` or `CLICOLOR_FORCE` enables color even when output is redirected.
    ///
    /// By default, terminals use color unless `TERM=dumb`. Set `CLICOLOR` to a non-empty
    /// value to use color even on a dumb terminal. For all four color variables, an empty
    /// string has the same effect as leaving the variable unset, and `0` counts as non-empty.
    #[default]
    Auto,

    /// Always display colors.
    Always,

    /// Never display colors.
    Never,
}

impl TerminalColor {
    /// Resolves the CLI option and environment to a color mode. In the resulting mode,
    /// [`Self::Auto`] means color is enabled only when the output is a terminal.
    ///
    /// Returns an explicit `--color=always` or `--color=never` without reading the environment.
    /// When the option is omitted or set to `auto`, applies the environment variables in
    /// the order documented on [`Self::Auto`].
    pub fn resolve(color: Option<Self>) -> Self {
        let color = color.unwrap_or_default();
        if color != Self::Auto {
            return color;
        }

        if non_empty(EnvVars::NO_COLOR) {
            Self::Never
        } else if non_empty(EnvVars::FORCE_COLOR) || non_empty(EnvVars::CLICOLOR_FORCE) {
            Self::Always
        } else if !non_empty(EnvVars::CLICOLOR)
            && env::var_os(EnvVars::TERM).as_deref() == Some(OsStr::new("dumb"))
        {
            Self::Never
        } else {
            Self::Auto
        }
    }

    /// Returns whether to use color on this stream. Call [`Self::resolve`] first to apply the
    /// environment. In the resolved [`Self::Auto`] mode, this checks whether the stream is a terminal.
    pub fn should_colorize(self, stream: &impl IsTerminal) -> bool {
        self.as_override().unwrap_or_else(|| stream.is_terminal())
    }

    /// Returns `Some(true)` for [`Self::Always`], `Some(false)` for [`Self::Never`], and `None`
    /// for [`Self::Auto`].
    pub const fn as_override(self) -> Option<bool> {
        match self {
            Self::Auto => None,
            Self::Always => Some(true),
            Self::Never => Some(false),
        }
    }
}

fn non_empty(variable: &str) -> bool {
    env::var_os(variable).is_some_and(|value| !value.is_empty())
}
