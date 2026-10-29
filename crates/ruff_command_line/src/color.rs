//! Terminal color selection for the Ruff and ty command-line interfaces.

use std::ffi::OsStr;

/// When to use color in terminal output.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum TerminalColor {
    /// Respect color environment variables and otherwise display colors in interactive terminals.
    #[default]
    Auto,

    /// Always display colors.
    Always,

    /// Never display colors.
    Never,
}

impl TerminalColor {
    /// Resolves `--color` and `FORCE_COLOR` to the mode used for output.
    ///
    /// `always` and `never` take precedence. With `auto` or no flag, a non-empty `FORCE_COLOR`
    /// enables color, an empty value disables it, and an unset variable yields [`Self::Auto`].
    pub fn resolve(color: Option<Self>, force_color: Option<&OsStr>) -> Self {
        match (color.unwrap_or_default(), force_color) {
            (Self::Auto, Some(value)) if value.is_empty() => Self::Never,
            (Self::Auto, Some(_)) => Self::Always,
            (color, _) => color,
        }
    }

    /// Forces color on or off process-wide for the `colored` crate.
    /// [`Self::Auto`] leaves its current setting unchanged.
    pub fn set_colored_override(self) {
        match self {
            Self::Auto => {}
            Self::Always => colored::control::set_override(true),
            Self::Never => colored::control::set_override(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::TerminalColor::{self, Always, Auto, Never};

    /// With `auto` or no flag, distinguish an unset, empty, and non-empty `FORCE_COLOR`.
    #[test]
    fn force_color_env_is_respected() {
        assert_eq!(TerminalColor::resolve(None, None), Auto);
        assert_eq!(TerminalColor::resolve(None, Some(OsStr::new("1"))), Always);
        assert_eq!(TerminalColor::resolve(None, Some(OsStr::new(""))), Never);
        assert_eq!(
            TerminalColor::resolve(Some(Auto), Some(OsStr::new("0"))),
            Always
        );
    }

    /// `--color always` and `--color never` override a conflicting `FORCE_COLOR`.
    #[test]
    fn explicit_color_overrides_force_color() {
        assert_eq!(
            TerminalColor::resolve(Some(Never), Some(OsStr::new("1"))),
            Never
        );
        assert_eq!(
            TerminalColor::resolve(Some(Always), Some(OsStr::new(""))),
            Always
        );
    }
}
