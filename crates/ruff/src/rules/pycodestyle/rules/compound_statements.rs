use rustpython_parser::lexer::{LexResult, Tok};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct MultipleStatementsOnOneLineColon;
);
impl Violation for MultipleStatementsOnOneLineColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple statements on one line (colon)")
    }
}

define_violation!(
    pub struct MultipleStatementsOnOneLineSemicolon;
);
impl Violation for MultipleStatementsOnOneLineSemicolon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple statements on one line (semicolon)")
    }
}

define_violation!(
    pub struct UselessSemicolon;
);
impl Violation for UselessSemicolon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Statement ends with an unnecessary semicolon")
    }
}

define_violation!(
    pub struct MultipleStatementsOnOneLineDef;
);
impl Violation for MultipleStatementsOnOneLineDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple statements on one line (def)")
    }
}

pub fn compound_statements(lxr: &[LexResult]) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    // Track the last seen instance of a variety of tokens.
    let mut def = None;
    let mut colon = None;
    let mut semi = None;

    // Track the bracket depth.
    let mut par_count = 0;
    let mut sqb_count = 0;
    let mut brace_count = 0;

    for &(start, ref tok, end) in lxr.iter().flatten() {
        match tok {
            Tok::Lpar => {
                par_count += 1;
            }
            Tok::Rpar => {
                par_count -= 1;
            }
            Tok::Lsqb => {
                sqb_count += 1;
            }
            Tok::Rsqb => {
                sqb_count -= 1;
            }
            Tok::Lbrace => {
                brace_count += 1;
            }
            Tok::Rbrace => {
                brace_count -= 1;
            }
            _ => {}
        }

        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        match tok {
            Tok::Newline => {
                if let Some((start, end)) = semi {
                    diagnostics.push(Diagnostic::new(UselessSemicolon, Range::new(start, end)));
                }

                // Reset.
                def = None;
                colon = None;
                semi = None;
            }
            Tok::Def => {
                def = Some((start, end));
            }
            Tok::Colon => {
                colon = Some((start, end));
            }
            Tok::Semi => {
                semi = Some((start, end));
            }
            Tok::Comment(..) | Tok::Indent | Tok::Dedent | Tok::NonLogicalNewline => {}
            _ => {
                if let Some((start, end)) = semi {
                    diagnostics.push(Diagnostic::new(
                        MultipleStatementsOnOneLineSemicolon,
                        Range::new(start, end),
                    ));

                    // Reset.
                    semi = None;
                }

                if let Some((start, end)) = colon {
                    if let Some((start, end)) = def {
                        diagnostics.push(Diagnostic::new(
                            MultipleStatementsOnOneLineDef,
                            Range::new(start, end),
                        ));
                    } else {
                        diagnostics.push(Diagnostic::new(
                            MultipleStatementsOnOneLineColon,
                            Range::new(start, end),
                        ));
                    }

                    // Reset.
                    def = None;
                    colon = None;
                }
            }
        }
    }

    diagnostics
}
