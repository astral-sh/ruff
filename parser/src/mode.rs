use crate::token::Tok;

#[derive(Clone, Copy)]
pub enum Mode {
    Module,
    Interactive,
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

#[derive(Debug)]
pub struct ModeParseError {
    _priv: (),
}

impl std::fmt::Display for ModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, r#"mode should be "exec", "eval", or "single""#)
    }
}
