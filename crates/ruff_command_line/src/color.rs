//! Terminal color selection for the Ruff and ty command-line interfaces.

use std::ffi::OsString;

/// When to use color in terminal output.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum TerminalColor {
    /// Only display colors if the output goes to an interactive terminal,
    /// unless a certain environment variable has been set.
    ///
    /// Set `FORCE_COLOR=1` or `CLICOLOR_FORCE=1` to color diagnostics even in a pipe or file.
    /// `FORCE_COLOR` takes precedence over `CLICOLOR_FORCE`: any non-empty `FORCE_COLOR` value
    /// (including `0`) enables colors, while an empty value disables them even with `CLICOLOR_FORCE=1`.
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
    /// Calls `read_force_color` only when `--color` is `auto` or omitted. A non-empty value
    /// enables color, an empty value disables it, and an unset variable yields [`Self::Auto`].
    pub fn resolve(
        color: Option<Self>,
        read_force_color: impl FnOnce() -> Option<OsString>,
    ) -> Self {
        let color = color.unwrap_or_default();
        match color {
            Self::Always | Self::Never => color,
            Self::Auto => match read_force_color() {
                None => Self::Auto,
                Some(value) if value.is_empty() => Self::Never,
                Some(_) => Self::Always,
            },
        }
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::TerminalColor::{self, Always, Auto, Never};

    /// With `auto` or no flag, distinguish an unset, empty, and non-empty `FORCE_COLOR`.
    #[test]
    fn force_color_env_is_respected() {
        assert_eq!(TerminalColor::resolve(None, || None), Auto);
        assert_eq!(
            TerminalColor::resolve(None, || Some(OsString::from("1"))),
            Always
        );
        assert_eq!(
            TerminalColor::resolve(None, || Some(OsString::new())),
            Never
        );
        assert_eq!(
            TerminalColor::resolve(Some(Auto), || Some(OsString::from("0"))),
            Always
        );
    }

    /// `--color always` and `--color never` select the output mode without reading `FORCE_COLOR`.
    #[test]
    fn explicit_color_skips_force_color_lookup() {
        let read_force_color = || panic!("explicit color modes should not read FORCE_COLOR");
        assert_eq!(TerminalColor::resolve(Some(Never), read_force_color), Never);
        assert_eq!(
            TerminalColor::resolve(Some(Always), read_force_color),
            Always
        );
    }
}
