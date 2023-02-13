//! Control in the different modes by which a source file can be parsed.
use crate::token::Tok;

/// The mode argument specifies in what way code must be parsed.
#[derive(Clone, Copy)]
pub enum Mode {
    /// The code consists of a sequence of statements.
    Module,
    /// The code consists of a sequence of interactive statement.
    Interactive,
    /// The code consists of a single expression.
    Expression,
}

impl Mode {
    pub(crate) fn to_marker(self) -> Tok {
        match self {
            Self::Module => Tok::StartModule,
            Self::Interactive => Tok::StartInteractive,
            Self::Expression => Tok::StartExpression,
        }
    }
}

impl From<rustpython_compiler_core::Mode> for Mode {
    fn from(mode: rustpython_compiler_core::Mode) -> Self {
        use rustpython_compiler_core::Mode as CompileMode;
        match mode {
            CompileMode::Exec => Self::Module,
            CompileMode::Eval => Self::Expression,
            CompileMode::Single | CompileMode::BlockExpr => Self::Interactive,
        }
    }
}

impl std::str::FromStr for Mode {
    type Err = ModeParseError;
    fn from_str(s: &str) -> Result<Self, ModeParseError> {
        match s {
            "exec" | "single" => Ok(Mode::Module),
            "eval" => Ok(Mode::Expression),
            _ => Err(ModeParseError { _priv: () }),
        }
    }
}

/// Returned when a given mode is not valid.
#[derive(Debug)]
pub struct ModeParseError {
    _priv: (),
}

impl std::fmt::Display for ModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, r#"mode should be "exec", "eval", or "single""#)
    }
}
