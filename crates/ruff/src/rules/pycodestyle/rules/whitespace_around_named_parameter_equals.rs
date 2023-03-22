#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[cfg(debug_assertions)]
use crate::rules::pycodestyle::helpers::is_op_token;

#[violation]
pub struct UnexpectedSpacesAroundKeywordParameterEquals;

impl Violation for UnexpectedSpacesAroundKeywordParameterEquals {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexpected spaces around keyword / parameter equals")
    }
}

#[violation]
pub struct MissingWhitespaceAroundParameterEquals;

impl Violation for MissingWhitespaceAroundParameterEquals {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around parameter equals")
    }
}

static STARTSWITH_DEF_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(async\s+def|def)\b").unwrap());

/// E251, E252
#[cfg(debug_assertions)]
pub fn whitespace_around_named_parameter_equals(
    tokens: &[(Location, &Tok, Location)],
    line: &str,
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut parens = 0;
    let mut require_space = false;
    let mut no_space = false;
    let mut annotated_func_arg = false;
    let mut prev_end: Option<&Location> = None;

    let in_def = STARTSWITH_DEF_REGEX.is_match(line);

    for (start, token, end) in tokens {
        if **token == Tok::NonLogicalNewline {
            continue;
        }
        if no_space {
            no_space = false;
            if Some(start) != prev_end {
                diagnostics.push((
                    *(prev_end.unwrap()),
                    UnexpectedSpacesAroundKeywordParameterEquals.into(),
                ));
            }
        }
        if require_space {
            require_space = false;
            if Some(start) == prev_end {
                diagnostics.push((*start, MissingWhitespaceAroundParameterEquals.into()));
            }
        }
        if is_op_token(token) {
            if **token == Tok::Lpar || **token == Tok::Lsqb {
                parens += 1;
            } else if **token == Tok::Rpar || **token == Tok::Rsqb {
                parens -= 1;
            } else if in_def && **token == Tok::Colon && parens == 1 {
                annotated_func_arg = true;
            } else if parens == 1 && **token == Tok::Comma {
                annotated_func_arg = false;
            } else if parens > 0 && **token == Tok::Equal {
                if annotated_func_arg && parens == 1 {
                    require_space = true;
                    if Some(start) == prev_end {
                        diagnostics.push((*start, MissingWhitespaceAroundParameterEquals.into()));
                    }
                } else {
                    no_space = true;
                    if Some(start) != prev_end {
                        diagnostics.push((
                            *(prev_end.unwrap()),
                            UnexpectedSpacesAroundKeywordParameterEquals.into(),
                        ));
                    }
                }
            }

            if parens < 1 {
                annotated_func_arg = false;
            }
        }
        prev_end = Some(end);
    }
    diagnostics
}

#[cfg(not(debug_assertions))]
pub fn whitespace_around_named_parameter_equals(
    _tokens: &[(Location, &Tok, Location)],
    _line: &str,
) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
