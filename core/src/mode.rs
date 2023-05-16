//! Control in the different modes by which a source file can be parsed.

/// The mode argument specifies in what way code must be parsed.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum Mode {
    /// The code consists of a sequence of statements.
    Module,
    /// The code consists of a sequence of interactive statement.
    Interactive,
    /// The code consists of a single expression.
    Expression,
}

impl std::str::FromStr for Mode {
    type Err = ModeParseError;
    fn from_str(s: &str) -> Result<Self, ModeParseError> {
        match s {
            "exec" | "single" => Ok(Mode::Module),
            "eval" => Ok(Mode::Expression),
            _ => Err(ModeParseError),
        }
    }
}

/// Returned when a given mode is not valid.
#[derive(Debug)]
pub struct ModeParseError;

impl std::fmt::Display for ModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, r#"mode must be "exec", "eval", or "single""#)
    }
}
