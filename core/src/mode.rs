#[derive(Clone, Copy)]
pub enum Mode {
    Exec,
    Eval,
    Single,
    BlockExpr,
}

impl std::str::FromStr for Mode {
    type Err = ModeParseError;

    // To support `builtins.compile()` `mode` argument
    fn from_str(s: &str) -> Result<Self, ModeParseError> {
        match s {
            "exec" => Ok(Mode::Exec),
            "eval" => Ok(Mode::Eval),
            "single" => Ok(Mode::Single),
            _ => Err(ModeParseError(())),
        }
    }
}

#[derive(Debug)]
pub struct ModeParseError(());

impl std::fmt::Display for ModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, r#"mode must be "exec", "eval", or "single""#)
    }
}
