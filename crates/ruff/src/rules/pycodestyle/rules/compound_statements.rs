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

pub fn compound_statements(lxr: &[LexResult]) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    // Track the last seen instance of a variety of tokens.
    let mut colon = None;
    let mut semi = None;
    let mut class = None;
    let mut elif = None;
    let mut else_ = None;
    let mut except = None;
    let mut finally = None;
    let mut for_ = None;
    let mut if_ = None;
    let mut try_ = None;
    let mut while_ = None;
    let mut with = None;

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
                colon = None;
                semi = None;
                class = None;
                elif = None;
                else_ = None;
                except = None;
                finally = None;
                for_ = None;
                if_ = None;
                try_ = None;
                while_ = None;
                with = None;
            }
            Tok::Colon => {
                if class.is_some()
                    || elif.is_some()
                    || else_.is_some()
                    || except.is_some()
                    || finally.is_some()
                    || for_.is_some()
                    || if_.is_some()
                    || try_.is_some()
                    || while_.is_some()
                    || with.is_some()
                {
                    colon = Some((start, end));
                }
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
                    diagnostics.push(Diagnostic::new(
                        MultipleStatementsOnOneLineColon,
                        Range::new(start, end),
                    ));

                    // Reset.
                    colon = None;
                    class = None;
                    elif = None;
                    else_ = None;
                    except = None;
                    finally = None;
                    for_ = None;
                    if_ = None;
                    try_ = None;
                    while_ = None;
                    with = None;
                }
            }
        }

        match tok {
            Tok::Lambda => {
                // Reset.
                colon = None;
                class = None;
                elif = None;
                else_ = None;
                except = None;
                finally = None;
                for_ = None;
                if_ = None;
                try_ = None;
                while_ = None;
                with = None;
            }
            Tok::If => {
                if_ = Some((start, end));
            }
            Tok::While => {
                while_ = Some((start, end));
            }
            Tok::For => {
                for_ = Some((start, end));
            }
            Tok::Try => {
                try_ = Some((start, end));
            }
            Tok::Except => {
                except = Some((start, end));
            }
            Tok::Finally => {
                finally = Some((start, end));
            }
            Tok::Elif => {
                elif = Some((start, end));
            }
            Tok::Else => {
                else_ = Some((start, end));
            }
            Tok::Class => {
                class = Some((start, end));
            }
            Tok::With => {
                with = Some((start, end));
            }
            _ => {}
        };
    }

    diagnostics
}
