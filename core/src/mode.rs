//! Control in the different modes by which a source file can be parsed.

/// The mode argument specifies in what way code must be parsed.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Mode {
    /// The code consists of a sequence of statements.
    Module,
    /// The code consists of a sequence of interactive statement.
    Interactive,
    /// The code consists of a single expression.
    Expression,
    /// The code consists of a sequence of statements which are part of a
    /// Jupyter Notebook and thus could include escape commands scoped to
    /// a single line.
    ///
    /// ## Limitations:
    ///
    /// For [Dynamic object information], the escape characters (`?`, `??`)
    /// must be used before an object. For example, `?foo` will be recognized,
    /// but `foo?` will not.
    ///
    /// ## Supported escape commands:
    ///
    /// - [Magic command system] which is limited to [line magics] and can start
    ///   with `?` or `??`.
    /// - [Dynamic object information] which can start with `?` or `??`.
    /// - [System shell access] which can start with `!` or `!!`.
    /// - [Automatic parentheses and quotes] which can start with `/`, `;`, or `,`.
    ///
    /// [Magic command system]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#magic-command-system
    /// [line magics]: https://ipython.readthedocs.io/en/stable/interactive/magics.html#line-magics
    /// [Dynamic object information]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#dynamic-object-information
    /// [System shell access]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#system-shell-access
    /// [Automatic parentheses and quotes]: https://ipython.readthedocs.io/en/stable/interactive/reference.html#automatic-parentheses-and-quotes
    Jupyter,
}

impl std::str::FromStr for Mode {
    type Err = ModeParseError;
    fn from_str(s: &str) -> Result<Self, ModeParseError> {
        match s {
            "exec" | "single" => Ok(Mode::Module),
            "eval" => Ok(Mode::Expression),
            "jupyter" => Ok(Mode::Jupyter),
            _ => Err(ModeParseError),
        }
    }
}

/// Returned when a given mode is not valid.
#[derive(Debug)]
pub struct ModeParseError;

impl std::fmt::Display for ModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, r#"mode must be "exec", "eval", "jupyter", or "single""#)
    }
}
