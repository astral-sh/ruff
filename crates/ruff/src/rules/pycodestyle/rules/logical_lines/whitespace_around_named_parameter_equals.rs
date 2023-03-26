use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;
use rustpython_parser::ast::Location;

use super::LogicalLineTokens;

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
            TokenKind::Async | TokenKind::Indent | TokenKind::Dedent => continue,
            TokenKind::Def => return true,
            _ => return false,
        }
    }

    false
}

/// E251, E252
pub(crate) fn whitespace_around_named_parameter_equals(
    tokens: &LogicalLineTokens,
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut parens = 0u32;
    let mut annotated_func_arg = false;
    let mut prev_end: Option<Location> = None;

    let in_def = is_in_def(tokens);
    let mut iter = tokens.iter().peekable();

    while let Some(token) = iter.next() {
        let kind = token.kind();

        if kind == TokenKind::NonLogicalNewline {
            continue;
        }

        match kind {
            TokenKind::Lpar | TokenKind::Lsqb => {
                parens += 1;
            }
            TokenKind::Rpar | TokenKind::Rsqb => {
                parens -= 1;

                if parens == 0 {
                    annotated_func_arg = false;
                }
            }

            TokenKind::Colon if parens == 1 && in_def => {
                annotated_func_arg = true;
            }
            TokenKind::Comma if parens == 1 => {
                annotated_func_arg = false;
            }
            TokenKind::Equal if parens > 0 => {
                if annotated_func_arg && parens == 1 {
                    let start = token.start();
                    if Some(start) == prev_end {
                        diagnostics.push((start, MissingWhitespaceAroundParameterEquals.into()));
                    }

                    while let Some(next) = iter.peek() {
                        if next.kind() == TokenKind::NonLogicalNewline {
                            iter.next();
                        } else {
                            let next_start = next.start();

                            if next_start == token.end() {
                                diagnostics.push((
                                    next_start,
                                    MissingWhitespaceAroundParameterEquals.into(),
                                ));
                            }
                            break;
                        }
                    }
                } else {
                    if Some(token.start()) != prev_end {
                        diagnostics.push((
                            prev_end.unwrap(),
                            UnexpectedSpacesAroundKeywordParameterEquals.into(),
                        ));
                    }

                    while let Some(next) = iter.peek() {
                        if next.kind() == TokenKind::NonLogicalNewline {
                            iter.next();
                        } else {
                            if next.start() != token.end() {
                                diagnostics.push((
                                    token.end(),
                                    UnexpectedSpacesAroundKeywordParameterEquals.into(),
                                ));
                            }
                            break;
                        }
                    }
                }
            }
            _ => {}
        }

        prev_end = Some(token.end());
    }
    diagnostics
}
