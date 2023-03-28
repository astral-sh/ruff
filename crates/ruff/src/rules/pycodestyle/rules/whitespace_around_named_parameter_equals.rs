#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[cfg(feature = "logical_lines")]
use crate::rules::pycodestyle::helpers::is_op_token;
use crate::rules::pycodestyle::logical_lines::LogicalLineTokens;

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

fn is_in_def(tokens: &LogicalLineTokens) -> bool {
    for token in tokens {
        match token.kind() {
            Tok::Async | Tok::Indent | Tok::Dedent => continue,
            Tok::Def => return true,
            _ => return false,
        }
    }

    false
}

/// E251, E252
#[cfg(feature = "logical_lines")]
pub(crate) fn whitespace_around_named_parameter_equals(
    tokens: &LogicalLineTokens,
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut parens = 0;
    let mut require_space = false;
    let mut no_space = false;
    let mut annotated_func_arg = false;
    let mut prev_end: Option<Location> = None;

    let in_def = is_in_def(tokens);

    for token in tokens {
        let kind = token.kind();

        if kind == &Tok::NonLogicalNewline {
            continue;
        }
        if no_space {
            no_space = false;
            if Some(token.start()) != prev_end {
                diagnostics.push((
                    prev_end.unwrap(),
                    UnexpectedSpacesAroundKeywordParameterEquals.into(),
                ));
            }
        }
        if require_space {
            require_space = false;
            let start = token.start();
            if Some(start) == prev_end {
                diagnostics.push((start, MissingWhitespaceAroundParameterEquals.into()));
            }
        }
        if is_op_token(kind) {
            match kind {
                Tok::Lpar | Tok::Lsqb => {
                    parens += 1;
                }
                Tok::Rpar | Tok::Rsqb => {
                    parens -= 1;
                }

                Tok::Colon if parens == 1 && in_def => {
                    annotated_func_arg = true;
                }
                Tok::Comma if parens == 1 => {
                    annotated_func_arg = false;
                }
                Tok::Equal if parens > 0 => {
                    if annotated_func_arg && parens == 1 {
                        require_space = true;
                        let start = token.start();
                        if Some(start) == prev_end {
                            diagnostics
                                .push((start, MissingWhitespaceAroundParameterEquals.into()));
                        }
                    } else {
                        no_space = true;
                        if Some(token.start()) != prev_end {
                            diagnostics.push((
                                prev_end.unwrap(),
                                UnexpectedSpacesAroundKeywordParameterEquals.into(),
                            ));
                        }
                    }
                }
                _ => {}
            }

            if parens < 1 {
                annotated_func_arg = false;
            }
        }
        prev_end = Some(token.end());
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn whitespace_around_named_parameter_equals(
    _tokens: &LogicalLineTokens,
) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
